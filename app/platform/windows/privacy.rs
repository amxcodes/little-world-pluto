//! Lightweight Windows microphone/camera activity observer.
//!
//! Core Audio and Media Foundation provide live microphone/camera state; the
//! Capability Access Manager registry provides application labels when Windows
//! publishes them. No media device is opened and no audio/video is captured.

use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use windows_core::implement;
use windows_startup::Win32::Media::Audio::{
    AudioSessionStateActive, DEVICE_STATE_ACTIVE, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
};
use windows_startup::Win32::Media::MediaFoundation::{
    IMFSensorActivitiesReport, IMFSensorActivitiesReportCallback,
    IMFSensorActivitiesReportCallback_Impl, IMFShutdown, MF_VERSION, MFCreateSensorActivityMonitor,
    MFSTARTUP_FULL, MFShutdown, MFStartup,
};
use windows_startup::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_startup::core::Interface;

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_QWORD, RegCloseKey,
    RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW,
};

const CAPABILITY_ROOT: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
const MAX_APP_KEYS: u32 = 512;
const MAX_REGISTRY_NAME_UNITS: usize = 256;
const MAX_APP_NAMES: usize = 8;
const MAX_LABEL_CHARS: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrivacyActivity {
    pub microphone_apps: Vec<String>,
    pub external_microphone_active: bool,
    pub camera_apps: Vec<String>,
}

impl PrivacyActivity {
    pub fn microphone_active(&self) -> bool {
        !self.microphone_apps.is_empty()
    }

    pub fn camera_active(&self) -> bool {
        !self.camera_apps.is_empty()
    }
}

#[derive(Clone, Copy)]
enum Capability {
    Microphone,
    Camera,
}

impl Capability {
    fn registry_name(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Camera => "webcam",
        }
    }
}

/// Combines live device activity with Windows' optional per-application usage
/// records. An error is returned only when both sources for a capability fail,
/// so the shell can retain its last successful sample.
pub fn read_current_activity() -> Result<PrivacyActivity, u32> {
    let microphone_registry = read_capability(Capability::Microphone);
    let microphone_sessions = read_capture_sessions();
    if microphone_sessions.is_err()
        && let Err(error) = microphone_registry
    {
        return Err(error);
    }
    let (mut microphone_apps, mut external_microphone_active) =
        microphone_registry.unwrap_or_default();
    // Capability Access Manager records are sometimes not updated while a
    // desktop capture stream is open. Core Audio reports the live capture
    // sessions directly, without opening the microphone ourselves.
    if let Ok((active, external)) = microphone_sessions {
        if active && microphone_apps.is_empty() {
            microphone_apps.push("Application".into());
        }
        external_microphone_active |= external;
    }
    let camera_registry = read_capability(Capability::Camera);
    let camera_sensor = read_camera_sensor_activity();
    if camera_sensor.is_err()
        && let Err(error) = camera_registry
    {
        return Err(error);
    }
    let mut camera_apps = camera_registry.unwrap_or_default().0;
    if camera_sensor.unwrap_or(false) && camera_apps.is_empty() {
        camera_apps.push("Application".into());
    }
    Ok(PrivacyActivity {
        microphone_apps,
        external_microphone_active,
        camera_apps,
    })
}

#[implement(IMFSensorActivitiesReportCallback)]
struct CameraActivityCallback {
    sender: SyncSender<bool>,
}

#[allow(non_snake_case)]
impl IMFSensorActivitiesReportCallback_Impl for CameraActivityCallback_Impl {
    fn OnActivitiesReport(
        &self,
        reports: windows_startup::core::Ref<'_, IMFSensorActivitiesReport>,
    ) -> windows_startup::core::Result<()> {
        let reports = reports.ok()?;
        let count = unsafe { reports.GetCount()? };
        if count == 0 {
            return Ok(());
        }
        let mut active = false;
        for index in 0..count.min(32) {
            let report = unsafe { reports.GetActivityReport(index)? };
            for process_index in 0..unsafe { report.GetProcessCount()? }.min(64) {
                let process = unsafe { report.GetProcessActivity(process_index)? };
                active |= unsafe { process.GetStreamingState()?.as_bool() };
            }
        }
        let _ = self.sender.try_send(active);
        Ok(())
    }
}

fn read_camera_sensor_activity() -> windows_startup::core::Result<bool> {
    // The Windows camera sensor monitor reports actual streaming state. A
    // brief bounded wait gets its initial snapshot without opening a camera.
    let _apartment = ComApartment::enter()?;
    unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL)? };
    struct Foundation;
    impl Drop for Foundation {
        fn drop(&mut self) {
            let _ = unsafe { MFShutdown() };
        }
    }
    let _foundation = Foundation;
    let (sender, receiver) = sync_channel(1);
    let callback: IMFSensorActivitiesReportCallback = CameraActivityCallback { sender }.into();
    let monitor = unsafe { MFCreateSensorActivityMonitor(&callback)? };
    unsafe { monitor.Start()? };
    let active = receiver
        .recv_timeout(Duration::from_millis(350))
        .unwrap_or(false);
    let _ = unsafe { monitor.Stop() };
    if let Ok(shutdown) = monitor.cast::<IMFShutdown>() {
        let _ = unsafe { shutdown.Shutdown() };
    }
    Ok(active)
}

fn read_capture_sessions() -> windows_startup::core::Result<(bool, bool)> {
    // SAFETY: the polling worker is a dedicated thread. COM is initialized
    // and uninitialized on that same thread, and all COM pointers stay here.
    unsafe {
        let _apartment = ComApartment::enter()?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        let mut active = false;
        let mut external = false;
        for device_index in 0..devices.GetCount()?.min(16) {
            let Ok(device) = devices.Item(device_index) else {
                continue;
            };
            let Ok(manager): windows_startup::core::Result<IAudioSessionManager2> =
                device.Activate(CLSCTX_ALL, None)
            else {
                continue;
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                continue;
            };
            let Ok(count) = sessions.GetCount() else {
                continue;
            };
            for session_index in 0..count.min(128) {
                let Ok(session) = sessions.GetSession(session_index) else {
                    continue;
                };
                if session.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                active = true;
                external |= session
                    .cast::<IAudioSessionControl2>()
                    .and_then(|session| session.GetProcessId())
                    .map_or(true, |pid| pid != std::process::id());
            }
        }
        Ok((active, external))
    }
}

struct ComApartment;

impl ComApartment {
    fn enter() -> windows_startup::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn read_capability(capability: Capability) -> Result<(Vec<String>, bool), u32> {
    let mut apps = Vec::new();
    let mut external = false;

    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let (mut hive_apps, hive_external) = read_capability_from_hive(hive, capability)?;
        apps.append(&mut hive_apps);
        external |= hive_external;
    }

    apps.sort_by_key(|name| name.to_ascii_lowercase());
    apps.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    apps.truncate(MAX_APP_NAMES);
    Ok((apps, external))
}

fn read_capability_from_hive(
    hive: HKEY,
    capability: Capability,
) -> Result<(Vec<String>, bool), u32> {
    let root_path = format!(r"{CAPABILITY_ROOT}\{}", capability.registry_name());
    let root = match RegistryKey::open(hive, &root_path) {
        Ok(root) => root,
        Err(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) => return Ok((Vec::new(), false)),
        Err(error) => return Err(error),
    };
    let entries = enum_subkeys(root.raw())?;
    let mut apps = Vec::new();
    let mut external = false;

    for entry in entries {
        if entry.eq_ignore_ascii_case("NonPackaged") {
            if let Ok(non_packaged) = RegistryKey::open(root.raw(), &entry) {
                for app_key in enum_subkeys(non_packaged.raw())? {
                    read_app_entry(non_packaged.raw(), &app_key, &mut apps, &mut external);
                }
            }
        } else {
            read_app_entry(root.raw(), &entry, &mut apps, &mut external);
        }
    }

    Ok((apps, external))
}

fn read_app_entry(parent: HKEY, key_name: &str, apps: &mut Vec<String>, external: &mut bool) {
    let Ok(app_key) = RegistryKey::open(parent, key_name) else {
        // Per-application keys can disappear while an app exits. Skip that
        // race; the next bounded sample will reconcile the current state.
        return;
    };
    let Some(start) = query_qword(app_key.raw(), "LastUsedTimeStart") else {
        return;
    };
    let stop = query_qword(app_key.raw(), "LastUsedTimeStop").unwrap_or_default();
    if capability_is_active(start, stop)
        && let Some(name) = application_label(key_name)
    {
        let path = key_name.replace('#', "\\").to_ascii_lowercase();
        let own_executable = std::env::current_exe()
            .ok()
            .is_some_and(|exe| exe.to_string_lossy().to_ascii_lowercase() == path);
        let own_package = key_name
            .to_ascii_lowercase()
            .starts_with("littleworld.pluto.desktop_");
        if !own_executable && !own_package {
            *external = true;
        }
        apps.push(name);
    }
}

fn capability_is_active(start: u64, stop: u64) -> bool {
    start != 0 && (stop == 0 || start > stop)
}

fn application_label(key_name: &str) -> Option<String> {
    let executable = key_name
        .rsplit(['\\', '/', '#'])
        .next()
        .unwrap_or(key_name)
        .trim();
    let package = executable
        .split_once('_')
        .map_or(executable, |(name, _)| name);
    let stem = package
        .strip_suffix(".exe")
        .or_else(|| package.strip_suffix(".EXE"))
        .unwrap_or(package);
    let mut label = String::with_capacity(stem.len().min(MAX_LABEL_CHARS));
    let mut previous_space = true;
    for character in stem.chars() {
        let character = if character.is_control() || matches!(character, '.' | '_' | '-') {
            ' '
        } else {
            character
        };
        if character.is_whitespace() {
            if !previous_space {
                label.push(' ');
                previous_space = true;
            }
        } else if character.is_alphanumeric() {
            if label.chars().count() >= MAX_LABEL_CHARS {
                break;
            }
            label.push(character);
            previous_space = false;
        }
    }
    let label = label.trim().to_owned();
    (!label.is_empty()).then_some(label)
}

fn enum_subkeys(key: HKEY) -> Result<Vec<String>, u32> {
    let mut names = Vec::new();
    for index in 0..MAX_APP_KEYS {
        let mut buffer = [0_u16; MAX_REGISTRY_NAME_UNITS];
        let mut length = u32::try_from(buffer.len() - 1).unwrap_or(u32::MAX);
        // SAFETY: the buffer and length pointer are valid for the duration of
        // the call; unsupported output fields are passed as null.
        let result = unsafe {
            RegEnumKeyExW(
                key,
                index,
                buffer.as_mut_ptr(),
                &mut length,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if result == ERROR_NO_MORE_ITEMS {
            return Ok(names);
        }
        if result != 0 {
            return Err(result);
        }
        names.push(String::from_utf16_lossy(&buffer[..length as usize]));
    }
    Ok(names)
}

fn query_qword(key: HKEY, value_name: &str) -> Option<u64> {
    let value_name: Vec<u16> = value_name.encode_utf16().chain([0]).collect();
    let mut value = 0_u64;
    let mut size = size_of::<u64>() as u32;
    let mut kind = 0;
    // SAFETY: outputs are correctly sized; the returned registry type and
    // byte count are checked before the value is used.
    let result = unsafe {
        RegQueryValueExW(
            key,
            value_name.as_ptr(),
            null(),
            &mut kind,
            (&mut value as *mut u64).cast(),
            &mut size,
        )
    };
    (result == 0 && kind == REG_QWORD && size == size_of::<u64>() as u32).then_some(value)
}

struct RegistryKey(HKEY);

impl RegistryKey {
    fn open(parent: HKEY, subkey: &str) -> Result<Self, u32> {
        let path: Vec<u16> = subkey.encode_utf16().chain([0]).collect();
        let mut key = null_mut();
        // SAFETY: the path is NUL-terminated and the output pointer is valid.
        let view = if parent == HKEY_LOCAL_MACHINE {
            KEY_WOW64_64KEY
        } else {
            0
        };
        let result = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ | view, &mut key) };
        if result == 0 {
            Ok(Self(key))
        } else {
            Err(result)
        }
    }

    fn raw(&self) -> HKEY {
        self.0
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        // SAFETY: this key was returned successfully by RegOpenKeyExW and is
        // closed once when its owner is dropped.
        unsafe { RegCloseKey(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::{application_label, capability_is_active};

    #[test]
    fn usage_requires_a_new_start_after_the_last_stop() {
        assert!(!capability_is_active(0, 0));
        assert!(capability_is_active(12, 0));
        assert!(capability_is_active(12, 11));
        assert!(!capability_is_active(11, 12));
        assert!(!capability_is_active(0, 12));
    }

    #[test]
    fn app_labels_hide_install_paths_and_bound_untrusted_text() {
        assert_eq!(
            application_label(r"C:\Program Files\Microsoft Teams\Teams.exe"),
            Some("Teams".into())
        );
        assert_eq!(
            application_label("MicrosoftTeams_123!abc"),
            Some("MicrosoftTeams".into())
        );
        assert_eq!(application_label("C:#apps#Zoom.exe"), Some("Zoom".into()));
        assert_eq!(application_label("   "), None);
        assert!(
            application_label(&format!("{}.exe", "x".repeat(100)))
                .is_some_and(|name| name.len() <= super::MAX_LABEL_CHARS)
        );
    }
}
