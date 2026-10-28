//! Opt-in, bounded text expansion. The hook observes only a possible trigger,
//! never stores ordinary typing, and does no work while expansion is disabled.
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VK_BACK, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GUITHREADINFO, GWL_STYLE, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowLongPtrW, GetWindowThreadProcessId, HHOOK, KBDLLHOOKSTRUCT, PostMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

pub const EXPAND_MESSAGE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 0x56;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub trigger: String,
    pub replacement: String,
}

#[derive(Default)]
struct Runtime {
    window: isize,
    hook: isize,
    entries: Vec<Entry>,
    typed: String,
    foreground: isize,
    pending: Option<(usize, String, isize)>,
}
static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();
fn runtime() -> &'static Mutex<Runtime> {
    RUNTIME.get_or_init(|| Mutex::new(Runtime::default()))
}

pub fn settings_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("Pluto")
            .join("text-expander.json")
    })
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub entries: Vec<Entry>,
}

pub fn load() -> Settings {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| bytes.len() <= 256_000)
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> bool {
    let Some(path) = settings_path() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    std::fs::create_dir_all(parent).is_ok()
        && serde_json::to_vec(settings)
            .ok()
            .is_some_and(|bytes| std::fs::write(path, bytes).is_ok())
}

pub fn valid(entry: &Entry) -> bool {
    let trigger = entry.trigger.as_bytes();
    trigger.len() >= 2
        && trigger.len() <= 24
        && trigger[0] == b'!'
        && trigger[1..].iter().all(u8::is_ascii_alphanumeric)
        && !entry.replacement.is_empty()
        && entry.replacement.chars().count() <= 2000
}

pub fn configure(window: HWND, settings: &Settings) -> bool {
    let mut state = runtime().lock().unwrap();
    state.window = window as isize;
    state.entries = settings
        .entries
        .iter()
        .filter(|entry| valid(entry))
        .cloned()
        .collect();
    state.typed.clear();
    state.pending = None;
    if settings.enabled && !state.entries.is_empty() && state.hook == 0 {
        state.hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                GetModuleHandleW(null_mut()),
                0,
            )
        } as isize;
    } else if (!settings.enabled || state.entries.is_empty()) && state.hook != 0 {
        unsafe { UnhookWindowsHookEx(state.hook as HHOOK) };
        state.hook = 0;
    }
    !settings.enabled || state.entries.is_empty() || state.hook != 0
}

pub fn stop() {
    let mut state = runtime().lock().unwrap();
    if state.hook != 0 {
        unsafe { UnhookWindowsHookEx(state.hook as HHOOK) };
    }
    *state = Runtime::default();
}

fn focused_native_password_field(window: HWND) -> bool {
    let mut process = 0;
    let thread = unsafe { GetWindowThreadProcessId(window, &mut process) };
    if thread == 0 {
        return false;
    }
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    (unsafe { GetGUIThreadInfo(thread, &mut info) }) != 0
        && !info.hwndFocus.is_null()
        && unsafe { GetWindowLongPtrW(info.hwndFocus, GWL_STYLE) } & 0x20 != 0 // ES_PASSWORD
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 || (wparam as u32 != WM_KEYDOWN && wparam as u32 != WM_SYSKEYDOWN) {
        return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
    }
    let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    if key.flags & 0x10 != 0 {
        return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
    } // injected
    let foreground = unsafe { GetForegroundWindow() } as isize;
    let mut state = runtime().lock().unwrap();
    if foreground != state.foreground {
        state.typed.clear();
        state.foreground = foreground;
    }
    if unsafe { GetAsyncKeyState(VK_CONTROL as i32) } < 0
        || unsafe { GetAsyncKeyState(VK_MENU as i32) } < 0
    {
        state.typed.clear();
    } else if key.vkCode == VK_BACK as u32 {
        state.typed.pop();
    } else if key.vkCode == 0x20 {
        if focused_native_password_field(foreground as HWND) {
            state.typed.clear();
            return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
        }
        if let Some((length, replacement)) = state.entries.iter().find_map(|entry| {
            state
                .typed
                .ends_with(&entry.trigger.to_ascii_lowercase())
                .then(|| (entry.trigger.len(), entry.replacement.clone()))
        }) {
            state.typed.clear();
            state.pending = Some((length, replacement, foreground));
            if unsafe { PostMessageW(state.window as HWND, EXPAND_MESSAGE, 0, 0) } != 0 {
                return 1;
            }
            state.pending = None;
        } else {
            state.typed.clear();
        }
    } else if (b'A' as u32..=b'Z' as u32).contains(&key.vkCode) {
        state
            .typed
            .push((key.vkCode as u8 as char).to_ascii_lowercase());
    } else if key.vkCode == b'1' as u32 && unsafe { GetAsyncKeyState(VK_SHIFT as i32) } < 0 {
        state.typed.push('!');
    } else if (b'0' as u32..=b'9' as u32).contains(&key.vkCode) {
        state.typed.push(key.vkCode as u8 as char);
    } else {
        state.typed.clear();
    }
    if state.typed.len() > 24 {
        state.typed.clear();
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

pub fn expand_pending() {
    let pending = runtime().lock().unwrap().pending.take();
    let Some((length, replacement, foreground)) = pending else {
        return;
    };
    if unsafe { GetForegroundWindow() } as isize != foreground {
        return;
    }
    let mut inputs = Vec::with_capacity((length + replacement.len() + 1) * 2);
    for _ in 0..length {
        inputs.push(key(VK_BACK, 0, false));
        inputs.push(key(VK_BACK, 0, true));
    }
    for unit in replacement.encode_utf16().chain([b' ' as u16]) {
        inputs.push(key(0, unit, false));
        inputs.push(key(0, unit, true));
    }
    // SendInput may reject an oversized array; small batches also keep the UI responsive.
    for batch in inputs.chunks(64) {
        unsafe {
            SendInput(
                batch.len() as u32,
                batch.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
    }
}

fn key(virtual_key: u16, scan: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: scan,
                dwFlags: (if scan != 0 { KEYEVENTF_UNICODE } else { 0 })
                    | (if up { KEYEVENTF_KEYUP } else { 0 }),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, valid};
    #[test]
    fn rejects_unsafe_triggers_and_empty_expansions() {
        assert!(valid(&Entry {
            trigger: "!AMN".into(),
            replacement: "A longer note".into()
        }));
        assert!(!valid(&Entry {
            trigger: "AMN".into(),
            replacement: "text".into()
        }));
        assert!(!valid(&Entry {
            trigger: "!A M".into(),
            replacement: "text".into()
        }));
        assert!(!valid(&Entry {
            trigger: "!A".into(),
            replacement: String::new()
        }));
    }
}
