//! Lightweight Windows microphone/camera activity observer.
//!
//! This reads only the current user's Windows capability-use timestamps. It
//! never opens a media device or captures audio/video. The registry location
//! and active timestamp rule are based on the reverse-engineered Pluto
//! reference recorded in `ISLAND_WINDOWING_RESEARCH.md`.

use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_QWORD, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    RegQueryValueExW,
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

/// Returns the applications Windows currently reports as using either
/// capability. Missing keys mean no active use; access failures are returned
/// so the caller can retain its last successful sample instead of flashing
/// false activity during a transient registry error.
pub fn read_current_activity() -> Result<PrivacyActivity, u32> {
    let (microphone_apps, external_microphone_active) = read_capability(Capability::Microphone)?;
    Ok(PrivacyActivity {
        microphone_apps,
        external_microphone_active,
        camera_apps: read_capability(Capability::Camera)?.0,
    })
}

fn read_capability(capability: Capability) -> Result<(Vec<String>, bool), u32> {
    let root_path = format!(r"{CAPABILITY_ROOT}\{}", capability.registry_name());
    let root = match RegistryKey::open(HKEY_CURRENT_USER, &root_path) {
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

    apps.sort_by_key(|name| name.to_ascii_lowercase());
    apps.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    apps.truncate(MAX_APP_NAMES);
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
        let result = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ, &mut key) };
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
