//! Packaged-app startup control with an authoritative Windows readback.

use windows_startup::ApplicationModel::{StartupTask, StartupTaskState as WinStartupTaskState};
use windows_startup::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
use windows_startup::core::HSTRING;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use std::ptr::{null, null_mut};

const TASK_ID: &str = "IslandDesktopStartup";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupTaskStatus {
    Checking,
    Enabled,
    Disabled,
    DisabledByUser,
    DisabledByPolicy,
    EnabledByPolicy,
    RegistryEnabled,
    RegistryDisabled,
    Unavailable,
    Failed,
}

impl StartupTaskStatus {
    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled | Self::EnabledByPolicy | Self::RegistryEnabled)
    }

    pub fn can_change(self) -> bool {
        matches!(self, Self::Enabled | Self::Disabled | Self::RegistryEnabled | Self::RegistryDisabled)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Checking => "Reading Windows startup state…",
            Self::Enabled => "Enabled in Windows. Island starts when you sign in.",
            Self::Disabled => "Disabled in Windows.",
            Self::DisabledByUser => {
                "Windows disabled this app at your request. Re-enable it in Startup settings."
            }
            Self::DisabledByPolicy => "Startup is disabled by Windows policy.",
            Self::EnabledByPolicy => "Startup is enabled by Windows policy.",
            Self::RegistryEnabled => "Enabled for this user. Island starts when you sign in.",
            Self::RegistryDisabled => "Disabled for this user.",
            Self::Unavailable => {
                "Windows is not exposing a registered startup task. Install the package and check Windows Startup settings."
            }
            Self::Failed => "Windows could not read or change the startup task. Retry the check.",
        }
    }

    fn from_windows(state: WinStartupTaskState) -> Self {
        if state == WinStartupTaskState::Enabled {
            Self::Enabled
        } else if state == WinStartupTaskState::Disabled {
            Self::Disabled
        } else if state == WinStartupTaskState::DisabledByUser {
            Self::DisabledByUser
        } else if state == WinStartupTaskState::DisabledByPolicy {
            Self::DisabledByPolicy
        } else if state == WinStartupTaskState::EnabledByPolicy {
            Self::EnabledByPolicy
        } else {
            Self::Failed
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupTaskError {
    Unavailable,
    Failed,
}

impl StartupTaskError {
    pub fn status(self) -> StartupTaskStatus {
        match self {
            Self::Unavailable => StartupTaskStatus::Unavailable,
            Self::Failed => StartupTaskStatus::Failed,
        }
    }
}

pub fn read_startup_task() -> Result<StartupTaskStatus, StartupTaskError> {
    match with_startup_task(|task| {
        task.State()
            .map(StartupTaskStatus::from_windows)
            .map_err(|_| StartupTaskError::Failed)
    }) {
        Err(StartupTaskError::Unavailable) => read_run_entry(),
        result => result,
    }
}

pub fn set_startup_task(enabled: bool) -> Result<StartupTaskStatus, StartupTaskError> {
    match with_startup_task(|task| {
        if enabled {
            let _effective_state = task
                .RequestEnableAsync()
                .map_err(|_| StartupTaskError::Failed)?
                .join()
                .map_err(|_| StartupTaskError::Failed)?;
        } else {
            task.Disable().map_err(|_| StartupTaskError::Failed)?;
        }
        task.State()
            .map(StartupTaskStatus::from_windows)
            .map_err(|_| StartupTaskError::Failed)
    }) {
        Err(StartupTaskError::Unavailable) => set_run_entry(enabled),
        result => result,
    }
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "LittleWorldIsland";

fn read_run_entry() -> Result<StartupTaskStatus, StartupTaskError> {
    let key_path = wide(RUN_KEY);
    let value_name = wide(RUN_VALUE);
    let mut key: HKEY = null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_path.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if opened == ERROR_FILE_NOT_FOUND {
        return Ok(StartupTaskStatus::RegistryDisabled);
    }
    if opened != ERROR_SUCCESS {
        return Err(StartupTaskError::Failed);
    }

    let mut value_type = 0;
    let mut byte_count = 0;
    let queried = unsafe {
        RegQueryValueExW(
            key,
            value_name.as_ptr(),
            null(),
            &mut value_type,
            null_mut(),
            &mut byte_count,
        )
    };
    unsafe { RegCloseKey(key) };
    match queried {
        ERROR_SUCCESS if value_type == REG_SZ && byte_count > 0 => {
            Ok(StartupTaskStatus::RegistryEnabled)
        }
        ERROR_FILE_NOT_FOUND => Ok(StartupTaskStatus::RegistryDisabled),
        _ => Err(StartupTaskError::Failed),
    }
}

fn set_run_entry(enabled: bool) -> Result<StartupTaskStatus, StartupTaskError> {
    let key_path = wide(RUN_KEY);
    let value_name = wide(RUN_VALUE);
    if enabled {
        let executable = std::env::current_exe().map_err(|_| StartupTaskError::Failed)?;
        let command = wide(&format!("\"{}\"", executable.display()));
        let byte_count = u32::try_from(command.len() * std::mem::size_of::<u16>())
            .map_err(|_| StartupTaskError::Failed)?;
        let mut key: HKEY = null_mut();
        let opened = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_path.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut(),
            )
        };
        if opened != ERROR_SUCCESS {
            return Err(StartupTaskError::Failed);
        }
        let written = unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                byte_count,
            )
        };
        unsafe { RegCloseKey(key) };
        if written != ERROR_SUCCESS {
            return Err(StartupTaskError::Failed);
        }
        return read_run_entry();
    }

    let mut key: HKEY = null_mut();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_path.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if opened == ERROR_FILE_NOT_FOUND {
        return Ok(StartupTaskStatus::RegistryDisabled);
    }
    if opened != ERROR_SUCCESS {
        return Err(StartupTaskError::Failed);
    }
    let deleted = unsafe { RegDeleteValueW(key, value_name.as_ptr()) };
    unsafe { RegCloseKey(key) };
    if deleted == ERROR_SUCCESS || deleted == ERROR_FILE_NOT_FOUND {
        Ok(StartupTaskStatus::RegistryDisabled)
    } else {
        Err(StartupTaskError::Failed)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn with_startup_task(
    operation: impl FnOnce(&StartupTask) -> Result<StartupTaskStatus, StartupTaskError>,
) -> Result<StartupTaskStatus, StartupTaskError> {
    // This worker thread owns its WinRT apartment for the duration of the call.
    // Never wait for StartupTask's async operation on the UI thread.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|_| StartupTaskError::Failed)?;
    let _apartment = WinrtApartment;
    let task_id = HSTRING::from(TASK_ID);
    let task = StartupTask::GetAsync(&task_id)
        .and_then(|operation| operation.join())
        .map_err(|_| StartupTaskError::Unavailable)?;
    operation(&task)
}

struct WinrtApartment;

impl Drop for WinrtApartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_state_only_shows_effective_enabled_value() {
        assert!(StartupTaskStatus::Enabled.is_enabled());
        assert!(StartupTaskStatus::EnabledByPolicy.is_enabled());
        assert!(!StartupTaskStatus::Disabled.is_enabled());
        assert!(!StartupTaskStatus::DisabledByUser.is_enabled());
        assert!(!StartupTaskStatus::DisabledByPolicy.is_enabled());
    }

    #[test]
    fn user_and_policy_states_cannot_be_overridden_in_the_app_ui() {
        assert!(StartupTaskStatus::Enabled.can_change());
        assert!(StartupTaskStatus::Disabled.can_change());
        assert!(!StartupTaskStatus::DisabledByUser.can_change());
        assert!(!StartupTaskStatus::DisabledByPolicy.can_change());
        assert!(!StartupTaskStatus::EnabledByPolicy.can_change());
        assert!(!StartupTaskStatus::Unavailable.can_change());
    }

    #[test]
    fn startup_api_errors_have_honest_user_visible_states() {
        assert_eq!(
            StartupTaskError::Unavailable.status(),
            StartupTaskStatus::Unavailable
        );
        assert_eq!(StartupTaskError::Failed.status(), StartupTaskStatus::Failed);
    }
}
