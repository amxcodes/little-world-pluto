use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::size_of;
use std::num::NonZeroIsize;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::shortcuts::{
    MOD_ALT, MOD_CONTROL, MOD_SHIFT, SHORTCUT_COUNT, ShortcutAction, ShortcutSet,
};
use eframe::egui;
use island_desktop::motion::{COLLAPSE_SPRING, CONTENT_EXIT_DURATION, Spring, SpringMotion};
use island_desktop::shell_state::{ShellEvent, ShellMode, ShellReducer};
use island_desktop::visual::{
    EXPANDED_HEIGHT, FOLDED_HEIGHT, ShapeMode, ShellLayout, centered_pixel_origin,
    centered_pixel_width,
};
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use windows_core::implement;
use windows_startup::Win32::System::Ole::{IDropSource, IDropTarget};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CLASS_ALREADY_EXISTS, ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE, HWND,
    LPARAM, LRESULT, POINT, RECT, SIZE, SYSTEMTIME, SetLastError, WAIT_FAILED, WAIT_OBJECT_0,
    WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{DWM_TIMING_INFO, DwmGetCompositionTimingInfo};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    CLIP_DEFAULT_PRECIS, CombineRgn, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreateRectRgn, CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, FF_DONTCARE, FW_SEMIBOLD, FillRgn, GetDIBits,
    GetObjectW, GetTextExtentPoint32W, HBITMAP, HBRUSH, HDC, HGDIOBJ, HRGN, OUT_DEFAULT_PRECIS,
    RGN_OR, SelectObject, SetBkMode, SetTextAlign, SetTextColor, SetWindowRgn, TA_CENTER, TA_TOP,
    TRANSPARENT, TextOutW,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, SYSTEM_POWER_STATUS, SetSuspendState,
};
use windows_sys::Win32::System::Shutdown::LockWorkStation;
use windows_sys::Win32::System::SystemInformation::{
    GetLocalTime, GetSystemDirectoryW, OSVERSIONINFOW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CancelWaitableTimer, CreateWaitableTimerExW,
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SetWaitableTimer, TIMER_ALL_ACCESS,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    UnregisterHotKey, VK_LBUTTON,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUT, RAWINPUTDEVICE, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, SHFILEINFOW, SHGFI_ICON, SHGetFileInfoW,
    ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DI_NORMAL, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW,
    DrawIconEx, EnumChildWindows, GWLP_USERDATA, GetClassNameW, GetCursorPos, GetForegroundWindow,
    GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HTTRANSPARENT,
    HWND_TOPMOST, IsChild, IsWindow, IsZoomed, KillTimer, LWA_ALPHA, MB_OK, MSG,
    MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostMessageW,
    PostQuitMessage, QS_ALLINPUT, RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP,
    RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetForegroundWindow, SetLayeredWindowAttributes, SetTimer,
    SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    ULW_ALPHA, UpdateLayeredWindow, WM_APP, WM_INPUT, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_NCCREATE, WM_NCHITTEST, WM_QUIT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WindowFromPoint,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SW_SHOW, ShowWindowAsync};

const CLASS_NAME: &str = "IslandDesktopEdgeShell";

fn wake_sensitivity() -> crate::platform::windows::wake_word::Sensitivity {
    let preferences = load_agent_preferences().unwrap_or_default();
    crate::platform::windows::wake_word::Sensitivity::from_name(
        preferences["wakeSensitivity"]
            .as_str()
            .unwrap_or("balanced"),
    )
}

fn agent_preferences_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        std::path::PathBuf::from(root)
            .join("Island")
            .join("agent-preferences.json")
    })
}

fn load_agent_preferences() -> Option<serde_json::Value> {
    let path = agent_preferences_path()?;
    if std::fs::metadata(&path).ok()?.len() > 64_000 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    value.as_object()?;
    Some(value)
}

fn save_agent_preferences(value: &serde_json::Value) {
    if !matches!(
        value["defaultProvider"].as_str(),
        Some("opencode" | "codex")
    ) || !value["providerSetupComplete"].is_boolean()
    {
        return;
    }
    let Ok(encoded) = serde_json::to_vec(value) else {
        return;
    };
    if encoded.len() > 64_000 {
        return;
    }
    let Some(path) = agent_preferences_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = std::fs::write(path, encoded);
}

fn pick_opencode_folder(owner: HWND) -> Option<String> {
    use windows_startup::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree,
    };
    use windows_startup::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
    };
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()? };
    let flags = unsafe { dialog.GetOptions().ok()? };
    unsafe {
        dialog
            .SetOptions(flags | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog
            .Show(Some(windows_startup::Win32::Foundation::HWND(owner)))
            .ok()?;
    }
    let item = unsafe { dialog.GetResult().ok()? };
    let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH).ok()? };
    let text = unsafe { path.to_string().ok() };
    unsafe {
        CoTaskMemFree(Some(path.0.cast()));
    }
    text
}
fn pick_opencode_files(owner: HWND) -> Option<Vec<String>> {
    use windows_startup::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree,
    };
    use windows_startup::Win32::UI::Shell::{
        FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, FileOpenDialog, IFileOpenDialog,
        SIGDN_FILESYSPATH,
    };
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()? };
    let flags = unsafe { dialog.GetOptions().ok()? };
    unsafe {
        dialog
            .SetOptions(flags | FOS_ALLOWMULTISELECT | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog
            .Show(Some(windows_startup::Win32::Foundation::HWND(owner)))
            .ok()?;
    }
    let items = unsafe { dialog.GetResults().ok()? };
    let count = unsafe { items.GetCount().ok()? }.min(8);
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let item = unsafe { items.GetItemAt(index).ok()? };
        let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH).ok()? };
        let text = unsafe { path.to_string().ok() };
        unsafe {
            CoTaskMemFree(Some(path.0.cast()));
        }
        if let Some(text) = text {
            paths.push(text);
        }
    }
    Some(paths)
}
const DROP_CATCHER_CLASS_NAME: &str = "IslandDesktopDropCatcher";
const WINDOW_TITLE: &str = "Island edge shell";
// Coverage is analytically antialiased from the contour's signed distance,
// so one center sample per edge pixel is sufficient at every device scale.
// Interior pixels still use the scanline fast path; only contour pixels need
// fractional coverage, including while the shell moves.
const ANIMATION_COVERAGE_SAMPLES: usize = 4;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_HOTKEY: u32 = 0x0312;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_DPICHANGED: u32 = 0x02E0;
const MAX_SHELL_SURFACE_BYTES: usize = 64 * 1024 * 1024;
const MAX_EXPANDED_BODY_HEIGHT: f64 = 640.0;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x0000_0011;
// Home's compact first-open layout: 43 px navigation + 190 px grid + two
// 6 px gaps + 42 px output and shelf rows + 22 px body padding + 32 px shell inset.
const INITIAL_HOME_EXPANDED_BODY_HEIGHT: f64 = 383.0;
const HOME_MEDIA_HEIGHT_DELTA: f64 = 18.0;
const PROFILE_PANEL_EXTRA_WIDTH: f64 = 112.0;
const SCREEN_FEEDBACK_ENTER: std::time::Duration = std::time::Duration::from_millis(130);
const SCREEN_FEEDBACK_EXIT: std::time::Duration = std::time::Duration::from_millis(120);
const COMPACT_FEEDBACK_CAPACITY: usize = 12;
const COMPACT_FEEDBACK_MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
const REACT_CONTENT_REVEAL_MARGIN: f64 = 36.0;
const REACT_COMPACT_RENDER_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(240);
const IN_PLACE_LAYOUT_SPRING: Spring = Spring {
    stiffness: 420.0,
    damping: 42.0,
    mass: 1.0,
};
// Small status-width changes settle with the 260 ms React handoff. Critical
// damping gives the contour a soft arrival without bouncing its pixel edge.
const COMPACT_STATUS_SPRING: Spring = Spring {
    stiffness: 900.0,
    damping: 60.0,
    mass: 1.0,
};
// Keep the shell's first response quick, with slight overdamping to prevent
// the 1 px overshoot that was visible at the end of the contour transition.
const SHELL_OPEN_SPRING: Spring = Spring {
    stiffness: 540.0,
    damping: 51.0,
    mass: 1.0,
};
const SHELL_COLLAPSE_SPRING: Spring = Spring {
    stiffness: 640.0,
    damping: 56.0,
    mass: 1.0,
};
const MA_NOACTIVATE: LRESULT = 3;
const WM_DROPFILES: u32 = 0x0233;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const HOLD_SPACE_TIMER_ID: usize = 0x1D59;
const NOTE_SAVE_TIMER_ID: usize = 0x1D54;
const WEATHER_POLL_TIMER_ID: usize = 0x1D55;
const HOTKEY_ID_BASE: i32 = 0x1D51;
const HOTKEY_MOD_NOREPEAT: u32 = 0x4000;
const WM_APP_SHELL_TICK: u32 = WM_APP + 0x51;
const WM_APP_SHELL_COMMAND: u32 = WM_APP + 0x52;
const WM_APP_SHELL_STOP: u32 = WM_APP + 0x53;
const WM_APP_REACT: u32 = WM_APP + 0x54;
const WM_APP_SCREEN_OVERLAY_DONE: u32 = WM_APP + 0x55;
const FOCUS_RESTORE_TIMER_ID: usize = 0x1D53;
const FULLSCREEN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const FOCUS_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_millis(150);
const SHELF_PEEK_SIZE: (f64, f64) = (360.0, 86.0);
const LIGHT_SHELL_BACKGROUND: (u8, u8, u8) = (246, 246, 247);
const DRAG_HOVER_POLL: std::time::Duration = std::time::Duration::from_millis(24);
const HOVER_INTENT_DWELL: std::time::Duration = std::time::Duration::from_millis(90);
const DROP_HANDOFF_DURATION: std::time::Duration = std::time::Duration::from_secs(2);

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version_information: *mut OSVERSIONINFOW) -> i32;
}

fn supports_screen_capture_exclusion() -> bool {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // RtlGetVersion reports the real OS build without compatibility-manifest
    // version virtualization. WDA_EXCLUDEFROMCAPTURE is only true exclusion
    // starting with Windows 10 2004 (build 19041); earlier builds show black.
    (unsafe { RtlGetVersion(&mut version) >= 0 })
        && (version.dwMajorVersion > 10
            || (version.dwMajorVersion == 10 && version.dwBuildNumber >= 19_041))
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct NoteFolder {
    name: String,
    text: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct NoteBook {
    folders: Vec<NoteFolder>,
    selected: usize,
    reminder_at: Option<u64>,
}

impl NoteBook {
    fn selected_text(&self) -> String {
        self.folders
            .get(self.selected)
            .map(|folder| folder.text.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, serde::Serialize)]
struct MediaSessionOption {
    id: String,
    source: String,
    playing: bool,
}

#[derive(Clone, serde::Serialize)]
struct MediaSnapshot {
    session_id: String,
    sessions: Vec<MediaSessionOption>,
    title: String,
    artist: String,
    source: String,
    playing: bool,
    #[serde(skip_serializing)]
    artwork: Option<Arc<str>>,
    #[serde(skip_serializing)]
    artwork_revision: u64,
    position_ms: Option<u64>,
    duration_ms: Option<u64>,
    can_toggle: bool,
    can_previous: bool,
    can_next: bool,
    can_seek: bool,
}

impl PartialEq for MediaSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.sessions.len() == other.sessions.len()
            && self
                .sessions
                .iter()
                .zip(&other.sessions)
                .all(|(left, right)| {
                    left.id == right.id
                        && left.source == right.source
                        && left.playing == right.playing
                })
            && self.title == other.title
            && self.artist == other.artist
            && self.source == other.source
            && self.playing == other.playing
            && self.artwork_revision == other.artwork_revision
            && self.position_ms == other.position_ms
            && self.duration_ms == other.duration_ms
            && self.can_toggle == other.can_toggle
            && self.can_previous == other.can_previous
            && self.can_next == other.can_next
            && self.can_seek == other.can_seek
    }
}

enum MediaAction {
    Refresh,
    Toggle,
    Previous,
    Next,
    SeekToMs(u64),
    Select(String),
}

fn queue_media_refresh(sender: &Sender<MediaAction>, pending: &AtomicBool) {
    if pending
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
        && sender.send(MediaAction::Refresh).is_err()
    {
        pending.store(false, Ordering::Release);
    }
}

fn media_artwork(
    properties: &windows_startup::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Arc<str>> {
    use base64::Engine;
    use windows_startup::Storage::Streams::DataReader;
    let stream = properties
        .Thumbnail()
        .ok()?
        .OpenReadAsync()
        .ok()?
        .join()
        .ok()?;
    let mime = stream
        .ContentType()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let length = usize::try_from(stream.Size().ok()?).ok()?;
    if length == 0 || length > 2 * 1024 * 1024 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    let loaded = reader.LoadAsync(length as u32).ok()?.join().ok()? as usize;
    if loaded == 0 || loaded > length {
        return None;
    }
    let mut bytes = vec![0; loaded];
    reader.ReadBytes(&mut bytes).ok()?;
    // Browser SMTC thumbnails are commonly backed by an in-memory stream
    // whose ContentType is empty or generic. Detect the actual image format
    // first so valid Chromium PNG artwork is not silently discarded.
    let detected_mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && (&bytes[8..12] == b"avif" || &bytes[8..12] == b"avis")
    {
        Some("image/avif")
    } else {
        None
    };
    let declared_mime = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let mime = detected_mime.or_else(|| match declared_mime.as_str() {
        "image/jpeg" | "image/png" | "image/webp" | "image/bmp" | "image/gif" | "image/avif" => {
            Some(declared_mime.as_str())
        }
        _ => None,
    })?;
    Some(Arc::from(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}

fn spawn_media_observer() -> (Sender<MediaAction>, Receiver<Option<MediaSnapshot>>) {
    use windows_startup::Foundation::TypedEventHandler;
    use windows_startup::Media::Control::GlobalSystemMediaTransportControlsSession as Session;
    use windows_startup::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    use windows_startup::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback;
    use windows_startup::Media::Control::{
        CurrentSessionChangedEventArgs, MediaPropertiesChangedEventArgs,
        PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
    };
    use windows_startup::Win32::System::WinRT::{
        RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
    };
    // Transport actions must not be dropped while a WinRT command is completing.
    // The worker serializes the small command stream; refresh requests remain coalesced.
    let (command_sender, command_receiver) = std::sync::mpsc::channel();
    let (snapshot_sender, snapshot_receiver) = sync_channel(2);
    let metadata_dirty = Arc::new(AtomicBool::new(true));
    let artwork_dirty = Arc::new(AtomicBool::new(true));
    let refresh_pending = Arc::new(AtomicBool::new(false));
    let worker_sender = command_sender.clone();
    thread::spawn(move || {
        if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
            return;
        }
        let mut manager = None;
        let mut manager_event_tokens: Option<(Option<i64>, Option<i64>)> = None;
        let mut observed_session: Option<Session> = None;
        let mut session_event_tokens: Option<(Option<i64>, Option<i64>, Option<i64>)> = None;
        let mut cached_metadata: Option<(String, String, String, Option<Arc<str>>, u64)> = None;
        let mut artwork_revision = 0u64;
        let mut refresh_metadata_at = std::time::Instant::now();
        let mut selected_session_id: Option<String> = None;
        loop {
            if metadata_dirty.swap(false, Ordering::AcqRel) {
                refresh_metadata_at = std::time::Instant::now();
            }
            if manager.is_none()
                && let Ok(next_manager) = Manager::RequestAsync().and_then(|request| request.join())
            {
                let current_wake = worker_sender.clone();
                let current_dirty = metadata_dirty.clone();
                let current_pending = refresh_pending.clone();
                let current_handler =
                    TypedEventHandler::<Manager, CurrentSessionChangedEventArgs>::new(
                        move |_, _| {
                            current_dirty.store(true, Ordering::Release);
                            queue_media_refresh(&current_wake, &current_pending);
                            Ok(())
                        },
                    );
                let sessions_wake = worker_sender.clone();
                let sessions_dirty = metadata_dirty.clone();
                let sessions_pending = refresh_pending.clone();
                let sessions_handler =
                    TypedEventHandler::<Manager, SessionsChangedEventArgs>::new(move |_, _| {
                        sessions_dirty.store(true, Ordering::Release);
                        queue_media_refresh(&sessions_wake, &sessions_pending);
                        Ok(())
                    });
                manager_event_tokens = Some((
                    next_manager.CurrentSessionChanged(&current_handler).ok(),
                    next_manager.SessionsChanged(&sessions_handler).ok(),
                ));
                manager = Some(next_manager);
            }
            let available_sessions = manager
                .as_ref()
                .map(|manager| {
                    let Ok(sessions) = manager.GetSessions() else {
                        return Vec::new();
                    };
                    let count = sessions.Size().unwrap_or(0);
                    let mut available = Vec::with_capacity(count as usize);
                    let mut source_occurrences = HashMap::<String, usize>::new();
                    for index in 0..count {
                        let Ok(candidate) = sessions.GetAt(index) else {
                            continue;
                        };
                        let source = candidate
                            .SourceAppUserModelId()
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let occurrence = source_occurrences.entry(source.clone()).or_default();
                        *occurrence += 1;
                        // The occurrence suffix distinguishes separate sessions
                        // from the same app without depending on list ordering
                        // across different apps.
                        let id = format!("{source}#{occurrence}");
                        let playing = candidate
                            .GetPlaybackInfo()
                            .and_then(|info| info.PlaybackStatus())
                            .is_ok_and(|status| status == Playback::Playing);
                        available.push((id, source, candidate, playing));
                    }
                    available
                })
                .unwrap_or_default();
            let current_session = manager
                .as_ref()
                .and_then(|manager| manager.GetCurrentSession().ok());
            if selected_session_id.as_ref().is_some_and(|selected| {
                !available_sessions
                    .iter()
                    .any(|(id, _, _, _)| id == selected)
            }) {
                selected_session_id = None;
            }
            // A paused preference must not mask another source that is playing.
            // Among playing sources, retain the explicit choice, then the
            // currently displayed source, before Windows' active preference.
            let preferred = selected_session_id.as_ref().and_then(|id| {
                available_sessions
                    .iter()
                    .find(|(candidate_id, _, _, _)| candidate_id == id)
            });
            let selected = preferred
                .filter(|(_, _, _, playing)| *playing)
                .or_else(|| {
                    observed_session.as_ref().and_then(|current| {
                        available_sessions
                            .iter()
                            .find(|(_, _, candidate, playing)| candidate == current && *playing)
                    })
                })
                .or_else(|| {
                    current_session.as_ref().and_then(|current| {
                        available_sessions
                            .iter()
                            .find(|(_, _, candidate, playing)| candidate == current && *playing)
                    })
                })
                .or_else(|| {
                    available_sessions
                        .iter()
                        .find(|(_, _, _, playing)| *playing)
                })
                .or(preferred)
                .or_else(|| {
                    current_session.as_ref().and_then(|current| {
                        available_sessions
                            .iter()
                            .find(|(_, _, candidate, _)| candidate == current)
                    })
                })
                .or_else(|| available_sessions.first());
            let (session_id, session) = selected
                .map(|(id, _, session, _)| (Some(id.clone()), Some(session.clone())))
                .unwrap_or_else(|| {
                    current_session
                        .map(|session| {
                            let source = session
                                .SourceAppUserModelId()
                                .map(|value| value.to_string())
                                .unwrap_or_default();
                            (Some(format!("{source}#1")), Some(session))
                        })
                        .unwrap_or_default()
                });
            let media_sessions = available_sessions
                .iter()
                .map(|(id, source, _, playing)| MediaSessionOption {
                    id: id.clone(),
                    source: source.clone(),
                    playing: *playing,
                })
                .collect::<Vec<_>>();
            let session_changed = match (&observed_session, &session) {
                (Some(previous), Some(current)) => previous != current,
                (None, Some(_)) | (Some(_), None) => true,
                (None, None) => false,
            };
            if session_changed {
                refresh_metadata_at = std::time::Instant::now();
                cached_metadata = None;
                if let (Some(previous), Some((playback, media, timeline))) =
                    (observed_session.take(), session_event_tokens.take())
                {
                    if let Some(token) = playback {
                        let _ = previous.RemovePlaybackInfoChanged(token);
                    }
                    if let Some(token) = media {
                        let _ = previous.RemoveMediaPropertiesChanged(token);
                    }
                    if let Some(token) = timeline {
                        let _ = previous.RemoveTimelinePropertiesChanged(token);
                    }
                }
                if let Some(current) = session.as_ref() {
                    let playback_wake = worker_sender.clone();
                    let playback_pending = refresh_pending.clone();
                    let playback_handler =
                        TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(
                            move |_, _| {
                                queue_media_refresh(&playback_wake, &playback_pending);
                                Ok(())
                            },
                        );
                    let media_wake = worker_sender.clone();
                    let media_dirty = metadata_dirty.clone();
                    let media_artwork_dirty = artwork_dirty.clone();
                    let media_pending = refresh_pending.clone();
                    let media_handler =
                        TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(
                            move |_, _| {
                                media_dirty.store(true, Ordering::Release);
                                media_artwork_dirty.store(true, Ordering::Release);
                                queue_media_refresh(&media_wake, &media_pending);
                                Ok(())
                            },
                        );
                    let timeline_wake = worker_sender.clone();
                    let timeline_pending = refresh_pending.clone();
                    let timeline_handler = TypedEventHandler::<
                        Session,
                        TimelinePropertiesChangedEventArgs,
                    >::new(move |_, _| {
                        queue_media_refresh(&timeline_wake, &timeline_pending);
                        Ok(())
                    });
                    session_event_tokens = Some((
                        current.PlaybackInfoChanged(&playback_handler).ok(),
                        current.MediaPropertiesChanged(&media_handler).ok(),
                        current.TimelinePropertiesChanged(&timeline_handler).ok(),
                    ));
                    observed_session = Some(current.clone());
                }
            }
            let artwork_was_updated = artwork_dirty.swap(false, Ordering::AcqRel);
            let snapshot = session.as_ref().and_then(|session: &Session| {
                let session_id = session_id.as_ref()?;
                let source = session
                    .SourceAppUserModelId()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let now = std::time::Instant::now();
                let source_changed = cached_metadata
                    .as_ref()
                    .is_none_or(|(cached_source, _, _, _, _)| cached_source != &source);
                if source_changed || now >= refresh_metadata_at {
                    if let Ok(properties) = session
                        .TryGetMediaPropertiesAsync()
                        .and_then(|request| request.join())
                    {
                        let title = properties
                            .Title()
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let artist = properties
                            .Artist()
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        if !title.is_empty() {
                            let track_changed = cached_metadata.as_ref().is_none_or(
                                |(old_source, old_title, old_artist, _, _)| {
                                    old_source != &source
                                        || old_title != &title
                                        || old_artist != &artist
                                },
                            );
                            let cached_artwork = cached_metadata
                                .as_ref()
                                .and_then(|(_, _, _, artwork, _)| artwork.as_ref());
                            let artwork =
                                if track_changed || artwork_was_updated || cached_artwork.is_none()
                                {
                                    // Some browser sessions publish metadata before their thumbnail is ready.
                                    // Retry on the existing metadata refresh cadence until artwork arrives.
                                    media_artwork(&properties)
                                } else {
                                    cached_artwork.cloned()
                                };
                            let artwork_changed =
                                cached_artwork.map(|old| old.as_ref()) != artwork.as_deref();
                            if track_changed || artwork_changed {
                                artwork_revision = artwork_revision.wrapping_add(1).max(1);
                            }
                            cached_metadata =
                                Some((source.clone(), title, artist, artwork, artwork_revision));
                        }
                    }
                    let retry_delay = if cached_metadata
                        .as_ref()
                        .is_some_and(|(_, _, _, artwork, _)| artwork.is_none())
                    {
                        std::time::Duration::from_millis(500)
                    } else {
                        std::time::Duration::from_secs(2)
                    };
                    refresh_metadata_at = now + retry_delay;
                }
                // Keep the card populated while a newly selected session is
                // still publishing its first track metadata. The React view
                // can show the provider name until a title arrives.
                if cached_metadata
                    .as_ref()
                    .is_none_or(|(cached_source, _, _, _, _)| cached_source != &source)
                {
                    cached_metadata = Some((
                        source.clone(),
                        String::new(),
                        String::new(),
                        None,
                        artwork_revision,
                    ));
                }
                let (cached_source, title, artist, artwork, artwork_revision) =
                    cached_metadata.as_ref()?;
                if cached_source != &source {
                    return None;
                }
                let playback = session.GetPlaybackInfo().ok();
                let playing = playback
                    .as_ref()
                    .and_then(|info| info.PlaybackStatus().ok())
                    .is_some_and(|status| status == Playback::Playing);
                let timeline = session.GetTimelineProperties().ok();
                // GSMTC reports a timeline with explicit start/end bounds.
                // Using EndTime as a duration is wrong for sessions with a
                // non-zero start offset; normalize both values to that range.
                let start_ticks = timeline
                    .as_ref()
                    .and_then(|value| value.StartTime().ok())
                    .map(|value| value.Duration)
                    .unwrap_or(0);
                let duration_ms = timeline
                    .as_ref()
                    .and_then(|value| value.EndTime().ok())
                    .and_then(|value| value.Duration.checked_sub(start_ticks))
                    .and_then(|ticks| u64::try_from(ticks).ok())
                    .map(|ticks| ticks / 10_000)
                    .filter(|duration| *duration > 0);
                let sampled_position_ms = timeline
                    .as_ref()
                    .and_then(|value| value.Position().ok())
                    .and_then(|value| value.Duration.checked_sub(start_ticks))
                    .and_then(|ticks| u64::try_from(ticks).ok())
                    .map(|ticks| ticks / 10_000);
                let now_unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_millis() as i128)
                    .unwrap_or(0);
                let sampled_age_ms = timeline
                    .as_ref()
                    .and_then(|value| value.LastUpdatedTime().ok())
                    .map(|updated| {
                        (i128::from(updated.UniversalTime) - 116_444_736_000_000_000i128) / 10_000
                    })
                    .and_then(|updated_ms| {
                        u64::try_from(now_unix_ms.saturating_sub(updated_ms)).ok()
                    });
                let position_ms = sampled_position_ms.map(|position| {
                    let current = if playing {
                        position.saturating_add(sampled_age_ms.unwrap_or(0))
                    } else {
                        position
                    };
                    duration_ms.map_or(current, |duration| current.min(duration))
                });
                let controls = playback.as_ref().and_then(|info| info.Controls().ok());
                let can_toggle = controls.as_ref().is_some_and(|controls| {
                    controls.IsPlayPauseToggleEnabled().unwrap_or(false)
                        || if playing {
                            controls.IsPauseEnabled().unwrap_or(false)
                        } else {
                            controls.IsPlayEnabled().unwrap_or(false)
                        }
                });
                let can_seek = controls
                    .as_ref()
                    .is_some_and(|controls| controls.IsPlaybackPositionEnabled().unwrap_or(false))
                    && duration_ms.is_some();
                Some(MediaSnapshot {
                    session_id: session_id.clone(),
                    sessions: media_sessions.clone(),
                    title: title.clone(),
                    artist: artist.clone(),
                    source: source.clone(),
                    playing,
                    artwork: artwork.clone(),
                    artwork_revision: *artwork_revision,
                    position_ms,
                    duration_ms,
                    can_toggle,
                    can_previous: controls
                        .as_ref()
                        .is_some_and(|controls| controls.IsPreviousEnabled().unwrap_or(false)),
                    can_next: controls
                        .as_ref()
                        .is_some_and(|controls| controls.IsNextEnabled().unwrap_or(false)),
                    can_seek,
                })
            });
            let wait = if snapshot.as_ref().is_some_and(|snapshot| snapshot.playing) {
                std::time::Duration::from_millis(350)
            } else {
                std::time::Duration::from_millis(1_000)
            };
            let _ = snapshot_sender.try_send(snapshot);
            match command_receiver.recv_timeout(wait) {
                Ok(MediaAction::Refresh) => {
                    refresh_pending.store(false, Ordering::Release);
                }
                Ok(MediaAction::Select(id)) => {
                    if available_sessions
                        .iter()
                        .any(|(candidate_id, _, _, _)| candidate_id == &id)
                    {
                        selected_session_id = Some(id);
                        metadata_dirty.store(true, Ordering::Release);
                        artwork_dirty.store(true, Ordering::Release);
                    }
                    continue;
                }
                Ok(action) => {
                    if let Some(session) = session.as_ref() {
                        let request = match action {
                            MediaAction::Refresh => continue,
                            MediaAction::Select(_) => continue,
                            MediaAction::Toggle => session.TryTogglePlayPauseAsync(),
                            MediaAction::Previous => session.TrySkipPreviousAsync(),
                            MediaAction::Next => session.TrySkipNextAsync(),
                            MediaAction::SeekToMs(position_ms) => {
                                let start_ticks = session
                                    .GetTimelineProperties()
                                    .ok()
                                    .and_then(|timeline| timeline.StartTime().ok())
                                    .map_or(0, |start| start.Duration);
                                let relative_ticks =
                                    i64::try_from(position_ms.saturating_mul(10_000))
                                        .unwrap_or(i64::MAX);
                                session.TryChangePlaybackPositionAsync(
                                    start_ticks.saturating_add(relative_ticks),
                                )
                            }
                        };
                        if let Ok(request) = request {
                            let _ = request.join();
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        if let (Some(session), Some((playback, media, timeline))) =
            (observed_session, session_event_tokens)
        {
            if let Some(token) = playback {
                let _ = session.RemovePlaybackInfoChanged(token);
            }
            if let Some(token) = media {
                let _ = session.RemoveMediaPropertiesChanged(token);
            }
            if let Some(token) = timeline {
                let _ = session.RemoveTimelinePropertiesChanged(token);
            }
        }
        if let (Some(manager), Some((current, sessions))) = (manager, manager_event_tokens) {
            if let Some(token) = current {
                let _ = manager.RemoveCurrentSessionChanged(token);
            }
            if let Some(token) = sessions {
                let _ = manager.RemoveSessionsChanged(token);
            }
        }
        unsafe { RoUninitialize() };
    });
    (command_sender, snapshot_receiver)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct AudioDevice {
    id: String,
    name: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
struct AudioSnapshot {
    volume: Option<u8>,
    peak: u8,
    devices: Vec<AudioDevice>,
    default_device_id: Option<String>,
    selected_device_id: Option<String>,
}

enum AudioAction {
    SetVolume(u8),
    SelectDevice(Option<String>),
}

fn audio_device_id(device: &windows_startup::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows_startup::Win32::System::Com::CoTaskMemFree;
    let id = unsafe { device.GetId().ok()? };
    let value = unsafe { id.to_string().ok() };
    unsafe {
        CoTaskMemFree(Some(id.0.cast()));
    }
    value
}

fn audio_device_name(device: &windows_startup::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows_startup::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows_startup::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToStringAlloc,
    };
    use windows_startup::Win32::System::Com::{CoTaskMemFree, STGM_READ};
    let store = unsafe { device.OpenPropertyStore(STGM_READ).ok()? };
    let mut value = unsafe { store.GetValue(&PKEY_Device_FriendlyName).ok()? };
    let name = unsafe { PropVariantToStringAlloc(&value) };
    unsafe {
        let _ = PropVariantClear(&mut value);
    }
    let name = name.ok()?;
    let result = unsafe { name.to_string().ok() };
    unsafe {
        CoTaskMemFree(Some(name.0.cast()));
    }
    result.filter(|name| !name.trim().is_empty())
}

fn audio_device_for_id(
    id: Option<&str>,
) -> Option<windows_startup::Win32::Media::Audio::IMMDevice> {
    use windows_startup::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows_startup::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    let id = id?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()? };
    let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
    let device = unsafe {
        enumerator
            .GetDevice(windows_startup::core::PCWSTR(wide.as_ptr()))
            .ok()?
    };
    Some(device)
}

fn output_devices() -> (Vec<AudioDevice>, Option<String>) {
    use windows_startup::Win32::Media::Audio::{
        DEVICE_STATE_ACTIVE, IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender,
    };
    use windows_startup::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    let enumerator: IMMDeviceEnumerator =
        match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(enumerator) => enumerator,
            Err(_) => return (Vec::new(), None),
        };
    let default_id = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()
    }
    .and_then(|device| audio_device_id(&device));
    let devices = unsafe {
        enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .ok()
    }
    .and_then(|collection| {
        let count = unsafe { collection.GetCount().ok()? };
        Some(
            (0..count)
                .filter_map(|index| {
                    let device = unsafe { collection.Item(index).ok()? };
                    Some(AudioDevice {
                        id: audio_device_id(&device)?,
                        name: audio_device_name(&device)?,
                    })
                })
                .collect(),
        )
    })
    .unwrap_or_default();
    (devices, default_id)
}

fn output_volume_for_device(
    device_id: Option<&str>,
) -> Option<windows_startup::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows_startup::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows_startup::Win32::Media::Audio::{eMultimedia, eRender};
    use windows_startup::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    let device = if let Some(id) = device_id {
        audio_device_for_id(Some(id))
    } else {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()? };
        unsafe {
            enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .ok()
        }
    }?;
    unsafe { device.Activate(CLSCTX_ALL, None).ok() }
}

fn output_meter_for_device(
    device_id: Option<&str>,
) -> Option<windows_startup::Win32::Media::Audio::Endpoints::IAudioMeterInformation> {
    use windows_startup::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender,
    };
    use windows_startup::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    let device = if let Some(id) = device_id {
        audio_device_for_id(Some(id))
    } else {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()? };
        unsafe {
            enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .ok()
        }
    }?;
    unsafe { device.Activate(CLSCTX_ALL, None).ok() }
}

fn spawn_output_observer(
    media_playing: Arc<AtomicBool>,
) -> (SyncSender<AudioAction>, Receiver<AudioSnapshot>) {
    use windows_startup::Win32::System::WinRT::{
        RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
    };
    let (command_sender, command_receiver) = sync_channel(8);
    let (snapshot_sender, snapshot_receiver) = sync_channel(2);
    thread::spawn(move || {
        if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
            return;
        }
        let mut selected_device_id: Option<String> = None;
        let mut endpoint = output_volume_for_device(None);
        let mut meter = output_meter_for_device(None);
        let mut devices: Vec<AudioDevice> = Vec::new();
        let mut default_device_id = None;
        let mut refresh_devices_at = std::time::Instant::now();
        let mut previous = AudioSnapshot::default();
        let mut last_peak_sample = std::time::Instant::now() - std::time::Duration::from_millis(40);
        let mut sampled_peak = 0u8;
        loop {
            let mut latest_volume = None;
            let mut selected_device_changed = false;
            while let Ok(action) = command_receiver.try_recv() {
                match action {
                    AudioAction::SetVolume(level) => latest_volume = Some(level),
                    AudioAction::SelectDevice(device_id) => {
                        let next =
                            device_id.filter(|id| devices.iter().any(|device| &device.id == id));
                        if selected_device_id != next {
                            selected_device_id = next;
                            selected_device_changed = true;
                        }
                    }
                }
            }
            let now = std::time::Instant::now();
            if now >= refresh_devices_at {
                let refreshed = output_devices();
                if refreshed.1 != default_device_id {
                    selected_device_changed = true;
                }
                devices = refreshed.0;
                default_device_id = refreshed.1;
                refresh_devices_at = now + std::time::Duration::from_secs(2);
            }
            if selected_device_id
                .as_ref()
                .is_some_and(|id| !devices.iter().any(|device| &device.id == id))
            {
                selected_device_id = None;
                selected_device_changed = true;
            }
            if selected_device_changed {
                endpoint = output_volume_for_device(selected_device_id.as_deref());
                meter = output_meter_for_device(selected_device_id.as_deref());
            }
            if let Some(level) = latest_volume
                && let Some(endpoint) = endpoint.as_ref()
            {
                let _ = unsafe {
                    endpoint.SetMasterVolumeLevelScalar(f32::from(level) / 100.0, null())
                };
            }
            let level = endpoint
                .as_ref()
                .and_then(|endpoint| unsafe { endpoint.GetMasterVolumeLevelScalar().ok() })
                .map(|level| (level.clamp(0.0, 1.0) * 100.0).round() as u8);
            if !media_playing.load(Ordering::Relaxed) {
                sampled_peak = 0;
            } else if now.duration_since(last_peak_sample) >= std::time::Duration::from_millis(33) {
                sampled_peak = meter
                    .as_ref()
                    .and_then(|meter| unsafe { meter.GetPeakValue().ok() })
                    .map_or(0, |value| {
                        ((value.clamp(0.0, 1.0).sqrt() * 100.0).round() as u8 / 4) * 4
                    });
                last_peak_sample = now;
            }
            let snapshot = AudioSnapshot {
                volume: level,
                peak: sampled_peak,
                devices: devices.clone(),
                default_device_id: default_device_id.clone(),
                selected_device_id: selected_device_id.clone(),
            };
            if snapshot != previous {
                // Keep retrying a changed snapshot if the UI queue is briefly
                // full; otherwise the final slider/device state could be lost.
                if snapshot_sender.try_send(snapshot.clone()).is_ok() {
                    previous = snapshot;
                }
            }
            // A short wait keeps volume writes responsive while the observer
            // still polls external system-volume changes at a modest cadence.
            match command_receiver.recv_timeout(std::time::Duration::from_millis(16)) {
                Ok(action) => match action {
                    AudioAction::SetVolume(level) => {
                        if let Some(endpoint) = endpoint.as_ref() {
                            let _ = unsafe {
                                endpoint
                                    .SetMasterVolumeLevelScalar(f32::from(level) / 100.0, null())
                            };
                        }
                    }
                    AudioAction::SelectDevice(device_id) => {
                        selected_device_id = device_id
                            .filter(|id| previous.devices.iter().any(|device| &device.id == id));
                        endpoint = output_volume_for_device(selected_device_id.as_deref());
                        meter = output_meter_for_device(selected_device_id.as_deref());
                    }
                },
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        unsafe { RoUninitialize() };
    });
    (command_sender, snapshot_receiver)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ShellPage {
    #[default]
    Home,
    Clipboard,
    Shelf,
    Convert,
    Text,
    Agents,
    Ask,
    Note,
    Weather,
    Controls,
}

impl ShellPage {
    const ALL: [Self; 10] = [
        Self::Home,
        Self::Clipboard,
        Self::Shelf,
        Self::Convert,
        Self::Text,
        Self::Agents,
        Self::Ask,
        Self::Note,
        Self::Weather,
        Self::Controls,
    ];

    const fn title(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Clipboard => "Clipboard",
            Self::Shelf => "Shelf",
            Self::Convert => "Convert",
            Self::Text => "Text",
            Self::Agents => "Agents",
            Self::Ask => "Ask",
            Self::Note => "Note",
            Self::Weather => "Weather",
            Self::Controls => "Controls",
        }
    }

    fn enabled(self, modules: &ModuleSettings) -> bool {
        match self {
            Self::Home => true,
            Self::Convert => modules.convert,
            Self::Clipboard => modules.clipboard,
            Self::Shelf => modules.shelf,
            Self::Text => modules.text,
            Self::Agents | Self::Ask => modules.agents,
            Self::Note => modules.note,
            Self::Weather => modules.weather,
            Self::Controls => modules.controls,
        }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModuleSettings {
    pub clipboard: bool,
    pub shelf: bool,
    pub convert: bool,
    pub text: bool,
    pub agents: bool,
    pub note: bool,
    pub weather: bool,
    pub controls: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WeatherUnits {
    #[default]
    Metric,
    Imperial,
}

#[derive(Clone, Copy)]
enum RadioToggle {
    Wifi,
    Bluetooth,
}

#[derive(Clone, Default, PartialEq, serde::Serialize)]
struct RadioSnapshot {
    wifi: Option<bool>,
    bluetooth: Option<bool>,
    message: Option<String>,
}

fn desktop_wifi(toggle: bool) -> Result<Option<bool>, String> {
    use windows_sys::Win32::NetworkManagement::WiFi::{
        WLAN_INTERFACE_INFO_LIST, WLAN_PHY_RADIO_STATE, WLAN_RADIO_STATE, WlanCloseHandle,
        WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface, WlanSetInterface,
        dot11_radio_state_off, dot11_radio_state_on, wlan_intf_opcode_radio_state,
    };
    let mut version = 0;
    let mut handle = std::ptr::null_mut();
    let status = unsafe { WlanOpenHandle(2, null(), &mut version, &mut handle) };
    if status != 0 {
        return Err(format!("Wi-Fi service error {status}"));
    }
    let result = (|| {
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
        let status = unsafe { WlanEnumInterfaces(handle, null(), &mut list) };
        if status != 0 {
            return Err(format!("Wi-Fi adapter error {status}"));
        }
        let result = (|| {
            if list.is_null() {
                return Ok(None);
            }
            let count = unsafe { (*list).dwNumberOfItems } as usize;
            let mut states = Vec::new();
            for index in 0..count {
                let interface = unsafe { &*((*list).InterfaceInfo.as_ptr().add(index)) };
                let mut size = 0;
                let mut data = null_mut();
                let status = unsafe {
                    WlanQueryInterface(
                        handle,
                        &interface.InterfaceGuid,
                        wlan_intf_opcode_radio_state,
                        null(),
                        &mut size,
                        &mut data,
                        null_mut(),
                    )
                };
                if status != 0 {
                    continue;
                }
                if !data.is_null() {
                    let radio = unsafe { &*(data as *const WLAN_RADIO_STATE) };
                    for phy in radio
                        .PhyRadioState
                        .iter()
                        .take(radio.dwNumberOfPhys.min(64) as usize)
                    {
                        states.push((interface.InterfaceGuid, *phy));
                    }
                    unsafe { WlanFreeMemory(data) };
                }
            }
            if states.is_empty() {
                return Ok(None);
            }
            let enabled = states.iter().any(|(_, phy)| {
                phy.dot11SoftwareRadioState == dot11_radio_state_on
                    && phy.dot11HardwareRadioState == dot11_radio_state_on
            });
            if toggle {
                let next = if enabled {
                    dot11_radio_state_off
                } else {
                    dot11_radio_state_on
                };
                for (guid, mut phy) in states {
                    phy.dot11SoftwareRadioState = next;
                    let status = unsafe {
                        WlanSetInterface(
                            handle,
                            &guid,
                            wlan_intf_opcode_radio_state,
                            std::mem::size_of::<WLAN_PHY_RADIO_STATE>() as u32,
                            &phy as *const _ as *const _,
                            null(),
                        )
                    };
                    if status != 0 {
                        return Err(format!("Windows refused Wi-Fi change (error {status})"));
                    }
                }
                return Ok(Some(!enabled));
            }
            Ok(Some(enabled))
        })();
        if !list.is_null() {
            unsafe { WlanFreeMemory(list as *const _) };
        }
        result
    })();
    unsafe { WlanCloseHandle(handle, null()) };
    result
}

fn read_radios(toggle: Option<RadioToggle>, cached: RadioSnapshot) -> RadioSnapshot {
    use windows_startup::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
    use windows_startup::Win32::System::WinRT::{
        RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
    };
    if matches!(toggle, Some(RadioToggle::Wifi)) {
        return match desktop_wifi(true) {
            Ok(wifi) => RadioSnapshot {
                wifi,
                bluetooth: cached.bluetooth,
                message: wifi.is_none().then(|| {
                    "No controllable Wi-Fi adapter was found. Open Windows Wi-Fi settings.".into()
                }),
            },
            Err(message) => RadioSnapshot {
                wifi: cached.wifi,
                bluetooth: cached.bluetooth,
                message: Some(message),
            },
        };
    }
    // Bluetooth changes don't need a Wi-Fi adapter scan. That scan can block on
    // the WLAN service, so reuse the last known value while the radio API runs.
    let (wifi, wifi_error) = if matches!(toggle, Some(RadioToggle::Bluetooth)) {
        (cached.wifi, None)
    } else {
        match desktop_wifi(false) {
            Ok(wifi) => (wifi, None),
            Err(message) => (cached.wifi, Some(message)),
        }
    };
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
        return RadioSnapshot {
            wifi,
            bluetooth: cached.bluetooth,
            message: Some("Windows Bluetooth radio service is unavailable.".into()),
            ..Default::default()
        };
    }
    let result = (|| -> windows_core::Result<RadioSnapshot> {
        let radios = Radio::GetRadiosAsync()?.join()?;
        let count = radios.Size()?;
        let mut items = Vec::new();
        for index in 0..count {
            items.push(radios.GetAt(index)?);
        }
        let mut message = wifi_error;
        let mut requested_bluetooth_state = None;
        if let Some(RadioToggle::Bluetooth) = toggle {
            let kind = RadioKind::Bluetooth;
            let matches: Vec<_> = items
                .iter()
                .filter(|radio| radio.Kind().ok() == Some(kind))
                .collect();
            if matches.is_empty() {
                message = Some("No matching radio was found on this PC.".to_owned());
            } else if Radio::RequestAccessAsync()?.join()? != RadioAccessStatus::Allowed {
                message = Some(
                    "Windows did not allow radio control. Check app permissions or system policy."
                        .to_owned(),
                );
            } else {
                let target = if matches
                    .iter()
                    .any(|radio| radio.State().ok() == Some(RadioState::On))
                {
                    RadioState::Off
                } else {
                    RadioState::On
                };
                let mut state_change_allowed = true;
                for radio in matches {
                    if radio.SetStateAsync(target)?.join()? != RadioAccessStatus::Allowed {
                        message = Some("Windows refused the radio change.".to_owned());
                        state_change_allowed = false;
                    }
                }
                if state_change_allowed {
                    requested_bluetooth_state = Some(target == RadioState::On);
                }
            }
        }
        let mut snapshot = RadioSnapshot {
            wifi,
            message,
            ..Default::default()
        };
        for radio in &items {
            let state = radio.State().ok();
            let value = match state {
                Some(RadioState::On) => Some(true),
                Some(RadioState::Off) => Some(false),
                _ => None,
            };
            if radio.Kind().ok() == Some(RadioKind::Bluetooth) {
                snapshot.bluetooth = value.or(snapshot.bluetooth);
            }
        }
        if let Some(state) = requested_bluetooth_state {
            snapshot.bluetooth = Some(state);
        }
        Ok(snapshot)
    })();
    unsafe { RoUninitialize() };
    result.unwrap_or_else(|error| RadioSnapshot {
        wifi,
        bluetooth: cached.bluetooth,
        message: Some(format!("Bluetooth radio unavailable ({error}).")),
        ..Default::default()
    })
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ClipboardSettings {
    pub history_limit: usize,
    pub capture_enabled: bool,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            history_limit: 200,
            capture_enabled: true,
        }
    }
}

impl Default for ModuleSettings {
    fn default() -> Self {
        Self {
            clipboard: true,
            shelf: true,
            convert: true,
            text: true,
            agents: true,
            note: true,
            weather: true,
            controls: true,
        }
    }
}
const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMSUSPEND: usize = 0x0004;
const PBT_APMRESUMECRITICAL: usize = 0x0006;
const PBT_APMRESUMESUSPEND: usize = 0x0007;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const FALLBACK_FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_nanos(16_666_667);
const INITIAL_RENDER_COST: std::time::Duration = std::time::Duration::from_millis(9);

struct ShellCallbacks {
    window: AtomicPtr<c_void>,
    desk_window: AtomicPtr<c_void>,
    tick_queued: AtomicBool,
    is_open: AtomicBool,
    is_pinned: AtomicBool,
    dark_theme: AtomicBool,
    reduced_motion: AtomicBool,
    toggle_requested: AtomicUsize,
    theme_toggle_requested: AtomicBool,
    preference_changes: Mutex<Vec<ShellPreferenceChange>>,
    shortcut_requested: [AtomicBool; SHORTCUT_COUNT],
    shortcut_registered: [AtomicBool; SHORTCUT_COUNT],
    pointer_inside: AtomicBool,
    pointer_poll_needed: AtomicBool,
    shelf_drag_active: AtomicBool,
    shelf_drag_hover_hint: AtomicBool,
    raw_mouse_down: AtomicBool,
    raw_external_press: AtomicBool,
    raw_press_x: AtomicI32,
    raw_press_y: AtomicI32,
    shelf_enabled: AtomicBool,
    pending_shelf_drops: Mutex<Vec<std::path::PathBuf>>,
    pending_conversion_drops: Mutex<Vec<std::path::PathBuf>>,
    drop_intent: AtomicI32,
    drop_convertible: AtomicBool,
    animation_failed: AtomicBool,
    owner: AtomicPtr<EdgeShell>,
    context: egui::Context,
}

enum ShellCommand {
    ShowSettings,
    ShowCommand,
    FocusRestoreTarget(isize),
    DarkTheme(bool),
    ReducedMotion(bool),
}

struct OleDropTargetApartment {
    initialized: bool,
}

impl OleDropTargetApartment {
    fn initialize() -> Self {
        Self {
            initialized: unsafe {
                windows_startup::Win32::System::Ole::OleInitialize(None).is_ok()
            },
        }
    }
}

impl Drop for OleDropTargetApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { windows_startup::Win32::System::Ole::OleUninitialize() };
        }
    }
}

fn trace_drop(message: std::fmt::Arguments<'_>) {
    // Enabled only for a diagnostic launch. Avoid disk work in the drag loop
    // during normal use, and never log file names or clipboard contents.
    let Some(path) = std::env::var_os("ISLAND_DROP_TRACE") else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{:?} {message}", std::time::SystemTime::now());
    }
}

#[implement(IDropSource)]
struct SignatureDropSource;

#[allow(non_snake_case)]
impl windows_startup::Win32::System::Ole::IDropSource_Impl for SignatureDropSource_Impl {
    fn QueryContinueDrag(
        &self,
        escape_pressed: windows_core::BOOL,
        key_state: windows_startup::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    ) -> windows_core::HRESULT {
        use windows_startup::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, S_OK};
        use windows_startup::Win32::System::SystemServices::MK_LBUTTON;

        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if key_state.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(
        &self,
        _effect: windows_startup::Win32::System::Ole::DROPEFFECT,
    ) -> windows_core::HRESULT {
        windows_startup::Win32::Foundation::DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Start a genuine Windows file drag so browser canvases and native apps receive
/// a CF_HDROP file, rather than only script-created WebView drag text formats.
fn start_signature_file_drag(hwnd: HWND, png: &[u8]) -> Result<bool, String> {
    use windows_startup::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
    use windows_startup::Win32::System::Ole::DROPEFFECT_COPY;
    use windows_startup::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows_startup::Win32::UI::Shell::{
        ILClone, ILFindLastID, ILRemoveLastID, SHCreateDataObject, SHDoDragDrop, SHParseDisplayName,
    };

    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
    if png.len() < PNG_SIGNATURE.len()
        || png.len() > MAX_PNG_BYTES
        || !png.starts_with(PNG_SIGNATURE)
    {
        return Err("The signature PNG is invalid or too large.".into());
    }

    let directory = std::env::temp_dir().join("Island").join("signature-drops");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "Could not prepare the signature drag file.".to_owned())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = directory.join(format!("signature-{}-{stamp}.png", std::process::id()));
    std::fs::write(&path, png).map_err(|_| "Could not save the signature PNG.".to_owned())?;

    let path_wide = wide(&path.to_string_lossy());
    let mut pidl: *mut ITEMIDLIST = null_mut();
    if let Err(error) = unsafe {
        SHParseDisplayName(
            windows_startup::core::PCWSTR(path_wide.as_ptr()),
            None::<&IBindCtx>,
            &mut pidl,
            0,
            None,
        )
    } {
        return Err(format!(
            "Could not prepare the signature file for dragging: {error}"
        ));
    }
    if pidl.is_null() {
        return Err("Could not prepare the signature file for dragging.".into());
    }
    struct Pidl(*mut ITEMIDLIST);
    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }
    let pidl = Pidl(pidl);

    let parent = unsafe { ILClone(pidl.0) };
    if parent.is_null() {
        let _ = std::fs::remove_file(&path);
        return Err("Could not prepare the signature file for dragging.".into());
    }
    let parent = Pidl(parent);
    if !unsafe { ILRemoveLastID(Some(parent.0)) }.as_bool() {
        let _ = std::fs::remove_file(&path);
        return Err("Could not prepare the signature file for dragging.".into());
    }
    let child = unsafe { ILFindLastID(pidl.0) };
    if child.is_null() {
        let _ = std::fs::remove_file(&path);
        return Err("Could not prepare the signature file for dragging.".into());
    }
    let child_items = [child.cast_const()];
    let data: IDataObject = unsafe {
        SHCreateDataObject(
            Some(parent.0.cast_const()),
            Some(&child_items),
            None::<&IDataObject>,
        )
        .map_err(|error| format!("Could not prepare the signature drag data: {error}"))?
    };
    let source: IDropSource = SignatureDropSource.into();
    let effect = match unsafe {
        SHDoDragDrop(
            Some(windows_startup::Win32::Foundation::HWND(hwnd as _)),
            &data,
            &source,
            DROPEFFECT_COPY,
        )
    } {
        Ok(effect) => effect,
        Err(error) => {
            let _ = std::fs::remove_file(&path);
            return Err(format!(
                "Windows could not start the signature drag: {error}"
            ));
        }
    };
    let dropped = effect.0 & DROPEFFECT_COPY.0 != 0;
    if !dropped {
        let _ = std::fs::remove_file(&path);
    }
    Ok(dropped)
}

#[implement(IDropTarget)]
struct ShelfDropTarget {
    callbacks: Arc<ShellCallbacks>,
    accepts_files: AtomicBool,
    dragged_paths: Mutex<Vec<std::path::PathBuf>>,
}

// Resolve against the visible HWND in physical screen coordinates. Hover uses a
// narrow dead band; release always uses its actual half and must be in bounds.
fn resolve_drop_intent(callbacks: &ShellCallbacks, x: i32, y: i32, release: bool) -> i32 {
    let window = callbacks.window.load(Ordering::Acquire) as HWND;
    let mut rect = RECT::default();
    if window.is_null()
        || unsafe { GetWindowRect(window, &mut rect) } == 0
        || x < rect.left
        || x >= rect.right
        || y < rect.top
        || y >= rect.bottom
    {
        return 0;
    }
    let center = rect.left + (rect.right - rect.left) / 2;
    let previous = callbacks.drop_intent.load(Ordering::Acquire);
    let intent = if !release && (x - center).abs() < 5 && previous != 0 {
        previous
    } else if x < center {
        if callbacks.drop_convertible.load(Ordering::Acquire) {
            1
        } else {
            3
        }
    } else if callbacks.shelf_enabled.load(Ordering::Acquire) {
        2
    } else {
        4
    };
    if callbacks.drop_intent.swap(intent, Ordering::AcqRel) != intent {
        post_shell_tick(window, callbacks);
    }
    intent
}

impl ShelfDropTarget {
    fn new(callbacks: &Arc<ShellCallbacks>) -> IDropTarget {
        ShelfDropTarget {
            callbacks: Arc::clone(callbacks),
            accepts_files: AtomicBool::new(false),
            dragged_paths: Mutex::new(Vec::new()),
        }
        .into()
    }

    fn effect_from_offered(
        effect: *mut windows_startup::Win32::System::Ole::DROPEFFECT,
    ) -> windows_startup::Win32::System::Ole::DROPEFFECT {
        use windows_startup::Win32::System::Ole::{
            DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
        };

        if effect.is_null() {
            return DROPEFFECT_NONE;
        }
        let offered = unsafe { (*effect).0 };
        // A shelf entry is a reference to the original path, so every
        // accepted file drop is deliberately non-destructive. Explorer can
        // advertise only MOVE for same-volume shortcuts; rejecting that mask
        // makes an otherwise valid .lnk show the blocked cursor. Like Wry's
        // WebView2 drop target, translate a supported file operation to COPY.
        if offered & (DROPEFFECT_COPY.0 | DROPEFFECT_LINK.0 | DROPEFFECT_MOVE.0) != 0 {
            DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        }
    }

    fn set_drag_state(&self, active: bool) {
        if self
            .callbacks
            .shelf_drag_active
            .swap(active, Ordering::AcqRel)
            == active
        {
            return;
        }
        let window = self.callbacks.window.load(Ordering::Acquire) as HWND;
        if !window.is_null() {
            post_shell_tick(window, &self.callbacks);
        }
    }

    fn has_file_drop(
        data: &windows_startup::core::Ref<'_, windows_startup::Win32::System::Com::IDataObject>,
    ) -> bool {
        use windows_startup::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
        use windows_startup::Win32::System::Ole::CF_HDROP;

        let Some(data) = data.as_ref() else {
            return false;
        };
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        unsafe { data.QueryGetData(&format).is_ok() }
    }

    fn paths_from_drop(
        data: &windows_startup::core::Ref<'_, windows_startup::Win32::System::Com::IDataObject>,
    ) -> Option<Vec<std::path::PathBuf>> {
        use std::os::windows::ffi::OsStringExt;
        use windows_startup::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
        use windows_startup::Win32::System::Ole::{CF_HDROP, ReleaseStgMedium};
        use windows_startup::Win32::UI::Shell::{DragQueryFileW, HDROP};

        let data = data.as_ref()?;
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = unsafe { data.GetData(&format).ok()? };
        let paths = if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            let hdrop = HDROP(unsafe { medium.u.hGlobal.0 });
            let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) }.min(32);
            let mut paths = Vec::with_capacity(count as usize);
            for index in 0..count {
                let length = unsafe { DragQueryFileW(hdrop, index, None) } as usize;
                if length == 0 || length > 32_767 {
                    continue;
                }
                let mut buffer = vec![0u16; length + 1];
                let copied = unsafe { DragQueryFileW(hdrop, index, Some(&mut buffer)) } as usize;
                if copied <= length {
                    paths.push(std::path::PathBuf::from(std::ffi::OsString::from_wide(
                        &buffer[..copied],
                    )));
                }
            }
            Some(paths)
        } else {
            None
        };
        unsafe { ReleaseStgMedium(&mut medium) };
        paths
    }

    fn replace_dragged_paths(&self, paths: Vec<std::path::PathBuf>) {
        *self
            .dragged_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = paths;
    }

    fn take_dragged_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = self
            .dragged_paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *paths)
    }

    fn queue_drop(&self, paths: Vec<std::path::PathBuf>, intent: i32) {
        if paths.is_empty() || !matches!(intent, 1 | 2) {
            return;
        }
        let queue = if intent == 1 {
            &self.callbacks.pending_conversion_drops
        } else {
            &self.callbacks.pending_shelf_drops
        };
        let mut pending = queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let remaining = 32usize.saturating_sub(pending.len());
        pending.extend(paths.into_iter().take(remaining));
    }
}

#[allow(non_snake_case)]
impl windows_startup::Win32::System::Ole::IDropTarget_Impl for ShelfDropTarget_Impl {
    fn DragEnter(
        &self,
        data: windows_startup::core::Ref<'_, windows_startup::Win32::System::Com::IDataObject>,
        _key_state: windows_startup::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        point: &windows_startup::Win32::Foundation::POINTL,
        effect: *mut windows_startup::Win32::System::Ole::DROPEFFECT,
    ) -> windows_startup::core::Result<()> {
        use windows_startup::Win32::System::Ole::DROPEFFECT_NONE;

        let selected_effect = ShelfDropTarget::effect_from_offered(effect);
        let offered_effect = if effect.is_null() {
            0
        } else {
            unsafe { (*effect).0 }
        };
        let has_files = ShelfDropTarget::has_file_drop(&data);
        let accepts = selected_effect.0 != DROPEFFECT_NONE.0 && has_files;
        let paths = if accepts {
            ShelfDropTarget::paths_from_drop(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        trace_drop(format_args!(
            "DragEnter offered={offered_effect:#x} selected={:#x} has_files={has_files} shelf_enabled={} paths={}",
            selected_effect.0,
            self.callbacks.shelf_enabled.load(Ordering::Acquire),
            paths.len(),
        ));
        self.callbacks.drop_convertible.store(
            !paths.is_empty() && paths.iter().all(|p| crate::conversion::could_convert(p)),
            Ordering::Release,
        );
        self.replace_dragged_paths(paths);
        let intent = resolve_drop_intent(&self.callbacks, point.x, point.y, false);
        self.accepts_files.store(accepts, Ordering::Release);
        if !effect.is_null() {
            unsafe {
                *effect = if accepts && matches!(intent, 1 | 2) {
                    selected_effect
                } else {
                    DROPEFFECT_NONE
                }
            };
        }
        self.set_drag_state(accepts);
        Ok(())
    }

    fn DragOver(
        &self,
        _key_state: windows_startup::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        point: &windows_startup::Win32::Foundation::POINTL,
        effect: *mut windows_startup::Win32::System::Ole::DROPEFFECT,
    ) -> windows_startup::core::Result<()> {
        use windows_startup::Win32::System::Ole::DROPEFFECT_NONE;

        let selected_effect = ShelfDropTarget::effect_from_offered(effect);
        let accepts =
            self.accepts_files.load(Ordering::Acquire) && selected_effect.0 != DROPEFFECT_NONE.0;
        let intent = resolve_drop_intent(&self.callbacks, point.x, point.y, false);
        if !effect.is_null() {
            unsafe {
                *effect = if accepts && matches!(intent, 1 | 2) {
                    selected_effect
                } else {
                    DROPEFFECT_NONE
                }
            };
        }
        self.set_drag_state(accepts);
        Ok(())
    }

    fn DragLeave(&self) -> windows_startup::core::Result<()> {
        trace_drop(format_args!("DragLeave"));
        self.accepts_files.store(false, Ordering::Release);
        if self.callbacks.drop_intent.swap(0, Ordering::AcqRel) != 0 {
            let window = self.callbacks.window.load(Ordering::Acquire) as HWND;
            if !window.is_null() {
                post_shell_tick(window, &self.callbacks);
            }
        }
        self.set_drag_state(false);
        Ok(())
    }

    fn Drop(
        &self,
        data: windows_startup::core::Ref<'_, windows_startup::Win32::System::Com::IDataObject>,
        _key_state: windows_startup::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        point: &windows_startup::Win32::Foundation::POINTL,
        effect: *mut windows_startup::Win32::System::Ole::DROPEFFECT,
    ) -> windows_startup::core::Result<()> {
        use windows_startup::Win32::System::Ole::DROPEFFECT_NONE;

        let selected_effect = ShelfDropTarget::effect_from_offered(effect);
        let accepts_files = self.accepts_files.load(Ordering::Acquire);
        let intent = resolve_drop_intent(&self.callbacks, point.x, point.y, true);
        let shelf_enabled = matches!(intent, 1 | 2) && selected_effect.0 != DROPEFFECT_NONE.0;
        let mut paths = if accepts_files && shelf_enabled {
            ShelfDropTarget::paths_from_drop(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        if paths.is_empty() && accepts_files && shelf_enabled {
            paths = self.take_dragged_paths();
        } else {
            self.take_dragged_paths();
        }
        self.accepts_files.store(false, Ordering::Release);
        self.set_drag_state(false);
        let dropped = !paths.is_empty();
        if !dropped && self.callbacks.drop_intent.swap(0, Ordering::AcqRel) != 0 {
            let window = self.callbacks.window.load(Ordering::Acquire) as HWND;
            if !window.is_null() {
                post_shell_tick(window, &self.callbacks);
            }
        }
        trace_drop(format_args!(
            "Drop accepted={dropped} paths={}",
            paths.len()
        ));
        if dropped {
            self.queue_drop(paths, intent);
            let window = self.callbacks.window.load(Ordering::Acquire) as HWND;
            if !window.is_null() {
                post_shell_tick(window, &self.callbacks);
            }
        }
        if !effect.is_null() {
            unsafe {
                *effect = if dropped {
                    selected_effect
                } else {
                    DROPEFFECT_NONE
                }
            };
        }
        Ok(())
    }
}

unsafe extern "system" fn collect_drop_child(
    hwnd: HWND,
    parameter: LPARAM,
) -> windows_sys::core::BOOL {
    if parameter != 0 {
        let children = unsafe { &mut *(parameter as *mut Vec<HWND>) };
        children.push(hwnd);
    }
    1
}

fn child_windows(parent: HWND) -> Vec<HWND> {
    let mut children = Vec::new();
    unsafe {
        EnumChildWindows(
            parent,
            Some(collect_drop_child),
            &mut children as *mut Vec<HWND> as LPARAM,
        );
    }
    children
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityIndicators {
    pub opencode_active_count: usize,
    pub microphone_active: bool,
    pub camera_active: bool,
    pub details: String,
    pub agent_runs: Vec<AgentRun>,
}

struct CompactScreenFeedback {
    id: u64,
    label: String,
    kind: &'static str,
    phase: &'static str,
    copy_value: Option<String>,
    color: Option<(u8, u8, u8)>,
    file_path: Option<String>,
    display_duration: Option<std::time::Duration>,
    display_remaining: Option<std::time::Duration>,
    stale_at: Option<std::time::Instant>,
    stale_remaining: Option<std::time::Duration>,
    next_visible_at: Option<std::time::Instant>,
    next_visible_remaining: Option<std::time::Duration>,
    appeared_at: std::time::Instant,
    expires_at: Option<std::time::Instant>,
    repeat_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AgentRun {
    pub provider: String,
    pub source: String,
    pub session_id: String,
    pub label: String,
    pub status: String,
}

impl ActivityIndicators {
    fn label(&self) -> Option<String> {
        match self.opencode_active_count {
            0 => None,
            1 => Some("OpenCode · active".to_owned()),
            count => Some(format!("OpenCode · {count} active")),
        }
    }
}

struct ObservedActivity {
    privacy: Option<crate::platform::windows::privacy::PrivacyActivity>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct ForegroundAppSnapshot {
    name: String,
    executable: String,
    kind: &'static str,
    fullscreen: bool,
}

fn foreground_app_snapshot(
    shell_window: HWND,
    fallback_window: HWND,
) -> Option<ForegroundAppSnapshot> {
    let current = unsafe { GetForegroundWindow() };
    let foreground = if current.is_null()
        || current == shell_window
        || unsafe { IsChild(shell_window, current) } != 0
    {
        if !fallback_window.is_null() && unsafe { IsWindow(fallback_window) } != 0 {
            fallback_window
        } else {
            return None;
        }
    } else {
        current
    };
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(foreground, &mut process_id);
    }
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let queried =
        unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
    unsafe {
        CloseHandle(process);
    }
    if !queried || length == 0 {
        return None;
    }
    let image = String::from_utf16_lossy(&path[..(length as usize).min(path.len())]);
    let executable = image
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&image)
        .to_owned();
    let id = executable.to_ascii_lowercase();
    let (name, kind) = match id.as_str() {
        "helium.exe" => ("Helium".to_owned(), "browser"),
        "brave.exe" => ("Brave".to_owned(), "browser"),
        "msedge.exe" => ("Microsoft Edge".to_owned(), "browser"),
        "chrome.exe" => ("Chrome".to_owned(), "browser"),
        "chromium.exe" => ("Chromium".to_owned(), "browser"),
        "firefox.exe" => ("Firefox".to_owned(), "browser"),
        "librewolf.exe" => ("LibreWolf".to_owned(), "browser"),
        "waterfox.exe" => ("Waterfox".to_owned(), "browser"),
        "floorp.exe" => ("Floorp".to_owned(), "browser"),
        "thorium.exe" => ("Thorium".to_owned(), "browser"),
        "vivaldi.exe" => ("Vivaldi".to_owned(), "browser"),
        "opera.exe" | "opera_gx.exe" => ("Opera".to_owned(), "browser"),
        "arc.exe" => ("Arc".to_owned(), "browser"),
        "zen.exe" => ("Zen Browser".to_owned(), "browser"),
        "codex.exe" => ("Codex".to_owned(), "coding"),
        "opencode.exe" => ("OpenCode".to_owned(), "coding"),
        "code.exe" => ("Visual Studio Code".to_owned(), "coding"),
        "cursor.exe" => ("Cursor".to_owned(), "coding"),
        "windsurf.exe" => ("Windsurf".to_owned(), "coding"),
        "devenv.exe" => ("Visual Studio".to_owned(), "coding"),
        "rider64.exe" => ("Rider".to_owned(), "coding"),
        "idea64.exe" => ("IntelliJ IDEA".to_owned(), "coding"),
        "windowsterminal.exe" | "wt.exe" => ("Terminal".to_owned(), "coding"),
        "wezterm-gui.exe" => ("WezTerm".to_owned(), "coding"),
        "alacritty.exe" => ("Alacritty".to_owned(), "coding"),
        "powershell.exe" | "pwsh.exe" | "cmd.exe" => ("Terminal".to_owned(), "coding"),
        "excel.exe" => ("Excel".to_owned(), "spreadsheet"),
        "soffice.exe" => ("LibreOffice".to_owned(), "office"),
        "winword.exe" => ("Word".to_owned(), "office"),
        "powerpnt.exe" => ("PowerPoint".to_owned(), "office"),
        "figma.exe" => ("Figma".to_owned(), "design"),
        "photoshop.exe" => ("Photoshop".to_owned(), "design"),
        "illustrator.exe" => ("Illustrator".to_owned(), "design"),
        "blender.exe" => ("Blender".to_owned(), "design"),
        "spotify.exe" => ("Spotify".to_owned(), "media"),
        "vlc.exe" => ("VLC".to_owned(), "media"),
        "mpv.exe" => ("mpv".to_owned(), "media"),
        "wmplayer.exe" | "music.ui.exe" => ("Media Player".to_owned(), "media"),
        _ => {
            let stem = std::path::Path::new(&executable)
                .file_stem()?
                .to_string_lossy()
                .into_owned();
            (stem, "other")
        }
    };
    Some(ForegroundAppSnapshot {
        name,
        executable,
        kind,
        fullscreen: foreground_covers_primary(shell_window),
    })
}

/// Thread-safe facade for the native shell. The layered HWND and its GDI
/// surface are created, mutated, and destroyed on a dedicated Win32 message
/// thread; the hidden eframe viewport never owns the shell's frame cadence.
pub struct ShellController {
    callbacks: Arc<ShellCallbacks>,
    commands: SyncSender<ShellCommand>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy)]
pub struct ShellStartup {
    pub shape: ShapeMode,
    pub accent_gradient: crate::storage::StoredAccentGradient,
    pub media_card_style: crate::storage::StoredMediaCardStyle,
    pub hover_open_delay: std::time::Duration,
    pub hover_close_delay: std::time::Duration,
    pub click_to_open: bool,
    pub close_on_outside_click: bool,
    pub reduced_motion: bool,
    pub folded_transparency_percent: u8,
    pub open_transparency_percent: u8,
    pub clock_indicator_enabled: bool,
    pub battery_indicator_enabled: bool,
    pub privacy_indicator_enabled: bool,
    pub hide_in_fullscreen: bool,
    pub exclude_from_screen_capture: bool,
    pub shortcuts: ShortcutSet,
}

#[derive(Clone, Copy)]
pub enum ShellPreferenceChange {
    Shape(ShapeMode),
    AccentGradient(crate::storage::StoredAccentGradient),
    MediaCardStyle(crate::storage::StoredMediaCardStyle),
    Clock(bool),
    Battery(bool),
    Privacy(bool),
    HideFullscreen(bool),
    ScreenCaptureExclusion(bool),
    ReducedMotion(bool),
    HoverOpen(u32),
    HoverClose(u32),
    ClickToOpen(bool),
    CloseOnOutsideClick(bool),
    FoldedTransparency(u8),
    OpenTransparency(u8),
}

/// Compact, no-activate top-edge pill. Its alpha plane is also the system hit map.
#[derive(Clone, Debug, serde::Serialize)]
struct CommandFileResult {
    name: String,
    path: String,
    kind: &'static str,
    icon: Option<String>,
}

impl CommandFileResult {
    fn launch(&self) -> Result<(), crate::commands::LaunchError> {
        crate::commands::FileEntry {
            name: self.name.clone(),
            path: self.path.clone(),
        }
        .launch()
    }
}

pub struct EdgeShell {
    window: HWND,
    _ole_apartment: OleDropTargetApartment,
    drop_target: Option<IDropTarget>,
    drop_catcher: HWND,
    drop_catcher_region: Option<(i32, i32, i32, i32, i32, ShapeMode)>,
    drag_hover_stop: Arc<AtomicBool>,
    drag_hover_worker: Option<JoinHandle<()>>,
    raw_input_registered: bool,
    child_drop_targets: Vec<(HWND, IDropTarget)>,
    drop_targets_refresh_attempts: u8,
    next_drop_targets_refresh: std::time::Instant,
    frame_timer: HANDLE,
    callbacks: Arc<ShellCallbacks>,
    commands: Receiver<ShellCommand>,
    previous_foreground: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    font: HGDIOBJ,
    previous_font: HGDIOBJ,
    pixels: *mut u8,
    alpha: Vec<u8>,
    layout: ShellLayout,
    width: i32,
    height: i32,
    max_width: i32,
    max_height: i32,
    position: POINT,
    screen_width: i32,
    scale: f64,
    status_label: String,
    expanded_body_width: f64,
    wide_layout: bool,
    expanded_body_height: f64,
    settings_layout_open: bool,
    settings_layout_height: Option<f64>,
    activity: ActivityIndicators,
    observed_activity: Option<ObservedActivity>,
    activity_receiver: Option<Receiver<ObservedActivity>>,
    activity_poll_at: std::time::Instant,
    opencode_actions: Sender<crate::opencode_control::Action>,
    opencode_receiver: Receiver<crate::opencode_control::Snapshot>,
    opencode: crate::opencode_control::Snapshot,
    acp_opencode_actions: Sender<crate::agent_acp::Action>,
    acp_opencode_receiver: Receiver<crate::agent_acp::Snapshot>,
    acp_opencode: crate::agent_acp::Snapshot,
    acp_codex_actions: Sender<crate::agent_acp::Action>,
    acp_codex_receiver: Receiver<crate::agent_acp::Snapshot>,
    acp_codex: crate::agent_acp::Snapshot,
    privacy_indicator_enabled: bool,
    page: ShellPage,
    converter: crate::conversion::Converter,
    react_view: Option<wry::WebView>,
    react_messages: Receiver<String>,
    screen_overlay_sender: Sender<crate::platform::windows::screen_overlay::ResultEvent>,
    screen_overlay_receiver: Receiver<crate::platform::windows::screen_overlay::ResultEvent>,
    pending_screen_overlay: Option<(
        crate::platform::windows::screen_overlay::Mode,
        bool,
        crate::platform::windows::screen_overlay::RecordQuality,
    )>,
    screen_overlay_active: bool,
    screen_overlay_feedback_kind: Option<&'static str>,
    compact_feedbacks: Vec<CompactScreenFeedback>,
    next_compact_feedback_id: u64,
    compact_feedback_visible_id: Option<u64>,
    compact_status_presentation_visible: bool,
    compact_status_visible: String,
    compact_context_tools: usize,
    auto_tuck_deadline: Option<std::time::Instant>,
    screen_clipboard_values: HashSet<String>,
    screen_recording_stop: Option<Sender<()>>,
    screen_ocr_sender: SyncSender<(u64, Result<String, String>)>,
    screen_ocr_receiver: Receiver<(u64, Result<String, String>)>,
    screen_ocr_pending: bool,
    screen_qr_sender: SyncSender<Result<Option<String>, String>>,
    screen_qr_receiver: Receiver<Result<Option<String>, String>>,
    screen_qr_pending: bool,
    speech_settings: crate::platform::windows::speech::SpeechSettings,
    speech_sender: SyncSender<(u64, Result<String, String>)>,
    speech_receiver: Receiver<(u64, Result<String, String>)>,
    speech_pending: bool,
    wake_word: crate::platform::windows::wake_word::WakeWordService,
    wake_word_status: String,
    wake_word_phrase: String,
    wake_word_error: Option<String>,
    wake_word_revision: u64,
    wake_surface: bool,
    wake_closing: bool,
    wake_mic_clear_samples: u8,
    startup_sender: SyncSender<crate::platform::windows::startup::StartupTaskStatus>,
    startup_receiver: Receiver<crate::platform::windows::startup::StartupTaskStatus>,
    startup_status: crate::platform::windows::startup::StartupTaskStatus,
    startup_pending: bool,
    react_ready: bool,
    react_visible: bool,
    react_compact_visible: bool,
    react_compact_faded: bool,
    react_compact_render_generation: u64,
    pending_react_compact_render: Option<u64>,
    pending_react_compact_since: Option<std::time::Instant>,
    react_compact_render_ready: bool,
    react_ever_visible: bool,
    react_bounds: Option<(i32, i32, u32, u32)>,
    content_exit_deadline: Option<std::time::Instant>,
    last_react_state: String,
    foreground_app: Option<ForegroundAppSnapshot>,
    foreground_app_poll_at: std::time::Instant,
    last_react_artwork_revision: Option<u64>,
    last_react_shelf_revision: Option<u64>,
    modules: ModuleSettings,
    clipboard_items: Vec<String>,
    clipboard_history_enabled: bool,
    clipboard_limit: usize,
    clipboard_listener_registered: bool,
    shelf_items: Vec<std::path::PathBuf>,
    shelf_icon_data: Vec<Option<String>>,
    shelf_revision: u64,
    shelf_generation: u64,
    home_actions: Vec<Option<String>>,
    shelf_icon_sender: SyncSender<(u64, usize, Option<String>)>,
    shelf_icon_receiver: Receiver<(u64, usize, Option<String>)>,
    text_draft: String,
    note: String,
    notebook: NoteBook,
    command_result: Option<String>,
    command_error: Option<String>,
    command_apps: Vec<crate::commands::AppEntry>,
    command_apps_loaded: bool,
    command_apps_loading: bool,
    command_apps_updated_at: Option<std::time::Instant>,
    command_app_query: String,
    command_app_results: Vec<String>,
    command_app_icons: HashMap<String, Option<String>>,
    command_app_icon_sender: Sender<(String, Vec<(String, Option<String>)>)>,
    command_app_icon_receiver: Receiver<(String, Vec<(String, Option<String>)>)>,
    command_app_icon_generation: Arc<AtomicU64>,
    command_icon_cache: Arc<Mutex<HashMap<std::path::PathBuf, Option<String>>>>,
    command_apps_receiver: Option<Receiver<Vec<crate::commands::AppEntry>>>,
    command_file_query: String,
    command_file_results: Vec<CommandFileResult>,
    command_file_receiver: Option<Receiver<(String, Vec<CommandFileResult>)>>,
    media_actions: Sender<MediaAction>,
    media_receiver: Receiver<Option<MediaSnapshot>>,
    media: Option<MediaSnapshot>,
    media_playing: Arc<AtomicBool>,
    output_actions: SyncSender<AudioAction>,
    output_receiver: Receiver<AudioSnapshot>,
    output_volume: Option<u8>,
    output_peak: u8,
    audio_devices: Vec<AudioDevice>,
    default_output_device_id: Option<String>,
    selected_output_device_id: Option<String>,
    radio_sender: SyncSender<RadioSnapshot>,
    radio_receiver: Receiver<RadioSnapshot>,
    radios: RadioSnapshot,
    radio_busy: bool,
    radio_last_check: Option<std::time::Instant>,
    power_notice: Option<String>,
    note_dirty: bool,
    weather_city: String,
    weather_units: WeatherUnits,
    weather_lines: Vec<String>,
    weather_data: Option<serde_json::Value>,
    weather_error: Option<String>,
    weather_updated_at: Option<u64>,
    weather_last_attempt: Option<std::time::Instant>,
    weather_receiver: Option<Receiver<Result<(Vec<String>, serde_json::Value), String>>>,
    dark_theme: bool,
    accent_gradient: crate::storage::StoredAccentGradient,
    media_card_style: crate::storage::StoredMediaCardStyle,
    reduced_motion: bool,
    hover_open_delay_ms: u32,
    hover_close_delay_ms: u32,
    click_to_open: bool,
    close_on_outside_click: bool,
    folded_transparency_percent: u8,
    open_transparency_percent: u8,
    clock_indicator_enabled: bool,
    battery_indicator_enabled: bool,
    hide_in_fullscreen: bool,
    exclude_from_screen_capture: bool,
    screen_capture_exclusion_supported: bool,
    fullscreen_hidden: bool,
    fullscreen_poll_at: std::time::Instant,
    reducer: ShellReducer,
    shape: ShapeMode,
    last_pointer_inside: bool,
    last_shelf_drag_active: bool,
    last_react_drop_active: bool,
    last_react_drop_intent: i32,
    drop_handoff_sequence: u64,
    shelf_drop_transition_at: Option<std::time::Instant>,
    drop_open_hold_until: Option<std::time::Instant>,
    hover_intent_deadline: Option<std::time::Instant>,
    width_motion: SpringMotion,
    height_motion: SpringMotion,
    drop_motion: SpringMotion,
    tuck_motion: SpringMotion,
    animation_target: (f64, f64),
    last_frame: std::time::Instant,
    clock_refresh_at: std::time::Instant,
    reminder_notified_at: Option<u64>,
    shortcut_registered: [bool; SHORTCUT_COUNT],
    refresh_period: std::time::Duration,
    render_cost_estimate: std::time::Duration,
    frame_interval: std::time::Duration,
}

struct ChildWindowHandle(HWND);

impl HasWindowHandle for ChildWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
        // SAFETY: this HWND is owned by EdgeShell and remains valid while the
        // WebView child is alive on the same shell thread.
        Ok(
            unsafe {
                WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(raw)))
            },
        )
    }
}

fn register_shortcuts(
    window: HWND,
    shortcuts: ShortcutSet,
    callbacks: &ShellCallbacks,
) -> [bool; SHORTCUT_COUNT] {
    let mut registered = [false; SHORTCUT_COUNT];
    for binding in shortcuts.bindings() {
        if !matches!(
            binding.action,
            ShortcutAction::ToggleIsland | ShortcutAction::CommandBar
        ) {
            continue;
        }
        let mut modifiers = HOTKEY_MOD_NOREPEAT;
        if binding.modifiers & MOD_ALT != 0 {
            modifiers |= 0x0001;
        }
        if binding.modifiers & MOD_CONTROL != 0 {
            modifiers |= 0x0002;
        }
        if binding.modifiers & MOD_SHIFT != 0 {
            modifiers |= 0x0004;
        }
        // SAFETY: registration is scoped to the owned shell HWND and each
        // action receives a stable, unique ID. Windows reports conflicts.
        let success = unsafe {
            RegisterHotKey(
                window,
                HOTKEY_ID_BASE + binding.action.index() as i32,
                modifiers,
                u32::from(binding.virtual_key),
            ) != 0
        };
        registered[binding.action.index()] = success;
        callbacks.shortcut_registered[binding.action.index()].store(success, Ordering::Release);
    }
    registered
}

fn unregister_shortcuts(window: HWND, registered: &[bool; SHORTCUT_COUNT]) {
    for (index, was_registered) in registered.iter().enumerate() {
        if *was_registered {
            // SAFETY: this ID was registered on the same owned HWND.
            unsafe { UnregisterHotKey(window, HOTKEY_ID_BASE + index as i32) };
        }
    }
}

impl ShellController {
    pub fn new(context: &egui::Context, startup: ShellStartup) -> Result<Self, String> {
        let callbacks = Arc::new(ShellCallbacks {
            window: AtomicPtr::new(null_mut()),
            desk_window: AtomicPtr::new(null_mut()),
            tick_queued: AtomicBool::new(false),
            is_open: AtomicBool::new(false),
            is_pinned: AtomicBool::new(false),
            dark_theme: AtomicBool::new(context.theme() == egui::Theme::Dark),
            reduced_motion: AtomicBool::new(startup.reduced_motion),
            toggle_requested: AtomicUsize::new(0),
            theme_toggle_requested: AtomicBool::new(false),
            preference_changes: Mutex::new(Vec::new()),
            shortcut_requested: std::array::from_fn(|_| AtomicBool::new(false)),
            shortcut_registered: std::array::from_fn(|_| AtomicBool::new(false)),
            pointer_inside: AtomicBool::new(false),
            pointer_poll_needed: AtomicBool::new(false),
            shelf_drag_active: AtomicBool::new(false),
            shelf_drag_hover_hint: AtomicBool::new(false),
            raw_mouse_down: AtomicBool::new(false),
            raw_external_press: AtomicBool::new(false),
            raw_press_x: AtomicI32::new(0),
            raw_press_y: AtomicI32::new(0),
            shelf_enabled: AtomicBool::new(load_module_settings().shelf),
            pending_shelf_drops: Mutex::new(Vec::new()),
            pending_conversion_drops: Mutex::new(Vec::new()),
            drop_intent: AtomicI32::new(0),
            drop_convertible: AtomicBool::new(true),
            animation_failed: AtomicBool::new(false),
            owner: AtomicPtr::new(null_mut()),
            context: context.clone(),
        });
        let (commands, command_receiver) = sync_channel(16);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let thread_callbacks = Arc::clone(&callbacks);
        let thread_context = context.clone();
        let worker = thread::Builder::new()
            .name("island-edge-shell".into())
            .spawn(move || {
                match EdgeShell::create(
                    &thread_context,
                    thread_callbacks,
                    command_receiver,
                    startup,
                ) {
                    Ok(shell) => {
                        if ready_sender.send(Ok(())).is_ok() {
                            run_native_message_loop(
                                shell.window,
                                shell.frame_timer,
                                Arc::as_ptr(&shell.callbacks),
                            );
                        }
                        drop(shell);
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                }
            })
            .map_err(|error| format!("could not start the native shell thread: {error}"))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = worker.join();
                return Err(error);
            }
            Err(_) => {
                let _ = worker.join();
                return Err("the native shell thread stopped during startup".into());
            }
        }

        Ok(Self {
            callbacks,
            commands,
            worker: Some(worker),
        })
    }

    fn send_command(&self, command: ShellCommand) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => "the native shell command queue is full".to_owned(),
                TrySendError::Disconnected(_) => "the native shell thread stopped".to_owned(),
            })?;

        let window = self.callbacks.window.load(Ordering::Acquire);
        if window.is_null() || unsafe { PostMessageW(window, WM_APP_SHELL_COMMAND, 0, 0) } == 0 {
            self.callbacks
                .animation_failed
                .store(true, Ordering::Release);
            self.callbacks.context.request_repaint();
            return Err(last_error("could not notify the native shell thread"));
        }
        Ok(())
    }

    pub fn take_animation_failure(&self) -> bool {
        self.callbacks
            .animation_failed
            .swap(false, Ordering::AcqRel)
    }

    pub fn take_toggle_request(&self) -> Option<bool> {
        let events = self.callbacks.toggle_requested.swap(0, Ordering::AcqRel);
        (events != 0).then(|| self.callbacks.is_open.load(Ordering::Acquire))
    }

    pub fn take_theme_toggle_request(&self) -> bool {
        self.callbacks
            .theme_toggle_requested
            .swap(false, Ordering::AcqRel)
    }

    pub fn take_preference_changes(&self) -> Vec<ShellPreferenceChange> {
        self.callbacks
            .preference_changes
            .lock()
            .map(|mut changes| std::mem::take(&mut *changes))
            .unwrap_or_default()
    }

    pub fn take_shortcut_requests(&self) -> Vec<ShortcutAction> {
        ShortcutAction::ALL
            .into_iter()
            .filter(|action| {
                self.callbacks.shortcut_requested[action.index()].swap(false, Ordering::AcqRel)
            })
            .collect()
    }

    pub fn show_settings(&self) -> Result<(), String> {
        self.send_command(ShellCommand::ShowSettings)
    }

    pub fn show_command(&self) -> Result<(), String> {
        self.send_command(ShellCommand::ShowCommand)
    }

    pub fn set_focus_restore_target(&self, target: HWND) -> Result<(), String> {
        self.send_command(ShellCommand::FocusRestoreTarget(target as isize))
    }

    pub fn set_desk_window(&self, window: HWND) {
        self.callbacks.desk_window.store(window, Ordering::Release);
    }

    pub fn set_dark_theme(&self, dark_theme: bool) -> Result<(), String> {
        let previous = self.callbacks.dark_theme.swap(dark_theme, Ordering::AcqRel);
        if previous == dark_theme {
            return Ok(());
        }
        self.send_command(ShellCommand::DarkTheme(dark_theme))
            .inspect_err(|_| self.callbacks.dark_theme.store(previous, Ordering::Release))
    }

    pub fn set_reduced_motion(&self, reduced_motion: bool) -> Result<(), String> {
        let previous = self
            .callbacks
            .reduced_motion
            .swap(reduced_motion, Ordering::AcqRel);
        if previous == reduced_motion {
            return Ok(());
        }
        self.send_command(ShellCommand::ReducedMotion(reduced_motion))
            .inspect_err(|_| {
                self.callbacks
                    .reduced_motion
                    .store(previous, Ordering::Release)
            })
    }
}

impl Drop for ShellController {
    fn drop(&mut self) {
        let window = self.callbacks.window.load(Ordering::Acquire);
        if !window.is_null() {
            unsafe { PostMessageW(window, WM_APP_SHELL_STOP, 0, 0) };
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl EdgeShell {
    fn create(
        context: &egui::Context,
        callbacks: Arc<ShellCallbacks>,
        commands: Receiver<ShellCommand>,
        startup: ShellStartup,
    ) -> Result<Box<Self>, String> {
        // OLE drag targets are registered and called on this shell STA. Keep
        // the apartment alive for the complete lifetime of the native HWND.
        let ole_apartment = OleDropTargetApartment::initialize();
        let dpi = unsafe { GetDpiForSystem() };
        if dpi == 0 {
            return Err(last_error("could not read the Windows display scale"));
        }
        let (scale, max_width, max_height) = shell_surface_metrics(dpi)
            .ok_or_else(|| "Windows display scale exceeds the shell's safe limits".to_owned())?;
        let layout = ShellLayout::new(startup.shape, startup.shape.folded_body_width(), scale)
            .ok_or_else(|| "invalid shell geometry".to_owned())?;
        let (width, height) = layout.physical_size();
        let width = i32::try_from(width).map_err(|_| "shell width exceeds Windows limits")?;
        let height = i32::try_from(height).map_err(|_| "shell height exceeds Windows limits")?;
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        if screen_width <= 0 {
            return Err(last_error("could not read the primary display width"));
        }
        let position = POINT {
            x: centered_pixel_origin(screen_width, width),
            y: 0,
        };
        let pixel_count = (max_width as usize)
            .checked_mul(max_height as usize)
            .ok_or_else(|| "shell backing surface exceeds memory limits".to_owned())?;
        let mut alpha = Vec::new();
        alpha
            .try_reserve_exact(pixel_count)
            .map_err(|_| "could not allocate the shell hit surface".to_owned())?;

        let class_name = wide(CLASS_NAME);
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            return Err(last_error("could not get the application instance"));
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(shell_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_CLASS_ALREADY_EXISTS as i32) {
                return Err(format!("could not register the shell window: {error}"));
            }
        }

        let callback_pointer = Arc::as_ptr(&callbacks).cast_mut().cast();
        let window = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class_name.as_ptr(),
                wide(WINDOW_TITLE).as_ptr(),
                WS_POPUP,
                position.x,
                position.y,
                width,
                height,
                null_mut(),
                null_mut(),
                instance,
                callback_pointer,
            )
        };
        if window.is_null() {
            return Err(last_error("could not create the compact shell window"));
        }
        let screen_capture_exclusion_supported = supports_screen_capture_exclusion();
        let mut exclude_from_screen_capture =
            startup.exclude_from_screen_capture && screen_capture_exclusion_supported;
        if exclude_from_screen_capture
            && unsafe { SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE) } == 0
        {
            exclude_from_screen_capture = false;
            if let Ok(mut changes) = callbacks.preference_changes.lock() {
                changes.push(ShellPreferenceChange::ScreenCaptureExclusion(false));
            }
        }
        callbacks.window.store(window, Ordering::Release);
        // Explorer can bypass OLE callbacks on a per-pixel layered top-level
        // window. Ask the Shell to deliver CF_HDROP as WM_DROPFILES here.
        unsafe { DragAcceptFiles(window, 1) };
        let drop_target = if ole_apartment.initialized {
            let target = ShelfDropTarget::new(&callbacks);
            // WebView2 children and the shaped catcher still use OLE when
            // they own the hit-test point.
            Some(target)
        } else {
            None
        };
        let shortcuts = startup.shortcuts;
        let shortcut_registered = register_shortcuts(window, shortcuts, &callbacks);
        let (dc, bitmap, previous_bitmap, font, previous_font, pixels) =
            match create_surface(max_width, max_height, scale) {
                Ok(surface) => surface,
                Err(error) => {
                    unregister_shortcuts(window, &shortcut_registered);
                    if drop_target.is_some() {
                        let _ = unsafe {
                            windows_startup::Win32::System::Ole::RevokeDragDrop(
                                windows_startup::Win32::Foundation::HWND(window as _),
                            )
                        };
                    }
                    callbacks.window.store(null_mut(), Ordering::Release);
                    unsafe { DestroyWindow(window) };
                    return Err(error);
                }
            };
        let refresh_period = composition_refresh_period();
        let (react_sender, react_messages) = sync_channel(64);
        let (screen_overlay_sender, screen_overlay_receiver) = std::sync::mpsc::channel();
        let (screen_ocr_sender, screen_ocr_receiver) = sync_channel(1);
        let (screen_qr_sender, screen_qr_receiver) = sync_channel(1);
        let (speech_sender, speech_receiver) = sync_channel(1);
        let (startup_sender, startup_receiver) = sync_channel(1);
        let (opencode_actions, opencode_receiver) = crate::opencode_control::start();
        let (acp_opencode_actions, acp_opencode_receiver) = crate::agent_acp::start("opencode");
        let (acp_codex_actions, acp_codex_receiver) = crate::agent_acp::start("codex");
        let wake_callbacks = Arc::clone(&callbacks);
        let wake_window = window as usize;
        let wake_word = crate::platform::windows::wake_word::WakeWordService::new(move || {
            post_shell_tick(wake_window as HWND, &wake_callbacks);
        });
        let mut reducer = ShellReducer::default();
        reducer.set_hover_delays(startup.hover_open_delay, startup.hover_close_delay);
        reducer.set_interaction_behavior(startup.click_to_open, startup.close_on_outside_click);
        let notebook = load_notebook();
        let weather_city = load_weather_city();
        let weather_units = load_weather_units();
        let cached_weather = load_weather_cache(&weather_city, weather_units);
        let shelf_items = load_shelf_paths();
        let home_actions = load_home_actions();
        let modules = load_module_settings();
        callbacks
            .shelf_enabled
            .store(modules.shelf, Ordering::Release);
        let (shelf_icon_sender, shelf_icon_receiver) = sync_channel(64);
        let (media_actions, media_receiver) = spawn_media_observer();
        let media_playing = Arc::new(AtomicBool::new(false));
        let (output_actions, output_receiver) = spawn_output_observer(Arc::clone(&media_playing));
        let (radio_sender, radio_receiver) = sync_channel(4);
        let (command_app_icon_sender, command_app_icon_receiver) = std::sync::mpsc::channel();
        let mut shell = Box::new(Self {
            window,
            _ole_apartment: ole_apartment,
            drop_target,
            drop_catcher: null_mut(),
            drop_catcher_region: None,
            drag_hover_stop: Arc::new(AtomicBool::new(false)),
            drag_hover_worker: None,
            raw_input_registered: false,
            child_drop_targets: Vec::new(),
            drop_targets_refresh_attempts: 0,
            next_drop_targets_refresh: std::time::Instant::now(),
            frame_timer: null_mut(),
            callbacks,
            commands,
            previous_foreground: null_mut(),
            dc,
            bitmap,
            previous_bitmap,
            font,
            previous_font,
            pixels,
            alpha,
            layout,
            width,
            height,
            max_width,
            max_height,
            position,
            screen_width,
            scale,
            status_label: current_status_label(
                startup.clock_indicator_enabled,
                startup.battery_indicator_enabled,
            ),
            expanded_body_width: startup.shape.expanded_body_width(),
            wide_layout: false,
            expanded_body_height: INITIAL_HOME_EXPANDED_BODY_HEIGHT,
            settings_layout_open: false,
            settings_layout_height: None,
            activity: ActivityIndicators::default(),
            observed_activity: None,
            activity_receiver: None,
            activity_poll_at: std::time::Instant::now(),
            opencode_actions,
            opencode_receiver,
            opencode: crate::opencode_control::Snapshot::default(),
            acp_opencode_actions,
            acp_opencode_receiver,
            acp_opencode: crate::agent_acp::Snapshot::default(),
            acp_codex_actions,
            acp_codex_receiver,
            acp_codex: crate::agent_acp::Snapshot::default(),
            privacy_indicator_enabled: startup.privacy_indicator_enabled,
            page: ShellPage::Home,
            converter: crate::conversion::Converter::default(),
            react_view: None,
            react_messages,
            screen_overlay_sender,
            screen_overlay_receiver,
            pending_screen_overlay: None,
            screen_overlay_active: false,
            screen_overlay_feedback_kind: None,
            compact_feedbacks: Vec::new(),
            next_compact_feedback_id: 1,
            compact_feedback_visible_id: None,
            compact_status_presentation_visible: false,
            compact_status_visible: "idle".to_owned(),
            compact_context_tools: 0,
            auto_tuck_deadline: None,
            screen_clipboard_values: HashSet::new(),
            screen_recording_stop: None,
            screen_ocr_sender,
            screen_ocr_receiver,
            screen_ocr_pending: false,
            screen_qr_sender,
            screen_qr_receiver,
            screen_qr_pending: false,
            speech_settings: crate::platform::windows::speech::SpeechSettings::load(),
            speech_sender,
            speech_receiver,
            speech_pending: false,
            wake_word,
            wake_word_status: "off".into(),
            wake_word_phrase: "Hey Island".into(),
            wake_word_error: None,
            wake_word_revision: 0,
            wake_surface: false,
            wake_closing: false,
            wake_mic_clear_samples: 0,
            startup_sender,
            startup_receiver,
            startup_status: crate::platform::windows::startup::StartupTaskStatus::Checking,
            startup_pending: false,
            react_ready: false,
            react_visible: false,
            react_compact_visible: false,
            react_compact_faded: false,
            react_compact_render_generation: 0,
            pending_react_compact_render: None,
            pending_react_compact_since: None,
            react_compact_render_ready: false,
            react_ever_visible: false,
            react_bounds: None,
            content_exit_deadline: None,
            last_react_state: String::new(),
            foreground_app: None,
            foreground_app_poll_at: std::time::Instant::now(),
            last_react_artwork_revision: None,
            last_react_shelf_revision: None,
            modules,
            clipboard_items: Vec::new(),
            clipboard_history_enabled: load_clipboard_history_enabled(),
            clipboard_limit: load_clipboard_settings().history_limit,
            clipboard_listener_registered: false,
            shelf_icon_data: vec![None; shelf_items.len()],
            shelf_items,
            shelf_revision: 0,
            shelf_generation: 0,
            home_actions,
            shelf_icon_sender,
            shelf_icon_receiver,
            text_draft: String::new(),
            note: notebook.selected_text(),
            notebook,
            command_result: None,
            command_error: None,
            command_apps: Vec::new(),
            command_apps_loaded: false,
            command_apps_loading: false,
            command_apps_updated_at: None,
            command_app_query: String::new(),
            command_app_results: Vec::new(),
            command_app_icons: HashMap::new(),
            command_app_icon_sender,
            command_app_icon_receiver,
            command_app_icon_generation: Arc::new(AtomicU64::new(0)),
            command_icon_cache: Arc::new(Mutex::new(HashMap::new())),
            command_apps_receiver: None,
            command_file_query: String::new(),
            command_file_results: Vec::new(),
            command_file_receiver: None,
            media_actions,
            media_receiver,
            media: None,
            media_playing,
            output_actions,
            output_receiver,
            output_volume: None,
            output_peak: 0,
            audio_devices: Vec::new(),
            default_output_device_id: None,
            selected_output_device_id: None,
            radio_sender,
            radio_receiver,
            radios: RadioSnapshot::default(),
            radio_busy: false,
            radio_last_check: None,
            power_notice: None,
            note_dirty: false,
            weather_city,
            weather_units,
            weather_lines: cached_weather
                .as_ref()
                .map(|cache| cache.lines.clone())
                .unwrap_or_default(),
            weather_data: cached_weather.as_ref().map(|cache| cache.data.clone()),
            weather_error: None,
            weather_updated_at: cached_weather.as_ref().map(|cache| cache.fetched_at),
            weather_last_attempt: None,
            weather_receiver: None,
            dark_theme: context.theme() == egui::Theme::Dark,
            accent_gradient: startup.accent_gradient,
            media_card_style: startup.media_card_style,
            reduced_motion: startup.reduced_motion,
            hover_open_delay_ms: startup.hover_open_delay.as_millis() as u32,
            hover_close_delay_ms: startup.hover_close_delay.as_millis() as u32,
            click_to_open: startup.click_to_open,
            close_on_outside_click: startup.close_on_outside_click,
            folded_transparency_percent: startup.folded_transparency_percent,
            open_transparency_percent: startup.open_transparency_percent,
            clock_indicator_enabled: startup.clock_indicator_enabled,
            battery_indicator_enabled: startup.battery_indicator_enabled,
            hide_in_fullscreen: startup.hide_in_fullscreen,
            exclude_from_screen_capture,
            screen_capture_exclusion_supported,
            fullscreen_hidden: false,
            fullscreen_poll_at: std::time::Instant::now(),
            reducer,
            shape: startup.shape,
            last_pointer_inside: false,
            last_shelf_drag_active: false,
            last_react_drop_active: false,
            last_react_drop_intent: 0,
            drop_handoff_sequence: 0,
            shelf_drop_transition_at: None,
            drop_open_hold_until: None,
            hover_intent_deadline: None,
            width_motion: SpringMotion::new(startup.shape.folded_body_width(), COLLAPSE_SPRING),
            height_motion: SpringMotion::new(f64::from(FOLDED_HEIGHT), COLLAPSE_SPRING),
            drop_motion: SpringMotion::new(0.0, COLLAPSE_SPRING),
            tuck_motion: SpringMotion::new(0.0, COMPACT_STATUS_SPRING),
            animation_target: (startup.shape.folded_body_width(), f64::from(FOLDED_HEIGHT)),
            last_frame: std::time::Instant::now(),
            clock_refresh_at: std::time::Instant::now(),
            reminder_notified_at: None,
            shortcut_registered,
            refresh_period,
            render_cost_estimate: INITIAL_RENDER_COST,
            frame_interval: animation_frame_interval(refresh_period, INITIAL_RENDER_COST),
        });
        shell.frame_timer = unsafe {
            CreateWaitableTimerExW(
                null(),
                null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if shell.frame_timer.is_null() {
            // Windows before 10 version 1803 do not support the high-resolution
            // flag; retain the same native loop with the standard timer instead.
            shell.frame_timer =
                unsafe { CreateWaitableTimerExW(null(), null(), 0, TIMER_ALL_ACCESS) };
        }
        if shell.frame_timer.is_null() {
            return Err(last_error("could not create the shell frame scheduler"));
        }
        // SAFETY: the boxed shell's address is stable until Drop destroys the
        // HWND. WndProc runs on this shell thread, never concurrently with
        // IslandApp's update callback.
        let shell_pointer = Box::as_mut(&mut shell) as *mut Self;
        // SAFETY: shell_pointer points at the stable allocation that owns this
        // callback state, and the callback is nulled before HWND destruction.
        let callbacks = unsafe { Arc::as_ref(&(*shell_pointer).callbacks) };
        callbacks.owner.store(shell_pointer, Ordering::Release);
        shell.clipboard_listener_registered = unsafe {
            windows_sys::Win32::System::DataExchange::AddClipboardFormatListener(shell.window)
        } != 0;
        let shelf_icon_sender = shell.shelf_icon_sender.clone();
        let shelf_paths = shell.shelf_items.clone();
        thread::spawn(move || {
            for (index, path) in shelf_paths.into_iter().enumerate() {
                if shelf_icon_sender
                    .send((0, index, shelf_item_visual_data_url(&path)))
                    .is_err()
                {
                    break;
                }
            }
        });
        shell.render_frame()?;
        shell.create_react_view(react_sender)?;
        shell.request_startup_state(None);
        // Surface creation can include one-time GDI/DWM work. Start hover
        // pacing from the normal estimate and learn the live animation cost.
        shell.render_cost_estimate = INITIAL_RENDER_COST;
        shell.frame_interval = animation_frame_interval(shell.refresh_period, INITIAL_RENDER_COST);
        if unsafe {
            SetWindowPos(
                shell.window,
                HWND_TOPMOST,
                shell.position.x,
                shell.position.y,
                shell.width,
                shell.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
        {
            return Err(last_error("could not place the edge shell"));
        }
        unsafe { ShowWindow(shell.window, SW_SHOWNOACTIVATE) };
        // Re-arm the legacy file-drop route after first showing the shell.
        shell.refresh_parent_drop_target();
        shell.create_drop_catcher();
        shell.refresh_react_drop_targets();
        let mouse_device = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x02,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: shell.window,
        };
        shell.raw_input_registered = unsafe {
            RegisterRawInputDevices(&mouse_device, 1, size_of::<RAWINPUTDEVICE>() as u32)
        } != 0;
        trace_drop(format_args!(
            "raw mouse registration={}",
            shell.raw_input_registered
        ));
        shell.drag_hover_worker = thread::Builder::new()
            .name("island-drag-hover".into())
            .spawn({
                let callbacks = Arc::clone(&shell.callbacks);
                let stop = Arc::clone(&shell.drag_hover_stop);
                move || monitor_external_drag_hover(callbacks, stop)
            })
            .ok();
        let clock_delay = shell.refresh_clock()?;
        shell.arm_timer(clock_delay)?;
        shell.request_tick();
        Ok(shell)
    }

    fn refresh_react_drop_targets(&mut self) {
        if self.drop_target.is_none() {
            return;
        }

        // WebView2 routes pointer input through its internal child HWNDs. Its
        // child tree can be replaced after a hidden view is first shown, so
        // revoke our previous registrations and bind to the current tree.
        for (child, _) in self.child_drop_targets.drain(..).rev() {
            if unsafe { IsWindow(child) } != 0 {
                let _ = unsafe {
                    windows_startup::Win32::System::Ole::RevokeDragDrop(
                        windows_startup::Win32::Foundation::HWND(child as _),
                    )
                };
            }
        }

        for child in child_windows(self.window) {
            let target = ShelfDropTarget::new(&self.callbacks);
            let child_window = windows_startup::Win32::Foundation::HWND(child as _);
            // Another WebView2 component may have registered its own target
            // before the child became part of this view. This view owns every
            // descendant of our shell HWND, so Island replaces that target.
            let _ = unsafe { windows_startup::Win32::System::Ole::RevokeDragDrop(child_window) };
            if unsafe {
                windows_startup::Win32::System::Ole::RegisterDragDrop(child_window, &target)
            }
            .is_ok()
            {
                self.child_drop_targets.push((child, target));
            }
        }
    }

    fn refresh_parent_drop_target(&mut self) {
        let parent = windows_startup::Win32::Foundation::HWND(self.window as _);
        let _ = unsafe { windows_startup::Win32::System::Ole::RevokeDragDrop(parent) };
        unsafe { DragAcceptFiles(self.window, 1) };
    }

    fn create_drop_catcher(&mut self) {
        // UpdateLayeredWindow makes the visible shell's transparent pixels
        // absent from Windows' OLE hit test. A separate shaped HWND immediately
        // behind it gives Explorer a stable target as the shell morphs.
        let class_name = wide(DROP_CATCHER_CLASS_NAME);
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            return;
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(drop_catcher_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0
            && std::io::Error::last_os_error().raw_os_error()
                != Some(ERROR_CLASS_ALREADY_EXISTS as i32)
        {
            return;
        }
        let catcher = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class_name.as_ptr(),
                wide("Island drop catcher").as_ptr(),
                WS_POPUP,
                self.position.x,
                self.position.y,
                self.width,
                self.height,
                null_mut(),
                null_mut(),
                instance,
                self.window.cast(),
            )
        };
        if catcher.is_null() {
            eprintln!(
                "Island drop catcher creation failed: {}",
                std::io::Error::last_os_error()
            );
            return;
        }
        // Alpha 1 participates in OLE hit testing without painting a visible
        // rectangle. The native region below removes every area outside the pill.
        if unsafe { SetLayeredWindowAttributes(catcher, 0, 1, LWA_ALPHA) } == 0 {
            unsafe { DestroyWindow(catcher) };
            return;
        }
        if let Some(target) = self.drop_target.as_ref() {
            let result = unsafe {
                windows_startup::Win32::System::Ole::RegisterDragDrop(
                    windows_startup::Win32::Foundation::HWND(catcher as _),
                    target,
                )
            };
            trace_drop(format_args!("catcher registration={result:?}"));
            if let Err(error) = result {
                eprintln!("Island drop catcher OLE registration failed: {error}");
                unsafe { DragAcceptFiles(catcher, 1) };
            }
        } else {
            unsafe { DragAcceptFiles(catcher, 1) };
        }
        self.drop_catcher = catcher;
        self.sync_drop_catcher();
        trace_drop(format_args!("catcher window created"));
    }

    fn sync_drop_catcher(&mut self) {
        let catcher = self.drop_catcher;
        if catcher.is_null() {
            return;
        }
        if self.fullscreen_hidden || self.screen_overlay_active {
            unsafe { ShowWindow(catcher, SW_HIDE) };
            return;
        }
        let body_width = self
            .width_motion
            .value()
            .clamp(self.folded_target_width(), self.expanded_body_width);
        let body_height = self
            .height_motion
            .value()
            .clamp(f64::from(FOLDED_HEIGHT), MAX_EXPANDED_BODY_HEIGHT);
        let left = (f64::from(self.shape.body_inset()) * self.scale).round() as i32;
        let top = (f64::from(self.shape.top_gap()) * self.scale).round() as i32;
        let right = left + (body_width * self.scale).round() as i32;
        let bottom = top + (body_height * self.scale).round() as i32;
        let radius = (f64::from(
            self.shape
                .lower_radius(body_height as f32, body_height > f64::from(FOLDED_HEIGHT)),
        ) * self.scale)
            .round() as i32;
        let geometry = (left, top, right, bottom, radius, self.shape);
        if self.drop_catcher_region != Some(geometry) {
            let round =
                unsafe { CreateRoundRectRgn(left, top, right, bottom, radius * 2, radius * 2) };
            if round.is_null() {
                unsafe { ShowWindow(catcher, SW_HIDE) };
                return;
            }
            if self.shape == ShapeMode::Notch {
                // The notch has square upper corners and rounded lower corners.
                let upper = unsafe { CreateRectRgn(left, top, right, bottom - radius) };
                if !upper.is_null() {
                    unsafe { CombineRgn(round, round, upper, RGN_OR) };
                    unsafe { DeleteObject(upper as HGDIOBJ) };
                }
            }
            if unsafe { SetWindowRgn(catcher, round, 0) } == 0 {
                unsafe { DeleteObject(round as HGDIOBJ) };
                unsafe { ShowWindow(catcher, SW_HIDE) };
                return;
            }
            // SetWindowRgn owns the region after success.
            self.drop_catcher_region = Some(geometry);
        }
        unsafe {
            SetWindowPos(
                catcher,
                self.window,
                self.position.x,
                self.position.y,
                self.width,
                self.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    fn create_react_view(&mut self, sender: SyncSender<String>) -> Result<(), String> {
        use wry::WebViewBuilderExtWindows;

        let window = self.window as isize;
        let parent = ChildWindowHandle(self.window);
        let html = include_str!("../../../island-ui/dist/index.html");
        // WebView2's implicit profile follows the executable path. Release
        // binaries in a new folder otherwise lose origin storage on relaunch.
        let profile = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Island")
            .join("WebView2");
        std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
        let mut web_context = wry::WebContext::new(Some(profile));
        let bootstrap = load_agent_preferences().unwrap_or(serde_json::Value::Null);
        let init_script = format!("window.islandAgentPreferenceBootstrap = {};", bootstrap);
        let builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
            .with_initialization_script(init_script)
            // Serve the inline React bundle from a trusted HTTPS virtual origin.
            // Screen capture and BarcodeDetector are secure-context APIs.
            .with_custom_protocol("island".to_owned(), move |_, _| {
                wry::http::Response::builder()
                    .header(wry::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(std::borrow::Cow::Borrowed(html.as_bytes()))
                    .expect("the embedded Island page response is valid")
            })
            .with_https_scheme(true)
            .with_url("island://localhost/")
            // WebView2's first visible compositor frame otherwise paints its
            // default white surface before the embedded CSS is presented.
            // Alpha zero lets the already-painted native island show through.
            .with_background_color((0, 0, 0, 0))
            .with_visible(false)
            .with_focused(false)
            .with_bounds(wry::Rect {
                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                size: wry::dpi::PhysicalSize::new(1, 1).into(),
            })
            .with_ipc_handler(move |request| {
                let _ = sender.try_send(request.body().to_owned());
                unsafe { PostMessageW(window as HWND, WM_APP_REACT, 0, 0) };
            });
        match builder.build_as_child(&parent) {
            Ok(view) => {
                let _ = view.set_visible(false);
                let bounds = self.react_panel_bounds();
                if view
                    .set_bounds(wry::Rect {
                        position: wry::dpi::PhysicalPosition::new(bounds.0, bounds.1).into(),
                        size: wry::dpi::PhysicalSize::new(bounds.2, bounds.3).into(),
                    })
                    .is_ok()
                {
                    self.react_bounds = Some(bounds);
                }
                self.react_view = Some(view);
                self.refresh_react_drop_targets();
                Ok(())
            }
            Err(error) => Err(format!(
                "Island needs WebView2 to show its React interface: {error}"
            )),
        }
    }

    fn react_state(&self, include_media_artwork: bool, include_shelf_icons: bool) -> String {
        let modules = serde_json::json!({
            "clipboard": self.modules.clipboard,
            "shelf": self.modules.shelf,
            "convert": self.modules.convert,
            "text": self.modules.text,
            "agents": self.modules.agents,
            "note": self.modules.note,
            "weather": self.modules.weather,
            "controls": self.modules.controls,
        });
        let mut power = SYSTEM_POWER_STATUS::default();
        let power_available = unsafe { GetSystemPowerStatus(&mut power) } != 0;
        let battery_percent = power_available
            .then(|| battery_percentage(power.BatteryFlag, power.BatteryLifePercent))
            .flatten();
        let battery_plugged = battery_percent.is_some() && power.ACLineStatus == 1;
        let battery_charging = battery_percent.is_some() && power.BatteryFlag & 0x08 != 0;
        let mut state = serde_json::json!({
            "page": self.page.title(),
            "dark": self.dark_theme,
            // The WebView stays mounted through the native silhouette spring.
            // Switch its content only when the expanded viewport fits.
            "expanded": self.react_visible,
            // A compact layout reports its committed generation before the
            // child WebView is resized to the compact viewport.
            "compactRenderGeneration": self.react_compact_render_generation,
            "pinned": self.callbacks.is_pinned.load(Ordering::Acquire),
            "status": self.status_label,
            "microphone": self.activity.microphone_active,
            "camera": self.activity.camera_active,
            "activeRuns": self.acp_opencode.active_count + self.acp_codex.active_count,
            "modules": modules,
            "activity": self.activity.details,
            "agentRuns": self.activity.agent_runs,
            "opencode": self.opencode,
            "clipboard": self.clipboard_items.iter().take(self.clipboard_limit).collect::<Vec<_>>(),
            "clipboardCapturing": self.clipboard_history_enabled && self.clipboard_listener_registered,
            "shelf": self.shelf_items.iter().map(|path| path.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "shelfRevision": self.shelf_revision,
            "text": self.text_draft,
            "note": self.note,
            "noteFolders": self.notebook.folders.iter().map(|folder| &folder.name).collect::<Vec<_>>(),
            "noteFolderSelected": self.notebook.selected,
            "noteReminderAt": self.notebook.reminder_at,
            "noteReminderDue": self.notebook.reminder_at.is_some_and(|at| at <= unix_seconds()),
            "commandResult": self.command_result,
            "commandError": self.command_error,
            "media": self.media,
            "outputVolume": self.output_volume,
            "outputDevices": self.audio_devices,
            "defaultOutputDeviceId": self.default_output_device_id,
            "selectedOutputDeviceId": self.selected_output_device_id,
            "batteryPercent": battery_percent,
            "radios": self.radios,
            "radioBusy": self.radio_busy,
            "powerNotice": self.power_notice,
            "weatherCity": self.weather_city,
            "weatherLines": self.weather_lines,
            "weatherData": self.weather_data,
            "weatherLoading": self.weather_receiver.is_some(),
            "weatherUpdatedAt": self.weather_updated_at,
            "weatherError": self.weather_error,
            "speechSettings": self.speech_settings.public_state(),
        });
        state["wakeWord"] = serde_json::json!({
            "status": self.wake_word_status,
            "phrase": self.wake_word_phrase,
            "error": self.wake_word_error,
            "revision": self.wake_word_revision,
            "surface": self.wake_surface || self.wake_closing,
            "local": true,
        });
        state["foregroundApp"] = serde_json::json!(self.foreground_app);
        state["conversion"] = serde_json::json!(self.converter.state);
        state["dropIntent"] = serde_json::json!(self.callbacks.drop_intent.load(Ordering::Acquire));
        state["dropActive"] =
            serde_json::json!(self.callbacks.shelf_drag_active.load(Ordering::Acquire));
        state["dropHandoffId"] = serde_json::json!(self.drop_handoff_sequence);
        state["agents"] =
            serde_json::json!({"opencode": self.acp_opencode, "codex": self.acp_codex});
        let approvals = [(&self.acp_opencode, "opencode"), (&self.acp_codex, "codex")];
        let approval_count = approvals
            .iter()
            .map(|(snapshot, _)| snapshot.permissions.len() + snapshot.questions.len())
            .sum::<usize>();
        let mut waiting = Vec::new();
        for (snapshot, provider) in approvals {
            waiting.extend(snapshot.permissions.iter().map(|item| {
                (
                    provider,
                    item.action.as_str(),
                    item.owner_session_id.as_str(),
                    "approval",
                    item.raised_at,
                )
            }));
            waiting.extend(snapshot.questions.iter().map(|item| {
                (
                    provider,
                    item.message.as_str(),
                    item.owner_session_id.as_str(),
                    "question",
                    item.raised_at,
                )
            }));
        }
        state["agentApproval"] = waiting.into_iter().min_by_key(|item| item.4)
            .map(|(provider, action, session_id, kind, _)| serde_json::json!({"provider":provider,"action":action,"sessionId":session_id,"kind":kind,"count":approval_count}))
            .unwrap_or(serde_json::Value::Null);
        state["homeActions"] = serde_json::json!(&self.home_actions);
        state["shelfKinds"] = serde_json::json!(
            self.shelf_items
                .iter()
                .map(|path| shelf_item_kind(path))
                .collect::<Vec<_>>()
        );
        state["accentGradient"] = serde_json::json!(self.accent_gradient.as_str());
        state["mediaCardStyle"] = serde_json::json!(self.media_card_style.as_str());
        state["compactFaded"] = serde_json::json!(self.react_compact_faded);
        state["clipboardCaptureEnabled"] = serde_json::json!(self.clipboard_history_enabled);
        state["clipboardHistoryLimit"] = serde_json::json!(self.clipboard_limit);
        state["weatherUnits"] =
            serde_json::json!(if self.weather_units == WeatherUnits::Imperial {
                "imperial"
            } else {
                "metric"
            });
        state["startup"] = serde_json::json!({
            "status": format!("{:?}", self.startup_status),
            "enabled": self.startup_status.is_enabled(),
            "canChange": self.startup_status.can_change(),
            "description": self.startup_status.description(),
            "busy": self.startup_pending,
        });
        state["batteryPlugged"] = serde_json::json!(battery_plugged);
        state["batteryCharging"] = serde_json::json!(battery_charging);
        state["shellSettings"] = serde_json::json!({
            "shape": if self.shape == ShapeMode::Capsule { "capsule" } else { "notch" },
            "privacy": self.privacy_indicator_enabled,
            "hideFullscreen": self.hide_in_fullscreen,
            "excludeFromScreenCapture": self.exclude_from_screen_capture,
            "screenCaptureExclusionSupported": self.screen_capture_exclusion_supported,
            "reducedMotion": self.reduced_motion,
            "hoverOpen": self.hover_open_delay_ms,
            "hoverClose": self.hover_close_delay_ms,
            "openOnClick": self.click_to_open,
            "closeOnOutsideClick": self.close_on_outside_click,
            "foldedTransparency": self.folded_transparency_percent,
            "openTransparency": self.open_transparency_percent,
        });
        let now = std::time::Instant::now();
        let compact_feedbacks = self.compact_feedbacks.iter()
            .filter(|feedback| feedback.expires_at.is_none_or(|deadline| now < deadline)
                && feedback.stale_at.is_none_or(|deadline| now < deadline)
                && feedback.next_visible_at.is_none_or(|deadline| now >= deadline))
            .map(|feedback| serde_json::json!({
                "id": feedback.id,
                "label": feedback.label,
                "kind": feedback.kind,
                "phase": feedback.phase,
                "repeatCount": feedback.repeat_count,
                "copy": feedback.copy_value.is_some(),
                "color": feedback.color.map(|(red, green, blue)| format!("#{red:02X}{green:02X}{blue:02X}")),
                "filePath": feedback.file_path,
            }))
            .collect::<Vec<_>>();
        state["compactFeedbacks"] = serde_json::json!(compact_feedbacks);
        state["compactFeedback"] = serde_json::json!(self.compact_feedbacks.iter()
            .find(|feedback| Some(feedback.id) == self.compact_feedback_visible_id)
            .filter(|feedback| feedback.expires_at.is_none_or(|deadline| now < deadline)
                && feedback.stale_at.is_none_or(|deadline| now < deadline)
                && feedback.next_visible_at.is_none_or(|deadline| now >= deadline))
            .map(|feedback| serde_json::json!({
                "id": feedback.id,
                "label": feedback.label,
                "kind": feedback.kind,
                "phase": feedback.phase,
                "repeatCount": feedback.repeat_count,
                "copy": feedback.copy_value.is_some(),
                "color": feedback.color.map(|(red, green, blue)| format!("#{red:02X}{green:02X}{blue:02X}")),
                "filePath": feedback.file_path,
            })));
        state["clockLabel"] = serde_json::json!(current_clock_label());
        state["clockEnabled"] = serde_json::json!(self.clock_indicator_enabled);
        state["batteryEnabled"] = serde_json::json!(self.battery_indicator_enabled);
        state["homeClipboard"] = serde_json::json!(
            self.clipboard_items
                .iter()
                .filter(|item| !self.screen_clipboard_values.contains(*item))
                .take(3)
                .collect::<Vec<_>>()
        );
        state["commandApps"] = serde_json::json!(
            self.command_app_results
                .iter()
                .map(|name| serde_json::json!({
                    "name": name,
                    "icon": self.command_app_icons.get(name).and_then(Option::as_deref),
                }))
                .collect::<Vec<_>>()
        );
        state["commandAppsLoading"] = serde_json::json!(self.command_apps_loading);
        state["commandFiles"] = serde_json::json!(&self.command_file_results);
        state["commandFilesLoading"] = serde_json::json!(
            !self.command_file_query.is_empty() && self.command_file_receiver.is_some()
        );
        if let (Some(media), Some(media_state)) = (
            self.media.as_ref(),
            state
                .get_mut("media")
                .and_then(serde_json::Value::as_object_mut),
        ) {
            media_state.insert(
                "artworkRevision".to_owned(),
                serde_json::json!(media.artwork_revision),
            );
            if include_media_artwork {
                media_state.insert(
                    "artwork".to_owned(),
                    serde_json::json!(media.artwork.as_deref()),
                );
            }
        }
        if include_shelf_icons {
            state["shelfIcons"] = serde_json::json!(&self.shelf_icon_data);
        }
        state.to_string()
    }

    fn push_react_state(&mut self) {
        if !self.react_ready {
            return;
        }
        let media_artwork_revision = self.media.as_ref().map(|media| media.artwork_revision);
        let include_media_artwork = self
            .media
            .as_ref()
            .is_some_and(|media| self.last_react_artwork_revision != Some(media.artwork_revision));
        let include_shelf_icons = self.last_react_shelf_revision != Some(self.shelf_revision);
        let state = self.react_state(include_media_artwork, include_shelf_icons);
        if state == self.last_react_state {
            return;
        }
        if let Some(view) = &self.react_view
            && view
                .evaluate_script(&format!("window.islandReceive?.({state});"))
                .is_ok()
        {
            self.last_react_state = state;
            self.last_react_artwork_revision = media_artwork_revision;
            self.last_react_shelf_revision = Some(self.shelf_revision);
        }
    }

    fn push_react_media(&mut self) {
        if !self.react_ready || (!self.react_visible && !self.react_compact_visible) {
            return;
        }
        let media_revision = self.media.as_ref().map(|media| media.artwork_revision);
        let include_artwork = self
            .media
            .as_ref()
            .is_some_and(|media| self.last_react_artwork_revision != Some(media.artwork_revision));
        let mut media_state = self
            .media
            .as_ref()
            .map_or(serde_json::Value::Null, |media| {
                serde_json::to_value(media).unwrap_or(serde_json::Value::Null)
            });
        if let (Some(media), Some(fields)) = (self.media.as_ref(), media_state.as_object_mut()) {
            fields.insert(
                "artworkRevision".to_owned(),
                serde_json::json!(media.artwork_revision),
            );
            if include_artwork {
                fields.insert(
                    "artwork".to_owned(),
                    serde_json::json!(media.artwork.as_deref()),
                );
            }
        }
        if let Some(view) = &self.react_view
            && view
                .evaluate_script(&format!("window.islandReceiveMedia?.({media_state});"))
                .is_ok()
        {
            self.last_react_artwork_revision = media_revision;
        }
    }

    fn react_panel_bounds(&self) -> (i32, i32, u32, u32) {
        let x = ((f64::from(self.shape.body_inset()) + 12.0) * self.scale).round() as i32;
        // The 12-point horizontal inset stays inside the 32-point capsule
        // corner at this 16-point vertical inset, including the lower edge.
        // This gives every page usable height without exposing the child HWND.
        let y = ((f64::from(self.shape.top_gap()) + 16.0) * self.scale).round() as i32;
        let width = ((self.expanded_body_width - 24.0) * self.scale)
            .round()
            .max(1.0) as u32;
        // Keep the WebView at its final size while the native contour moves.
        // Repeated child HWND resizes compete with painting and delay content.
        let content_height = self.expanded_layout_height();
        let height = ((content_height - 32.0) * self.scale).round().max(1.0) as u32;
        (x, y, width, height)
    }

    fn react_panel_bounds_at_width(&self, _body_width: f64) -> (i32, i32, u32, u32) {
        let (_, y, width, height) = self.react_panel_bounds();
        // Keep one final-size WebView so content does not reflow as the shell
        // grows. Move that fixed surface with the centered shell; the parent
        // clips it until the matching part of the native contour has arrived.
        let width = centered_pixel_width(f64::from(width), self.screen_width);
        let x = centered_pixel_origin(self.width, width as i32);
        (x, y, width, height)
    }

    fn react_compact_bounds(&self) -> (i32, i32, u32, u32) {
        // Follow the native spring so privacy controls do not jump straight to
        // the final edge when the contextual toolbar closes.
        let pill_width = if self.height_motion.value() <= f64::from(FOLDED_HEIGHT) + 1.0
            && self.animation_target.1 <= f64::from(FOLDED_HEIGHT) + 1.0
        {
            self.width_motion.value()
        } else {
            self.folded_target_width()
        };
        let width = centered_pixel_width((pill_width - 8.0) * self.scale, self.screen_width);
        let x = centered_pixel_origin(self.width, width as i32);
        let y = ((f64::from(self.shape.top_gap()) + 2.0) * self.scale).round() as i32;
        // All compact states, including screen-tool feedback, share one row.
        // A taller WebView child could paint before the native height spring
        // reached it, leaving the bottom of the label below the notch.
        (x, y, width, (28.0 * self.scale).round().max(1.0) as u32)
    }

    fn sync_react_view(&mut self, body_width: f64, body_height: f64) {
        if self.react_view.is_none() {
            return;
        }
        // The child is one persistent content surface. Keep its compact
        // viewport centered while the native hit region morphs; do not hide
        // and recreate it between compact and expanded states.
        let now = std::time::Instant::now();
        let expansion_active = self.wants_expanded_without_exit()
            || self
                .content_exit_deadline
                .is_some_and(|deadline| now < deadline);
        let content_can_enter = body_width
            >= self.expanded_body_width - REACT_CONTENT_REVEAL_MARGIN
            && body_height >= self.expanded_layout_height() - REACT_CONTENT_REVEAL_MARGIN;
        let visible = self.react_ready
            && self.reducer.snapshot().mode != ShellMode::Tucked
            && expansion_active
            && (self.react_visible || content_can_enter);
        let compact_visible = self.react_ready
            && (self.reducer.snapshot().mode != ShellMode::Tucked
                || self.tuck_motion.value() < 0.999)
            && !visible
            && self
                .shelf_drop_transition_at
                .is_none_or(|at| at.elapsed() >= DROP_HANDOFF_DURATION)
            && !self.callbacks.shelf_drag_active.load(Ordering::Acquire)
            && !self.callbacks.shelf_drag_hover_hint.load(Ordering::Acquire)
            && !self.fullscreen_hidden;
        // Every transition into the compact surface needs a fresh committed
        // React frame, including resumes from hidden/tucked states where the
        // expanded flag may already be false. Otherwise the child HWND can be
        // resized before its compact tree has painted and expose the black
        // native backing surface.
        let entering_compact = compact_visible && !self.react_compact_visible;
        let entering_expanded = visible && !self.react_visible;
        // If the user reopens during the handoff, invalidate its acknowledgement.
        // A stale compact render must never resize a newly expanded surface.
        if visible && self.pending_react_compact_render.take().is_some() {
            self.pending_react_compact_since = None;
            self.react_compact_render_generation =
                self.react_compact_render_generation.wrapping_add(1).max(1);
        }
        if entering_compact {
            self.react_compact_render_generation =
                self.react_compact_render_generation.wrapping_add(1).max(1);
            self.pending_react_compact_render = Some(self.react_compact_render_generation);
            self.pending_react_compact_since = Some(now);
        } else if !visible && !compact_visible {
            // Tucking or hiding cancels an in-flight compact handoff.
            self.pending_react_compact_render = None;
            self.pending_react_compact_since = None;
        }
        // Keep the expanded-size WebView centered while React commits the
        // compact tree. Shrinking first can expose a blank frame in WebView2.
        let awaiting_compact_render =
            compact_visible && self.pending_react_compact_render.is_some();
        let bounds = if awaiting_compact_render && self.react_visible {
            self.react_panel_bounds_at_width(body_width)
        } else if compact_visible || self.reducer.snapshot().mode == ShellMode::Tucked {
            self.react_compact_bounds()
        } else {
            self.react_panel_bounds_at_width(body_width)
        };
        if self.react_bounds != Some(bounds)
            && self.react_view.as_ref().is_some_and(|view| {
                view.set_bounds(wry::Rect {
                    position: wry::dpi::PhysicalPosition::new(bounds.0, bounds.1).into(),
                    size: wry::dpi::PhysicalSize::new(bounds.2, bounds.3).into(),
                })
                .is_ok()
            })
        {
            self.react_bounds = Some(bounds);
        }
        // Do not reveal compact content while the native silhouette is still
        // too tall to clip the centered, fixed-size child surface.
        let compact_faded = compact_visible && (awaiting_compact_render || body_height > 72.0);
        // Visibility belongs to the same snapshot as the compact tree. An
        // independent fire-and-forget setter can leave a committed tree faded.
        let compact_fade_changed = compact_faded != self.react_compact_faded;
        self.react_compact_faded = compact_faded;
        if visible != self.react_visible || compact_visible != self.react_compact_visible {
            trace_drop(format_args!(
                "react expanded={visible} compact={compact_visible} light={}",
                !self.dark_theme
            ));
            // Queue the state transition while the WebView is still hidden.
            // Revealing first can expose one compositor frame of the previous
            // expanded React tree inside the compact viewport (a blank pill).
            self.react_visible = visible;
            self.react_compact_visible = compact_visible;
            self.push_react_state();
            if let Some(view) = &self.react_view {
                let _ = view.set_visible(visible || compact_visible);
            }
            if entering_expanded {
                if let Some(view) = &self.react_view {
                    let _ = view.evaluate_script("window.islandShellPhase?.('enter');");
                }
            }
            if entering_compact {
                self.refresh_react_drop_targets();
                self.drop_targets_refresh_attempts = 2;
                self.next_drop_targets_refresh =
                    std::time::Instant::now() + std::time::Duration::from_millis(120);
            }
            if visible {
                self.react_ever_visible = true;
                // WebView2 can report a provisional scroll height while its
                // child HWND is hidden. Measure again after the first paint.
                if let Some(view) = &self.react_view {
                    let _ = view.evaluate_script(
                        "requestAnimationFrame(() => window.islandRequestLayout?.());",
                    );
                }
                self.shelf_drop_transition_at = None;
                // Hidden WebView2 child HWNDs are not reliable drop targets
                // until the view is shown. Rebind now and retry after its
                // asynchronous compositor/child-window initialization.
                self.refresh_react_drop_targets();
                self.drop_targets_refresh_attempts = 3;
                self.next_drop_targets_refresh =
                    std::time::Instant::now() + std::time::Duration::from_millis(120);
            } else if !compact_visible {
                self.drop_targets_refresh_attempts = 0;
                self.content_exit_deadline = None;
            }
        }
        if compact_fade_changed
            || ((visible || compact_visible)
                && self.width_motion.is_settled()
                && self.height_motion.is_settled())
        {
            self.push_react_state();
        }
        let now = std::time::Instant::now();
        if (self.react_visible || self.react_compact_visible)
            && self.drop_targets_refresh_attempts > 0
            && now >= self.next_drop_targets_refresh
        {
            if self.callbacks.shelf_drag_active.load(Ordering::Acquire) {
                self.next_drop_targets_refresh = now + std::time::Duration::from_millis(150);
            } else {
                self.refresh_react_drop_targets();
                self.drop_targets_refresh_attempts -= 1;
                self.next_drop_targets_refresh = now + std::time::Duration::from_millis(250);
            }
        }
    }

    fn drain_react_messages(&mut self) {
        while let Ok(mut message) = self.react_messages.try_recv() {
            if message.len() > 8 * 1024 * 1024 {
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&message) else {
                continue;
            };
            let Some(kind) = value["kind"].as_str() else {
                continue;
            };
            let mut redraw = true;
            match kind {
                "conversionPick" => {
                    if !self.converter.state.busy {
                        if let Some(paths) = pick_opencode_files(self.window) {
                            self.open_conversion(
                                paths.into_iter().map(std::path::PathBuf::from).collect(),
                                false,
                            );
                        }
                    }
                }
                "conversionDestination" => {
                    if !self.converter.state.busy {
                        if let Some(path) = pick_opencode_folder(self.window) {
                            self.converter.destination(path);
                        }
                    }
                }
                "conversionDestinationClear" => self.converter.destination(String::new()),
                "conversionEngine" => {
                    if !self.converter.state.busy {
                        if let Some(paths) = pick_opencode_files(self.window) {
                            if let Some(path) = paths.first() {
                                self.converter.engine(path.into());
                            }
                        }
                    }
                }
                "conversionEngineInstall" => self.converter.install_engine(),
                "conversionEngineCheckUpdate" => self.converter.check_engine_update(),
                "conversionEngineRemove" => self.converter.remove_engine(),
                "conversionEngineForget" => self.converter.forget_engine(),
                "systemUpdate" => match value["component"].as_str() {
                    Some("ffmpeg") if self.converter.state.update_available => {
                        self.converter.install_engine()
                    }
                    Some("opencode")
                        if self.opencode.update_available
                            && self.opencode.active_count == 0
                            && self.acp_opencode.active_count == 0 =>
                    {
                        let _ = self
                            .opencode_actions
                            .send(crate::opencode_control::Action::Update);
                    }
                    Some("codex")
                        if self.acp_codex.update_available && self.acp_codex.active_count == 0 =>
                    {
                        let _ = self
                            .acp_codex_actions
                            .send(crate::agent_acp::Action::Install);
                    }
                    _ => {}
                },
                "conversionStart" => {
                    if let Ok(profile) = serde_json::from_value(value["profile"].clone()) {
                        let ids = value["ids"]
                            .as_array()
                            .map(|ids| ids.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
                            .unwrap_or_default();
                        self.converter.start(profile, &ids);
                    }
                }
                "conversionCancel" => self.converter.cancel(),
                "conversionRemove" => self.converter.remove(value["id"].as_u64().unwrap_or(0)),
                "conversionOpen" => {
                    if let Some(path) = self.converter.output(value["id"].as_u64().unwrap_or(0)) {
                        open_shelf_path(self.window, &path);
                    }
                }
                "conversionShelf" => {
                    if let Some(path) = self.converter.output(value["id"].as_u64().unwrap_or(0)) {
                        self.accept_dropped_paths(vec![path]);
                    }
                }
                "compactContext" => {
                    self.compact_context_tools = if self.click_to_open {
                        value["count"].as_u64().unwrap_or(0).min(5) as usize
                    } else {
                        0
                    };
                    self.request_tick();
                    redraw = false;
                }
                "compactPointer" => {
                    self.callbacks.pointer_inside.store(
                        value["inside"].as_bool().unwrap_or(false),
                        Ordering::Release,
                    );
                    self.callbacks
                        .pointer_poll_needed
                        .store(true, Ordering::Release);
                    redraw = false;
                }
                "compactRenderReady" => {
                    let generation = value["generation"].as_u64();
                    if generation.is_some() && generation == self.pending_react_compact_render {
                        self.pending_react_compact_render = None;
                        self.pending_react_compact_since = None;
                        self.react_compact_render_ready = true;
                        self.request_tick();
                    }
                    redraw = false;
                }
                "compactResync" => {
                    if value["generation"].as_u64() == Some(self.react_compact_render_generation)
                        && self.react_compact_visible
                        && !self.wants_expanded_without_exit()
                    {
                        self.last_react_state.clear();
                        self.react_compact_render_ready = true;
                        self.request_tick();
                    }
                    redraw = false;
                }
                "compactOpen" => {
                    if let Some(page) = value["page"].as_str().and_then(|name| {
                        ShellPage::ALL.into_iter().find(|page| page.title() == name)
                    }) && page.enabled(&self.modules)
                    {
                        self.page = page;
                    }
                    self.open_from_click();
                    self.push_react_state();
                }
                "compactStatusVisible" => {
                    let id = value["id"].as_str().unwrap_or("");
                    let feedback_id = value["feedbackId"].as_u64();
                    let presentation_visible = value["visible"].as_bool().unwrap_or(true);
                    let valid_id = !id.is_empty()
                        && id.len() <= 48
                        && id.bytes().all(|byte| {
                            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                        });
                    if valid_id {
                        self.compact_status_visible = id.to_owned();
                        if id == "updates" {
                            self.hover_intent_deadline = None;
                        }
                        self.compact_feedback_visible_id = feedback_id;
                        self.compact_status_presentation_visible = presentation_visible;
                        self.sync_compact_feedback_clocks();
                        self.request_tick();
                    }
                    redraw = false;
                }
                "compactFeedbackCopy" => {
                    let feedback_id = value["feedbackId"]
                        .as_u64()
                        .or(self.compact_feedback_visible_id);
                    if let Some(value) = feedback_id
                        .and_then(|id| {
                            self.compact_feedbacks
                                .iter()
                                .find(|feedback| feedback.id == id)
                        })
                        .and_then(|feedback| feedback.copy_value.as_ref())
                    {
                        let _ = write_clipboard_text(value);
                    }
                    redraw = false;
                }
                "reminderNoticed" => {
                    let feedback_id = value["feedbackId"]
                        .as_u64()
                        .or(self.compact_feedback_visible_id);
                    if let Some(index) = feedback_id.and_then(|id| {
                        self.compact_feedbacks
                            .iter()
                            .position(|feedback| feedback.id == id && feedback.kind == "reminder")
                    }) {
                        self.compact_feedbacks.remove(index);
                        if self.compact_feedback_visible_id == feedback_id {
                            self.compact_feedback_visible_id = None;
                        }
                        self.sync_compact_feedback_clocks();
                        self.push_react_state();
                    }
                }
                "ready" => {
                    trace_drop(format_args!("react ready light={}", !self.dark_theme));
                    self.react_ready = true;
                    self.last_react_state.clear();
                    self.last_react_artwork_revision = None;
                    self.last_react_shelf_revision = None;
                }
                "agentPreferencesSave" => {
                    let previous_sensitivity = wake_sensitivity();
                    save_agent_preferences(&value["preferences"]);
                    if previous_sensitivity != wake_sensitivity()
                        && matches!(self.wake_word_status.as_str(), "arming" | "listening")
                    {
                        match self
                            .wake_word
                            .arm(&self.wake_word_phrase, wake_sensitivity())
                        {
                            Ok(_) => {
                                self.wake_word_status = "arming".into();
                                self.wake_word_error = None;
                            }
                            Err(error) => {
                                self.wake_word_status = "error".into();
                                self.wake_word_error = Some(error);
                            }
                        }
                        self.push_react_state();
                    }
                    redraw = false;
                }
                "holdSpaceBegin" => {
                    unsafe {
                        SetTimer(self.window, HOLD_SPACE_TIMER_ID, 30, None);
                    }
                    redraw = false;
                }
                "holdSpaceEnd" => {
                    unsafe {
                        KillTimer(self.window, HOLD_SPACE_TIMER_ID);
                    }
                    redraw = false;
                }
                "wakeCaptureReady" => {
                    unsafe {
                        let _ = MessageBeep(MB_OK);
                    }
                    redraw = false;
                }
                "voiceSurface" => {
                    self.wake_surface = value["open"].as_bool().unwrap_or(false);
                    if !self.wake_surface {
                        self.set_expanded_body_width(self.wide_layout);
                        if value["dismiss"].as_bool().unwrap_or(false) {
                            self.reducer
                                .dispatch(ShellEvent::Close, std::time::Instant::now());
                        }
                    }
                    self.push_react_state();
                    self.request_tick();
                }
                "wakeWordSet" => {
                    if value["enabled"].as_bool().unwrap_or(false) {
                        let phrase = value["phrase"].as_str().unwrap_or("Hey Island");
                        match self.wake_word.arm(phrase, wake_sensitivity()) {
                            Ok(phrase) => {
                                self.wake_word_status = "arming".into();
                                self.wake_word_phrase = phrase;
                                self.wake_word_error = None;
                            }
                            Err(error) => {
                                self.wake_word_status = "error".into();
                                self.wake_word_error = Some(error);
                            }
                        }
                    } else {
                        self.wake_word.disarm();
                        self.wake_word_status = "off".into();
                        self.wake_word_error = None;
                    }
                    self.wake_word_revision = self.wake_word_revision.wrapping_add(1);
                    self.push_react_state();
                    redraw = false;
                }
                "wakeWordResume" => {
                    if self.wake_word_status != "triggered" {
                        return;
                    }
                    let phrase = value["phrase"].as_str().unwrap_or(&self.wake_word_phrase);
                    match self.wake_word.arm(phrase, wake_sensitivity()) {
                        Ok(phrase) => {
                            self.wake_word_status = "arming".into();
                            self.wake_word_phrase = phrase;
                            self.wake_word_error = None;
                        }
                        Err(error) => {
                            self.wake_word_status = "error".into();
                            self.wake_word_error = Some(error);
                        }
                    }
                    self.wake_word_revision = self.wake_word_revision.wrapping_add(1);
                    self.push_react_state();
                    redraw = false;
                }
                "page" => {
                    if let Some(name) = value["page"].as_str()
                        && let Some(page) = ShellPage::ALL
                            .into_iter()
                            .find(|page| page.title() == name && page.enabled(&self.modules))
                    {
                        self.page = page;
                        if page == ShellPage::Weather {
                            self.refresh_weather_if_stale();
                        } else if page == ShellPage::Controls {
                            self.refresh_radios(None);
                        }
                    }
                }
                "opencodeRefresh" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::Refresh);
                }
                "opencodeMore" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::More);
                }
                "opencodeAgentsRefresh" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::RefreshAgents);
                }
                "opencodeSaveAgent" => {
                    if let (Some(id), Some(description), Some(mode), Some(model), Some(system)) = (
                        value["id"].as_str(),
                        value["description"].as_str(),
                        value["mode"].as_str(),
                        value["model"].as_str(),
                        value["system"].as_str(),
                    ) && id.len() <= 48
                        && description.len() <= 2_000
                        && system.len() <= 100_000
                        && model.len() <= 200
                    {
                        let _ = self.opencode_actions.send(
                            crate::opencode_control::Action::SaveAgent {
                                id: id.into(),
                                description: description.into(),
                                mode: mode.into(),
                                model: model.into(),
                                system: system.into(),
                            },
                        );
                    }
                }
                "opencodeDeleteAgent" => {
                    if let Some(id) = value["id"].as_str()
                        && id.len() <= 48
                    {
                        let _ = self
                            .opencode_actions
                            .send(crate::opencode_control::Action::DeleteAgent { id: id.into() });
                    }
                }
                "opencodeSelect" => {
                    if let Some(id) = value["sessionId"].as_str()
                        && id.len() <= 128
                    {
                        let _ = self
                            .opencode_actions
                            .send(crate::opencode_control::Action::Select(id.into()));
                    }
                }
                "opencodeInstall" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::Install);
                }
                "opencodeUpdate" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::Update);
                }
                "opencodeCheckUpdate" => {
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::CheckUpdate);
                }
                "opencodeCatalog" => {
                    let directory = value["directory"]
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 1024)
                        .map(str::to_owned);
                    let _ = self
                        .opencode_actions
                        .send(crate::opencode_control::Action::Catalog(directory));
                }
                "opencodePickFolder" => {
                    if let Some(folder) = pick_opencode_folder(self.window) {
                        if let Some(view) = self.react_view.as_ref() {
                            if let Ok(encoded) = serde_json::to_string(&folder) {
                                let _ = view.evaluate_script(&format!(
                                    "window.islandFolderPicked?.({encoded});"
                                ));
                            }
                        }
                    }
                    self.reducer
                        .dispatch(ShellEvent::Open, std::time::Instant::now());
                    self.request_tick();
                }
                "opencodePickFiles" => {
                    if let Some(paths) = pick_opencode_files(self.window) {
                        if let Some(view) = self.react_view.as_ref() {
                            if let Ok(encoded) = serde_json::to_string(&paths) {
                                let _ = view.evaluate_script(&format!(
                                    "window.islandFilesPicked?.({encoded});"
                                ));
                            }
                        }
                    }
                    self.reducer
                        .dispatch(ShellEvent::Open, std::time::Instant::now());
                    self.request_tick();
                }
                "opencodeStart" => {
                    if let Some(text) = value["text"].as_str()
                        && text.len() <= 400_000
                    {
                        let directory = value["directory"]
                            .as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 1024)
                            .map(str::to_owned);
                        let model = value["model"]
                            .as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 200)
                            .map(str::to_owned);
                        let agent = value["agent"]
                            .as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 48)
                            .map(str::to_owned);
                        let files = value["files"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .take(8)
                                    .filter_map(|item| {
                                        item.as_str()
                                            .filter(|path| path.len() <= 4096)
                                            .map(str::to_owned)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let _ =
                            self.opencode_actions
                                .send(crate::opencode_control::Action::Start {
                                    directory,
                                    text: text.into(),
                                    model,
                                    agent,
                                    files,
                                });
                    }
                }
                "opencodeSetModel" => {
                    if let (Some(session_id), Some(model)) =
                        (value["sessionId"].as_str(), value["model"].as_str())
                        && session_id.len() <= 128
                        && !model.is_empty()
                        && model.len() <= 200
                    {
                        let _ =
                            self.opencode_actions
                                .send(crate::opencode_control::Action::SetModel {
                                    session_id: session_id.into(),
                                    model: model.into(),
                                });
                    }
                }
                "opencodePrompt" => {
                    if let (Some(session_id), Some(text)) =
                        (value["sessionId"].as_str(), value["text"].as_str())
                        && session_id.len() <= 128
                        && text.len() <= 400_000
                    {
                        let agent = value["agent"]
                            .as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 48)
                            .map(str::to_owned);
                        let files = value["files"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .take(8)
                                    .filter_map(|item| {
                                        item.as_str()
                                            .filter(|path| path.len() <= 4096)
                                            .map(str::to_owned)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let _ =
                            self.opencode_actions
                                .send(crate::opencode_control::Action::Prompt {
                                    session_id: session_id.into(),
                                    text: text.into(),
                                    agent,
                                    files,
                                });
                    }
                }
                "opencodeAbort" => {
                    if let Some(session_id) = value["sessionId"].as_str()
                        && session_id.len() <= 128
                    {
                        let _ =
                            self.opencode_actions
                                .send(crate::opencode_control::Action::Abort {
                                    session_id: session_id.into(),
                                });
                    }
                }
                "opencodeReply" => {
                    if let (Some(session_id), Some(request_id), Some(allow)) = (
                        value["sessionId"].as_str(),
                        value["requestId"].as_str(),
                        value["allow"].as_bool(),
                    ) && session_id.len() <= 128
                        && request_id.len() <= 128
                    {
                        let _ =
                            self.opencode_actions
                                .send(crate::opencode_control::Action::Reply {
                                    session_id: session_id.into(),
                                    request_id: request_id.into(),
                                    allow,
                                });
                    }
                }
                "agentAction" => {
                    use crate::agent_acp::Action;
                    let mut push_agent_state = false;
                    let sender = match value["provider"].as_str() {
                        Some("opencode") => Some(&self.acp_opencode_actions),
                        Some("codex") => Some(&self.acp_codex_actions),
                        _ => None,
                    };
                    if let Some(sender) = sender {
                        let short = |key: &str, max: usize| {
                            value[key]
                                .as_str()
                                .filter(|item| item.len() <= max)
                                .map(str::to_owned)
                        };
                        let session = short("sessionId", 256);
                        let action = match value["action"].as_str() {
                            Some("connect") => Some(Action::Connect),
                            Some("install") if value["provider"] == "codex" => {
                                Some(Action::Install)
                            }
                            Some("checkUpdate") if value["provider"] == "codex" => {
                                Some(Action::CheckUpdate)
                            }
                            Some("install") if value["provider"] == "opencode" => {
                                if self.acp_opencode.active_count > 0
                                    || self.opencode.active_count > 0
                                    || self.opencode.installing
                                {
                                    self.acp_opencode.notice =
                                        "Finish active OpenCode tasks before updating.".into();
                                    self.acp_opencode.notice_severity = "warning".into();
                                    push_agent_state = true;
                                } else {
                                    let _ = self
                                        .opencode_actions
                                        .send(crate::opencode_control::Action::Install);
                                }
                                None
                            }
                            Some("checkUpdate") if value["provider"] == "opencode" => {
                                let _ = self
                                    .opencode_actions
                                    .send(crate::opencode_control::Action::CheckUpdate);
                                None
                            }
                            Some("refresh") => Some(Action::Refresh),
                            Some("more") => Some(Action::More),
                            Some("login") => short("methodId", 128).map(Action::Authenticate),
                            Some("logout") => Some(Action::Logout),
                            Some("select") => session.map(Action::Select),
                            Some("prepare") => Some(Action::Prepare {
                                directory: short("directory", 4096).unwrap_or_default(),
                                additional_directories: value["additionalDirectories"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .take(6)
                                            .filter_map(|item| {
                                                item.as_str()
                                                    .filter(|path| path.len() <= 4096)
                                                    .map(str::to_owned)
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                config: value["config"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .take(12)
                                            .filter_map(|item| {
                                                let id = item["id"].as_str()?;
                                                let selected = item["value"]
                                                    .as_str()
                                                    .map(str::to_owned)
                                                    .or_else(|| {
                                                        item["value"]
                                                            .as_bool()
                                                            .map(|value| value.to_string())
                                                    })?;
                                                (id.len() <= 128 && selected.len() <= 256)
                                                    .then(|| (id.into(), selected))
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            }),
                            Some("start") => short("text", 400_000).and_then(|text| {
                                let directory = short("directory", 4096).unwrap_or_default();
                                let system_prompt =
                                    short("systemPrompt", 24_000).unwrap_or_default();
                                let additional_directories = value["additionalDirectories"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .take(6)
                                            .filter_map(|item| {
                                                item.as_str()
                                                    .filter(|path| path.len() <= 4096)
                                                    .map(str::to_owned)
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let config = value["config"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .take(12)
                                            .filter_map(|item| {
                                                let id = item["id"].as_str()?;
                                                let selected = item["value"]
                                                    .as_str()
                                                    .map(str::to_owned)
                                                    .or_else(|| {
                                                        item["value"]
                                                            .as_bool()
                                                            .map(|value| value.to_string())
                                                    })?;
                                                (id.len() <= 128 && selected.len() <= 256)
                                                    .then(|| (id.into(), selected))
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                let files: Vec<String> = value["files"]
                                    .as_array()
                                    .map(|items| {
                                        items
                                            .iter()
                                            .take(8)
                                            .filter_map(|item| {
                                                item.as_str()
                                                    .filter(|path| path.len() <= 4096)
                                                    .map(str::to_owned)
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                if text.trim().is_empty() && files.is_empty() {
                                    return None;
                                }
                                Some(Action::Start {
                                    directory,
                                    additional_directories,
                                    text,
                                    config,
                                    files,
                                    system_prompt,
                                    hybrid_whiteboard: value["hybridWhiteboard"] == true,
                                })
                            }),
                            Some("prompt") => {
                                session
                                    .zip(short("text", 400_000))
                                    .map(|(session_id, text)| {
                                        let files = value["files"]
                                            .as_array()
                                            .map(|items| {
                                                items
                                                    .iter()
                                                    .take(8)
                                                    .filter_map(|item| {
                                                        item.as_str()
                                                            .filter(|path| path.len() <= 4096)
                                                            .map(str::to_owned)
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default();
                                        let system_prompt =
                                            short("systemPrompt", 24_000).unwrap_or_default();
                                        Action::Prompt {
                                            session_id,
                                            text,
                                            files,
                                            system_prompt,
                                            hybrid_whiteboard: value["hybridWhiteboard"] == true,
                                        }
                                    })
                            }
                            Some("cancel") => session.map(Action::Cancel),
                            Some("close") => session.map(Action::Close),
                            Some("delete") => session.map(Action::Delete),
                            Some("fork") => session.map(Action::Fork),
                            Some("configure") => session
                                .zip(short("configId", 128))
                                .zip(short("configValue", 256))
                                .map(|((session_id, id), value)| Action::Configure {
                                    session_id,
                                    id,
                                    value,
                                }),
                            Some("mode") => session
                                .zip(short("modeId", 128))
                                .map(|(session_id, mode)| Action::SetMode { session_id, mode }),
                            Some("reply") => session
                                .zip(short("requestId", 128))
                                .zip(short("optionId", 128))
                                .map(|((session_id, request_id), option_id)| Action::Reply {
                                    session_id,
                                    request_id,
                                    option_id,
                                }),
                            Some("answer") => session
                                .zip(short("requestId", 128))
                                .zip(short("choice", 16))
                                .and_then(|((session_id, request_id), choice)| {
                                    let content = value["content"].clone();
                                    (content.to_string().len() <= 32_000).then_some(
                                        Action::Answer {
                                            session_id,
                                            request_id,
                                            choice,
                                            content,
                                        },
                                    )
                                }),
                            _ => None,
                        };
                        if let Some(action) = action {
                            let _ = sender.send(action);
                        }
                    }
                    if push_agent_state {
                        self.push_react_state();
                    }
                }
                "agentLibrary" => {
                    use crate::platform::windows::agent_library as library;
                    let action = value["action"].as_str().unwrap_or("");
                    let operation = match action {
                        "mcpSave" => library::save_mcp_server(&value["server"]),
                        "mcpDelete" => value["name"]
                            .as_str()
                            .filter(|name| name.len() <= 64)
                            .ok_or_else(|| "Invalid MCP server name.".to_owned())
                            .and_then(library::delete_mcp_server),
                        "skillSave" => {
                            let name = value["name"].as_str().filter(|name| name.len() <= 64);
                            let description = value["description"]
                                .as_str()
                                .filter(|text| text.len() <= 1024);
                            let instructions = value["instructions"]
                                .as_str()
                                .filter(|text| text.len() <= 128_000);
                            match (name, description, instructions) {
                                (Some(name), Some(description), Some(instructions)) => {
                                    library::save_skill(name, description, instructions)
                                }
                                _ => Err("Skill fields exceed the supported limits.".to_owned()),
                            }
                        }
                        "skillDelete" => value["name"]
                            .as_str()
                            .filter(|name| name.len() <= 64)
                            .ok_or_else(|| "Invalid skill name.".to_owned())
                            .and_then(library::delete_skill),
                        "refresh" | "skillRead" | "projectEntries" => Ok(()),
                        _ => Err("Unknown agent library action.".to_owned()),
                    };
                    let mut result =
                        serde_json::json!({ "action": action, "ok": operation.is_ok() });
                    if let Err(error) = operation {
                        result["error"] = serde_json::json!(error);
                    }
                    if action == "projectEntries" {
                        let root = value["root"]
                            .as_str()
                            .filter(|path| path.len() <= 32_768)
                            .unwrap_or("");
                        let relative = value["relative"]
                            .as_str()
                            .filter(|path| path.len() <= 2_048)
                            .unwrap_or("");
                        let query = value["query"]
                            .as_str()
                            .filter(|query| query.len() <= 2_048)
                            .unwrap_or("");
                        result["requestId"] = value["requestId"].clone();
                        match library::list_project_entries(root, relative, query) {
                            Ok(project) => result["project"] = project,
                            Err(error) => {
                                result["ok"] = serde_json::json!(false);
                                result["error"] = serde_json::json!(error);
                            }
                        }
                    }
                    if action != "projectEntries" {
                        match library::list_mcp_servers() {
                            Ok(servers) => result["mcpServers"] = servers,
                            Err(error) => result["mcpError"] = serde_json::json!(error),
                        }
                        match library::list_skills() {
                            Ok(skills) => result["skills"] = skills,
                            Err(error) => result["skillsError"] = serde_json::json!(error),
                        }
                    }
                    if action == "skillRead" {
                        if let Some(name) = value["name"].as_str().filter(|name| name.len() <= 64) {
                            match library::read_skill(name) {
                                Ok(content) => {
                                    result["skill"] =
                                        serde_json::json!({"name":name,"content":content});
                                }
                                Err(error) => {
                                    result["ok"] = serde_json::json!(false);
                                    result["error"] = serde_json::json!(error);
                                }
                            }
                        }
                    }
                    if let Some(view) = &self.react_view
                        && let Ok(encoded) = serde_json::to_string(&result)
                    {
                        let _ = view.evaluate_script(&format!(
                            "window.islandAgentLibraryResult?.({encoded});"
                        ));
                    }
                    library::clear_mcp_input(&mut value);
                    unsafe {
                        for byte in message.as_mut_vec() {
                            std::ptr::write_volatile(byte, 0);
                        }
                    }
                    redraw = false;
                }
                "layout" => {
                    // ResizeObserver messages are asynchronous. A closing or
                    // previous presentation cannot change the next open target.
                    if !self.react_visible
                        || !self.wants_expanded_without_exit()
                        || value["generation"].as_u64()
                            != Some(self.react_compact_render_generation)
                        || value["page"].as_str() != Some(self.page.title())
                    {
                        continue;
                    }
                    let wide = value["wide"].as_bool().unwrap_or(false);
                    let settings_open = value["settingsOpen"].as_bool().unwrap_or(false);
                    let settings_layout_changed = settings_open != self.settings_layout_open;
                    if settings_layout_changed {
                        self.settings_layout_open = settings_open;
                        if settings_open {
                            self.settings_layout_height =
                                Some(480.0_f64.min(MAX_EXPANDED_BODY_HEIGHT));
                        } else {
                            self.settings_layout_height = None;
                        }
                        self.request_tick();
                    }
                    if self.wide_layout != wide && !self.wake_closing {
                        self.set_expanded_body_width(wide);
                        self.request_tick();
                    }
                    // A ResizeObserver callback can arrive after the view has
                    // switched back to the 28-point compact viewport. Its
                    // stale scroll height must never retarget the next open.
                    if self.react_visible
                        && !self.wake_closing
                        && !settings_open
                        && let Some(height) = value["height"].as_f64()
                        && height.is_finite()
                    {
                        let measured = (height.ceil() + 32.0).clamp(
                            if self.wake_surface {
                                90.0
                            } else {
                                f64::from(EXPANDED_HEIGHT)
                            },
                            MAX_EXPANDED_BODY_HEIGHT,
                        );
                        // Keep Settings' temporary height independent of the
                        // shared page measurement; otherwise other pages can
                        // inherit its larger viewport on close.
                        let next = measured;
                        trace_drop(format_args!(
                            "react layout height={next:.0} trusted={}",
                            self.react_ever_visible
                        ));
                        // A hidden WebView can briefly report a much taller
                        // scroll area on its first layout. Accept modest
                        // growth, but wait for the first visible paint before
                        // trusting a large jump.
                        if (self.react_ever_visible
                            || value["allowLarge"].as_bool().unwrap_or(false)
                            || next <= self.expanded_body_height + 64.0)
                            // Ignore sub-pixel and tiny ResizeObserver noise.
                            // Retargeting the shell for a one-point fluctuation
                            // near the spring's endpoint is perceived as a final
                            // notch wobble, especially at non-integer DPI scales.
                            && (next - self.expanded_body_height).abs() >= 4.0
                        {
                            self.expanded_body_height = next;
                            self.request_tick();
                        }
                    }
                    redraw = false;
                }
                "settings" => {
                    self.show_settings();
                }
                "indicatorToggle" => {
                    let field = value["indicator"].as_str().unwrap_or("");
                    let change = match field {
                        "clock" => {
                            self.clock_indicator_enabled = !self.clock_indicator_enabled;
                            ShellPreferenceChange::Clock(self.clock_indicator_enabled)
                        }
                        "battery" => {
                            self.battery_indicator_enabled = !self.battery_indicator_enabled;
                            ShellPreferenceChange::Battery(self.battery_indicator_enabled)
                        }
                        _ => continue,
                    };
                    self.clock_refresh_at = std::time::Instant::now();
                    if let Ok(mut changes) = self.callbacks.preference_changes.lock() {
                        changes.push(change);
                    }
                    self.callbacks.context.request_repaint();
                    self.push_react_state();
                }
                "appearanceSet" => {
                    let mut changes = Vec::with_capacity(2);
                    if let Some(gradient) = value["accentGradient"]
                        .as_str()
                        .and_then(crate::storage::StoredAccentGradient::parse)
                        && gradient != self.accent_gradient
                    {
                        self.accent_gradient = gradient;
                        changes.push(ShellPreferenceChange::AccentGradient(gradient));
                    }
                    if let Some(style) = value["mediaCardStyle"]
                        .as_str()
                        .and_then(crate::storage::StoredMediaCardStyle::parse)
                        && style != self.media_card_style
                    {
                        self.media_card_style = style;
                        changes.push(ShellPreferenceChange::MediaCardStyle(style));
                    }
                    if !changes.is_empty() {
                        if let Ok(mut pending) = self.callbacks.preference_changes.lock() {
                            pending.extend(changes);
                        }
                        self.callbacks.context.request_repaint();
                        self.push_react_state();
                    }
                }
                "shellSetting" => {
                    let key = value["key"].as_str().unwrap_or("");
                    let setting = &value["value"];
                    let change = match key {
                        "shape" if setting == "notch" => {
                            self.set_shape(ShapeMode::Notch);
                            Some(ShellPreferenceChange::Shape(ShapeMode::Notch))
                        }
                        "shape" if setting == "capsule" => {
                            self.set_shape(ShapeMode::Capsule);
                            Some(ShellPreferenceChange::Shape(ShapeMode::Capsule))
                        }
                        "privacy" if setting.is_boolean() => {
                            self.privacy_indicator_enabled = setting.as_bool().unwrap();
                            Some(ShellPreferenceChange::Privacy(
                                self.privacy_indicator_enabled,
                            ))
                        }
                        "hideFullscreen" if setting.is_boolean() => {
                            self.set_hide_in_fullscreen(setting.as_bool().unwrap());
                            Some(ShellPreferenceChange::HideFullscreen(
                                self.hide_in_fullscreen,
                            ))
                        }
                        "excludeFromScreenCapture"
                            if setting.is_boolean() && self.screen_capture_exclusion_supported =>
                        {
                            let enabled = setting.as_bool().unwrap();
                            self.set_exclude_from_screen_capture(enabled)
                                .then_some(ShellPreferenceChange::ScreenCaptureExclusion(enabled))
                        }
                        "reducedMotion" if setting.is_boolean() => {
                            let enabled = setting.as_bool().unwrap();
                            if self.set_reduced_motion(enabled).is_ok() {
                                self.callbacks
                                    .reduced_motion
                                    .store(enabled, Ordering::Release);
                                Some(ShellPreferenceChange::ReducedMotion(enabled))
                            } else {
                                None
                            }
                        }
                        "hoverOpen" if setting.as_u64().is_some_and(|value| value <= 600) => {
                            self.hover_open_delay_ms = setting.as_u64().unwrap() as u32;
                            self.reducer.set_hover_delays(
                                std::time::Duration::from_millis(u64::from(
                                    self.hover_open_delay_ms,
                                )),
                                std::time::Duration::from_millis(u64::from(
                                    self.hover_close_delay_ms,
                                )),
                            );
                            Some(ShellPreferenceChange::HoverOpen(self.hover_open_delay_ms))
                        }
                        "hoverClose"
                            if setting
                                .as_u64()
                                .is_some_and(|value| (100..=1500).contains(&value)) =>
                        {
                            self.hover_close_delay_ms = setting.as_u64().unwrap() as u32;
                            self.reducer.set_hover_delays(
                                std::time::Duration::from_millis(u64::from(
                                    self.hover_open_delay_ms,
                                )),
                                std::time::Duration::from_millis(u64::from(
                                    self.hover_close_delay_ms,
                                )),
                            );
                            Some(ShellPreferenceChange::HoverClose(self.hover_close_delay_ms))
                        }
                        "openOnClick" if setting.is_boolean() => {
                            self.click_to_open = setting.as_bool().unwrap();
                            self.hover_intent_deadline = None;
                            self.reducer.set_interaction_behavior(
                                self.click_to_open,
                                self.close_on_outside_click,
                            );
                            Some(ShellPreferenceChange::ClickToOpen(self.click_to_open))
                        }
                        "closeOnOutsideClick" if setting.is_boolean() => {
                            self.close_on_outside_click = setting.as_bool().unwrap();
                            self.reducer.set_interaction_behavior(
                                self.click_to_open,
                                self.close_on_outside_click,
                            );
                            Some(ShellPreferenceChange::CloseOnOutsideClick(
                                self.close_on_outside_click,
                            ))
                        }
                        "foldedTransparency"
                            if setting.as_u64().is_some_and(|value| value <= 100) =>
                        {
                            self.folded_transparency_percent = setting.as_u64().unwrap() as u8;
                            self.request_tick();
                            Some(ShellPreferenceChange::FoldedTransparency(
                                self.folded_transparency_percent,
                            ))
                        }
                        "openTransparency"
                            if setting.as_u64().is_some_and(|value| value <= 100) =>
                        {
                            self.open_transparency_percent = setting.as_u64().unwrap() as u8;
                            self.request_tick();
                            Some(ShellPreferenceChange::OpenTransparency(
                                self.open_transparency_percent,
                            ))
                        }
                        _ => None,
                    };
                    if let Some(change) = change {
                        if let Ok(mut changes) = self.callbacks.preference_changes.lock() {
                            changes.push(change);
                        }
                        self.callbacks.context.request_repaint();
                        self.push_react_state();
                    }
                }
                "clipboardSetting" => {
                    let capture_enabled = value["captureEnabled"]
                        .as_bool()
                        .unwrap_or(self.clipboard_history_enabled);
                    let history_limit = value["historyLimit"]
                        .as_u64()
                        .unwrap_or(self.clipboard_limit as u64)
                        as usize;
                    let settings = ClipboardSettings {
                        capture_enabled,
                        history_limit,
                    };
                    if save_clipboard_settings(settings) {
                        self.clipboard_history_enabled = capture_enabled;
                        self.clipboard_limit = history_limit;
                        self.clipboard_items.truncate(history_limit);
                        self.push_react_state();
                    }
                }
                "weatherUnitsSet" => {
                    let units = match value["units"].as_str() {
                        Some("metric") => WeatherUnits::Metric,
                        Some("imperial") => WeatherUnits::Imperial,
                        _ => continue,
                    };
                    if save_weather_units(units) {
                        self.weather_units = units;
                        self.weather_lines.clear();
                        self.weather_data = None;
                        self.weather_error = None;
                        self.weather_updated_at = None;
                        self.weather_last_attempt = None;
                        self.refresh_weather_if_stale();
                        self.push_react_state();
                    }
                }
                "moduleSetting" => {
                    let key = value["module"].as_str().unwrap_or("");
                    let Some(enabled) = value["enabled"].as_bool() else {
                        continue;
                    };
                    let mut modules = self.modules;
                    match key {
                        "clipboard" => modules.clipboard = enabled,
                        "shelf" => modules.shelf = enabled,
                        "convert" => modules.convert = enabled,
                        "text" => modules.text = enabled,
                        "agents" => modules.agents = enabled,
                        "note" => modules.note = enabled,
                        "weather" => modules.weather = enabled,
                        "controls" => modules.controls = enabled,
                        _ => continue,
                    }
                    if save_module_settings(modules) {
                        self.modules = modules;
                        self.callbacks
                            .shelf_enabled
                            .store(modules.shelf, Ordering::Release);
                        if !self.page.enabled(&modules) {
                            self.page = ShellPage::Home;
                        }
                        self.push_react_state();
                        self.request_tick();
                    }
                }
                "startupSet" => {
                    let requested = value["enabled"].as_bool();
                    if requested.is_none() || self.startup_status.can_change() {
                        self.request_startup_state(requested);
                    }
                }
                "pinToggle" => {
                    let now = std::time::Instant::now();
                    let pinned = self.callbacks.is_pinned.load(Ordering::Acquire);
                    self.reducer.dispatch(ShellEvent::EndTextEntry, now);
                    if !pinned {
                        self.reducer.dispatch(ShellEvent::Open, now);
                    }
                    self.reducer.dispatch(
                        if pinned {
                            ShellEvent::Unpin
                        } else {
                            ShellEvent::Pin
                        },
                        now,
                    );
                    if pinned && !self.callbacks.pointer_inside.load(Ordering::Acquire) {
                        self.reducer.dispatch(ShellEvent::PointerLeft, now);
                    }
                    self.callbacks.is_pinned.store(
                        self.reducer.snapshot().mode == ShellMode::Pinned,
                        Ordering::Release,
                    );
                    self.request_tick();
                }
                "textFocus" => {
                    let now = std::time::Instant::now();
                    self.reducer.dispatch(ShellEvent::Open, now);
                    self.reducer.dispatch(ShellEvent::BeginTextEntry, now);
                    self.request_tick();
                    redraw = false;
                }
                "textBlur" => {
                    let now = std::time::Instant::now();
                    self.reducer.dispatch(ShellEvent::EndTextEntry, now);
                    if !self.callbacks.pointer_inside.load(Ordering::Acquire) {
                        self.reducer.dispatch(ShellEvent::PointerLeft, now);
                    }
                    self.request_tick();
                    redraw = false;
                }
                "clipboardCapture" => {
                    if let Some(text) = read_clipboard_text()
                        && !text.is_empty()
                        && self.clipboard_items.first() != Some(&text)
                    {
                        self.clipboard_items.insert(0, text);
                        self.clipboard_items.truncate(self.clipboard_limit);
                    }
                }
                "clipboardClear" => {
                    self.clipboard_items.clear();
                    self.screen_clipboard_values.clear();
                }
                "clipboardCopy" => {
                    if let Some(index) = value["index"].as_u64()
                        && let Some(item) = self.clipboard_items.get(index as usize)
                    {
                        let _ = write_clipboard_text(item);
                    }
                }
                "opencodeCopy" => {
                    if let Some(text) = value["text"].as_str().filter(|text| text.len() <= 400_000)
                    {
                        let _ = write_clipboard_text(text);
                    }
                }
                "openExternalUrl" => {
                    if let Some(url) = value["url"].as_str().filter(|url| {
                        url.len() <= 2048
                            && !url.chars().any(char::is_control)
                            && (url.starts_with("https://") || url.starts_with("http://"))
                    }) {
                        let target = wide(url);
                        let operation = wide("open");
                        unsafe {
                            ShellExecuteW(
                                self.window,
                                operation.as_ptr(),
                                target.as_ptr(),
                                null(),
                                null(),
                                SW_SHOW,
                            )
                        };
                    }
                }
                "shelfOpen" => {
                    if let Some(index) = value["index"].as_u64()
                        && let Some(path) = self.shelf_items.get(index as usize)
                    {
                        open_shelf_path(self.window, path);
                    }
                }
                "homeActionsSet" => {
                    let Some(values) = value["actions"].as_array() else {
                        continue;
                    };
                    let mut actions = Vec::with_capacity(10);
                    let mut used = HashSet::new();
                    let mut valid = values.len() == 10;
                    for entry in values.iter().take(10) {
                        match entry {
                            serde_json::Value::Null => actions.push(None),
                            serde_json::Value::String(id)
                                if HOME_ACTION_IDS.contains(&id.as_str())
                                    && used.insert(id.clone()) =>
                            {
                                actions.push(Some(id.clone()));
                            }
                            _ => valid = false,
                        }
                    }
                    if valid && actions.len() == 10 && save_home_actions(&actions) {
                        self.home_actions = actions;
                        self.push_react_state();
                    }
                }
                "shelfMove" => {
                    let from = value["from"].as_u64().map(|index| index as usize);
                    let to = value["to"].as_u64().map(|index| index as usize);
                    if let (Some(from), Some(to)) = (from, to)
                        && from < self.shelf_items.len()
                        && to < self.shelf_items.len()
                        && from != to
                    {
                        self.shelf_icon_data.resize(self.shelf_items.len(), None);
                        let path = self.shelf_items.remove(from);
                        self.shelf_items.insert(to, path);
                        let icon = if from < self.shelf_icon_data.len() {
                            self.shelf_icon_data.remove(from)
                        } else {
                            None
                        };
                        self.shelf_icon_data.insert(to, icon);
                        self.shelf_generation = self.shelf_generation.wrapping_add(1);
                        self.shelf_revision = self.shelf_revision.wrapping_add(1);
                        save_shelf_paths(&self.shelf_items);
                        self.refresh_missing_shelf_icons();
                        self.push_react_state();
                    }
                }
                "shelfRemove" => {
                    if let Some(index) = value["index"].as_u64().map(|index| index as usize)
                        && index < self.shelf_items.len()
                    {
                        self.shelf_icon_data.resize(self.shelf_items.len(), None);
                        self.shelf_items.remove(index);
                        self.shelf_icon_data.remove(index);
                        self.shelf_generation = self.shelf_generation.wrapping_add(1);
                        self.shelf_revision = self.shelf_revision.wrapping_add(1);
                        save_shelf_paths(&self.shelf_items);
                        self.refresh_missing_shelf_icons();
                        self.push_react_state();
                    }
                }
                "screenToolFile" => {
                    let raw_path = value["path"].as_str().unwrap_or("");
                    let action = value["action"].as_str().unwrap_or("");
                    if raw_path.len() <= 2048 && !raw_path.chars().any(char::is_control) {
                        let user_profile =
                            std::env::var_os("USERPROFILE").map(std::path::PathBuf::from);
                        if let Some(downloads) = user_profile.map(|path| path.join("Downloads"))
                            && let (Ok(downloads), Ok(path)) = (
                                downloads.canonicalize(),
                                std::path::PathBuf::from(raw_path).canonicalize(),
                            )
                            && path.parent() == Some(downloads.as_path())
                            && path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| {
                                    name.starts_with("Island-capture-")
                                        || name.starts_with("Island-recording-")
                                })
                        {
                            if action == "open" {
                                let target = wide(&path.to_string_lossy());
                                let operation = wide("open");
                                unsafe {
                                    ShellExecuteW(
                                        self.window,
                                        operation.as_ptr(),
                                        target.as_ptr(),
                                        null(),
                                        null(),
                                        SW_SHOW,
                                    );
                                }
                            } else if action == "reveal" {
                                let target = wide("explorer.exe");
                                let parameters =
                                    wide(&format!("/select,\"{}\"", path.to_string_lossy()));
                                let operation = wide("open");
                                unsafe {
                                    ShellExecuteW(
                                        self.window,
                                        operation.as_ptr(),
                                        target.as_ptr(),
                                        parameters.as_ptr(),
                                        null(),
                                        SW_SHOW,
                                    );
                                }
                            } else if action == "copyImage"
                                && path
                                    .extension()
                                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                            {
                                let copied = std::fs::read(&path)
                                    .ok()
                                    .is_some_and(|bytes| copy_png_image(&bytes));
                                self.set_compact_screen_feedback(
                                    "notice",
                                    if copied {
                                        "Image copied"
                                    } else {
                                        "Could not copy image"
                                    },
                                    None,
                                    None,
                                    Some(std::time::Duration::from_millis(2300)),
                                );
                            } else if action == "copyFile" {
                                let copied = copy_file_to_clipboard(&path);
                                self.set_compact_screen_feedback(
                                    "notice",
                                    if copied {
                                        "Recording copied"
                                    } else {
                                        "Could not copy recording"
                                    },
                                    None,
                                    None,
                                    Some(std::time::Duration::from_millis(2300)),
                                );
                            }
                        }
                    }
                }
                "signatureDrag" => {
                    use base64::Engine as _;

                    let result = value["png"]
                        .as_str()
                        .filter(|encoded| encoded.len() <= 5_600_000)
                        .ok_or_else(|| "Signature PNG is missing or too large.".to_owned())
                        .and_then(|encoded| {
                            base64::engine::general_purpose::STANDARD
                                .decode(encoded)
                                .map_err(|_| "Signature PNG data is invalid.".to_owned())
                        })
                        .and_then(|png| start_signature_file_drag(self.window, &png));
                    let message = match result {
                        Ok(true) => "Signature added".to_owned(),
                        Ok(false) => "Drag canceled".to_owned(),
                        Err(_) => "Could not start file drag".to_owned(),
                    };
                    if let Some(view) = self.react_view.as_ref()
                        && let Ok(encoded) =
                            serde_json::to_string(&serde_json::json!({ "message": message }))
                    {
                        let _ = view.evaluate_script(&format!(
                            "window.islandSignatureDragResult?.({encoded});"
                        ));
                    }
                    redraw = false;
                }
                "shelfClear" => {
                    self.shelf_items.clear();
                    self.shelf_icon_data.clear();
                    self.shelf_generation = self.shelf_generation.wrapping_add(1);
                    self.shelf_revision = self.shelf_revision.wrapping_add(1);
                    save_shelf_paths(&self.shelf_items);
                    self.push_react_state();
                }
                "textSet" => {
                    if let Some(text) = value["value"].as_str()
                        && text.len() <= 32 * 1024
                    {
                        self.text_draft = text.to_owned();
                    }
                    redraw = false;
                }
                "textReadClipboard" => {
                    if let Some(text) = read_clipboard_text() {
                        self.text_draft = text;
                    }
                }
                "textCopy" => {
                    let _ = write_clipboard_text(&self.text_draft);
                }
                "textUpper" => self.text_draft = self.text_draft.to_uppercase(),
                "textLower" => self.text_draft = self.text_draft.to_lowercase(),
                "textTrim" => self.text_draft = self.text_draft.trim().to_owned(),
                "textClear" => self.text_draft.clear(),
                "noteSet" => {
                    if let Some(text) = value["value"].as_str()
                        && text.len() <= 256 * 1024
                    {
                        self.note = text.to_owned();
                        if let Some(folder) = self.notebook.folders.get_mut(self.notebook.selected)
                        {
                            folder.text = self.note.clone();
                        }
                        self.note_dirty = true;
                        unsafe { SetTimer(self.window, NOTE_SAVE_TIMER_ID, 400, None) };
                    }
                    redraw = false;
                }
                "noteCopy" => {
                    let _ = write_clipboard_text(&self.note);
                }
                "noteClear" => {
                    self.note.clear();
                    if let Some(folder) = self.notebook.folders.get_mut(self.notebook.selected) {
                        folder.text.clear();
                    }
                    self.note_dirty = true;
                    self.save_note();
                }
                "noteFolderAdd" => {
                    if self.notebook.folders.len() < 16 {
                        let name = value["name"].as_str().unwrap_or("").trim();
                        if !name.is_empty() && name.len() <= 48 {
                            self.notebook.folders.push(NoteFolder {
                                name: name.to_owned(),
                                text: String::new(),
                            });
                            self.notebook.selected = self.notebook.folders.len() - 1;
                            self.note.clear();
                            self.note_dirty = true;
                            self.save_note();
                        }
                    }
                }
                "noteFolderSelect" => {
                    if let Some(index) = value["index"].as_u64().map(|value| value as usize)
                        && index < self.notebook.folders.len()
                    {
                        self.save_note();
                        self.notebook.selected = index;
                        self.note = self.notebook.selected_text();
                        self.note_dirty = true;
                        self.save_note();
                    }
                }
                "noteFolderRename" => {
                    if let Some(index) = value["index"].as_u64().map(|value| value as usize)
                        && let Some(folder) = self.notebook.folders.get_mut(index)
                        && let Some(name) = value["name"].as_str().map(str::trim)
                        && !name.is_empty()
                        && name.len() <= 48
                    {
                        folder.name = name.to_owned();
                        self.note_dirty = true;
                        self.save_note();
                    }
                }
                "noteFolderDelete" => {
                    if let Some(index) = value["index"].as_u64().map(|value| value as usize)
                        && index < self.notebook.folders.len()
                        && self.notebook.folders.len() > 1
                    {
                        self.save_note();
                        self.notebook.folders.remove(index);
                        self.notebook.selected = if self.notebook.selected > index {
                            self.notebook.selected - 1
                        } else {
                            self.notebook.selected.min(self.notebook.folders.len() - 1)
                        };
                        self.note = self.notebook.selected_text();
                        self.note_dirty = true;
                        self.save_note();
                    }
                }
                "noteReminderSet" => {
                    let due = value["at"].as_u64();
                    let now = unix_seconds();
                    if due.is_some_and(|at| at > now && at < now.saturating_add(365 * 86400)) {
                        self.notebook.reminder_at = due;
                        self.reminder_notified_at = None;
                        self.note_dirty = true;
                        self.save_note();
                        self.clock_refresh_at = std::time::Instant::now();
                    }
                }
                "noteReminderClear" => {
                    self.notebook.reminder_at = None;
                    self.reminder_notified_at = None;
                    self.compact_feedbacks
                        .retain(|feedback| feedback.kind != "reminder");
                    if self.compact_feedback_visible_id.is_some_and(|id| {
                        !self
                            .compact_feedbacks
                            .iter()
                            .any(|feedback| feedback.id == id)
                    }) {
                        self.compact_feedback_visible_id = None;
                    }
                    self.sync_compact_feedback_clocks();
                    self.note_dirty = true;
                    self.save_note();
                    self.clock_refresh_at = std::time::Instant::now();
                }
                "commandEvaluate" => {
                    let query = value["query"].as_str().unwrap_or("");
                    if query.trim_start().starts_with('>') {
                        self.command_result = None;
                        self.command_error = Some("Island will not execute shell text. Open a terminal if you want to run this command.".to_owned());
                    } else {
                        match crate::commands::evaluate(query) {
                            Ok(result) => {
                                self.command_result = Some(result);
                                self.command_error = None;
                            }
                            Err(error) => {
                                self.command_result = None;
                                self.command_error = Some(error.user_message().to_owned());
                            }
                        }
                    }
                }
                "commandOpen" => self.start_command_app_scan(),
                "commandAppSearch" => {
                    let query = value["query"].as_str().unwrap_or("");
                    if query.len() <= 128 {
                        self.update_command_app_results(query);
                    }
                }
                "commandFileSearch" => {
                    let query = value["query"].as_str().unwrap_or("");
                    if query.len() <= 128 {
                        self.start_command_file_search(query);
                    }
                }
                "commandFileOpen" => {
                    let path = value["path"].as_str().unwrap_or("");
                    if let Some(file) = self
                        .command_file_results
                        .iter()
                        .find(|file| file.path == path)
                    {
                        if file.launch().is_ok() {
                            self.command_result = Some(format!("Opening {}…", file.name));
                            self.command_error = None;
                        } else {
                            self.command_result = None;
                            self.command_error = Some(
                                "Windows could not open that file. It may have moved.".to_owned(),
                            );
                        }
                    }
                }
                "commandAppLaunch" => {
                    let name = value["name"]
                        .as_str()
                        .filter(|name| name.len() <= 128)
                        .unwrap_or("");
                    let result = self
                        .command_apps
                        .iter()
                        .find(|app| app.name.eq_ignore_ascii_case(name))
                        .map(crate::commands::AppEntry::launch);
                    match result {
                        Some(Ok(())) => {
                            self.command_result = Some(format!("Opening {name}…"));
                            self.command_error = None;
                        }
                        Some(Err(error)) => {
                            self.command_result = None;
                            self.command_error = Some(error.user_message().to_owned());
                        }
                        None => {
                            self.command_result = None;
                            self.command_error =
                                Some("That Start Menu shortcut is no longer available.".to_owned());
                        }
                    }
                }
                "commandClear" => {
                    self.command_result = None;
                    self.command_error = None;
                }
                "clipboardWriteText" => {
                    if let Some(text) = value["value"].as_str().filter(|text| text.len() <= 64_000)
                    {
                        let _ = write_clipboard_text(text);
                        if self
                            .screen_clipboard_values
                            .iter()
                            .any(|screen_value| screen_value.eq_ignore_ascii_case(text))
                            || text.starts_with("rgb(")
                        {
                            self.screen_clipboard_values.insert(text.to_owned());
                        }
                        self.add_clipboard_item(text);
                    }
                }
                "screenToolStart" => {
                    if let Some(mode) = value["mode"]
                        .as_str()
                        .and_then(crate::platform::windows::screen_overlay::Mode::parse)
                    {
                        let quality = value["quality"]
                            .as_str()
                            .map(crate::platform::windows::screen_overlay::RecordQuality::parse)
                            .unwrap_or_default();
                        self.start_screen_overlay(
                            mode,
                            value["scope"].as_str() == Some("full"),
                            quality,
                        );
                    }
                    redraw = false;
                }
                "screenToolStop" => {
                    if let Some(stop) = &self.screen_recording_stop {
                        let _ = stop.send(());
                        self.send_screen_tool_result(
                            serde_json::json!({ "mode": "record", "recording": "stopping" }),
                        );
                    }
                    redraw = false;
                }
                "screenOcr" => {
                    let request_id = value["requestId"].as_u64().unwrap_or(0);
                    let image = value["image"].as_str().unwrap_or("");
                    let Some(encoded) = image.strip_prefix("data:image/jpeg;base64,") else {
                        self.send_screen_ocr_result(
                            request_id,
                            Err("Invalid image data. Capture the area again.".into()),
                        );
                        continue;
                    };
                    if request_id == 0 || encoded.len() > 7 * 1024 * 1024 {
                        self.send_screen_ocr_result(
                            request_id,
                            Err("The captured image is too large. Select a smaller area.".into()),
                        );
                        continue;
                    }
                    if self.screen_ocr_pending {
                        self.send_screen_ocr_result(
                            request_id,
                            Err("OCR is already processing an image.".into()),
                        );
                        continue;
                    }
                    use base64::Engine as _;
                    let bytes = match base64::engine::general_purpose::STANDARD.decode(encoded) {
                        Ok(bytes) if bytes.len() <= 6 * 1024 * 1024 => bytes,
                        _ => {
                            self.send_screen_ocr_result(
                                request_id,
                                Err("The captured image is invalid or too large.".into()),
                            );
                            continue;
                        }
                    };
                    self.screen_ocr_pending = true;
                    let sender = self.screen_ocr_sender.clone();
                    let window = self.window as usize;
                    thread::spawn(move || {
                        let result = crate::platform::windows::screen_ocr::recognize_image(&bytes);
                        let _ = sender.try_send((request_id, result));
                        unsafe { PostMessageW(window as HWND, WM_APP_REACT, 0, 0) };
                    });
                }
                "speechSettingsSave" => {
                    let base_url = value["baseUrl"].as_str().unwrap_or("");
                    let model = value["model"].as_str().unwrap_or("");
                    let language = value["language"].as_str().unwrap_or("");
                    let api_key = value["apiKey"].as_str().unwrap_or("");
                    match self
                        .speech_settings
                        .save(base_url, model, language, api_key)
                    {
                        Ok(settings) => {
                            self.speech_settings = settings;
                            self.send_speech_settings_result(Ok(self
                                .speech_settings
                                .public_state()));
                            self.push_react_state();
                        }
                        Err(error) => self.send_speech_settings_result(Err(error)),
                    }
                }
                "speechSettingsRemoveKey" => match self.speech_settings.remove_key() {
                    Ok(settings) => {
                        self.speech_settings = settings;
                        self.send_speech_settings_result(Ok(self.speech_settings.public_state()));
                        self.push_react_state();
                    }
                    Err(error) => self.send_speech_settings_result(Err(error)),
                },
                "speechTranscribe" => {
                    let request_id = value["requestId"].as_u64().unwrap_or(0);
                    let audio = value["audio"].as_str().unwrap_or("");
                    let (content_type, encoded) =
                        if let Some(encoded) = audio.strip_prefix("data:audio/webm;base64,") {
                            ("audio/webm", encoded)
                        } else if let Some(encoded) = audio.strip_prefix("data:audio/mp4;base64,") {
                            ("audio/mp4", encoded)
                        } else {
                            self.send_speech_result(
                                request_id,
                                Err("Island received an unsupported audio recording.".into()),
                            );
                            continue;
                        };
                    if request_id == 0 || encoded.len() > 7 * 1024 * 1024 {
                        self.send_speech_result(
                            request_id,
                            Err("The recording is too large. Record a shorter clip.".into()),
                        );
                        continue;
                    }
                    if self.speech_pending {
                        self.send_speech_result(
                            request_id,
                            Err("A transcription is already in progress.".into()),
                        );
                        continue;
                    }
                    use base64::Engine as _;
                    let bytes = match base64::engine::general_purpose::STANDARD.decode(encoded) {
                        Ok(bytes) if bytes.len() <= 5 * 1024 * 1024 => bytes,
                        _ => {
                            self.send_speech_result(
                                request_id,
                                Err("The recording is invalid or too large.".into()),
                            );
                            continue;
                        }
                    };
                    self.speech_pending = true;
                    let settings = self.speech_settings.clone();
                    let sender = self.speech_sender.clone();
                    let window = self.window as usize;
                    thread::spawn(move || {
                        let result = settings.transcribe(bytes, content_type);
                        let _ = sender.try_send((request_id, result));
                        unsafe { PostMessageW(window as HWND, WM_APP_REACT, 0, 0) };
                    });
                }
                "dictationPaste" => {
                    if let Some(text) = value["text"].as_str().filter(|text| text.len() <= 64_000) {
                        let _ = paste_clipboard_text(text);
                    }
                }
                "mediaToggle" => {
                    let _ = self.media_actions.send(MediaAction::Toggle);
                }
                "mediaPrevious" => {
                    let _ = self.media_actions.send(MediaAction::Previous);
                }
                "mediaNext" => {
                    let _ = self.media_actions.send(MediaAction::Next);
                }
                "mediaSeek" => {
                    if let Some(position_ms) = value["positionMs"].as_u64() {
                        let _ = self.media_actions.send(MediaAction::SeekToMs(position_ms));
                    }
                }
                "mediaSelect" => {
                    if let Some(id) = value["id"].as_str().filter(|id| !id.is_empty()) {
                        let _ = self.media_actions.send(MediaAction::Select(id.to_owned()));
                    }
                }
                "outputVolume" => {
                    if let Some(level) = value["value"].as_u64().filter(|level| *level <= 100) {
                        let _ = self
                            .output_actions
                            .try_send(AudioAction::SetVolume(level as u8));
                        self.output_volume = Some(level as u8);
                    }
                }
                "outputDevice" => {
                    let id = value["id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned);
                    let _ = self.output_actions.try_send(AudioAction::SelectDevice(id));
                }
                "soundSettings" => {
                    let target = wide("ms-settings:sound");
                    let operation = wide("open");
                    unsafe {
                        ShellExecuteW(
                            self.window,
                            operation.as_ptr(),
                            target.as_ptr(),
                            null(),
                            null(),
                            SW_SHOW,
                        )
                    };
                }
                "weatherCity" => {
                    if let Some(city) = value["value"].as_str()
                        && save_weather_city(city)
                    {
                        if !self.weather_city.eq_ignore_ascii_case(city.trim()) {
                            self.weather_city = city.trim().to_owned();
                            self.weather_lines.clear();
                            self.weather_data = None;
                            self.weather_error = None;
                            self.weather_updated_at = None;
                            self.weather_last_attempt = None;
                        }
                        self.refresh_weather_if_stale();
                    }
                }
                "weatherRefresh" => self.refresh_weather_if_stale(),
                "radioToggle" => match value["radio"].as_str() {
                    Some("wifi") => self.refresh_radios(Some(RadioToggle::Wifi)),
                    Some("bluetooth") => self.refresh_radios(Some(RadioToggle::Bluetooth)),
                    _ => {}
                },
                "radioSettings" => {
                    let page = match value["radio"].as_str() {
                        Some("wifi") => "ms-settings:network-wifi",
                        _ => "ms-settings:bluetooth",
                    };
                    let target = wide(page);
                    let operation = wide("open");
                    unsafe {
                        ShellExecuteW(
                            self.window,
                            operation.as_ptr(),
                            target.as_ptr(),
                            null(),
                            null(),
                            SW_SHOW,
                        )
                    };
                }
                "powerAction" => {
                    if let Some(action) = value["action"].as_str() {
                        let result = run_power_action(action);
                        // A successful workstation lock immediately occludes
                        // WebView2. Avoid replacing the compact UI with a
                        // transient success message during that handoff.
                        self.power_notice = if action == "lock" && result == "PC locked." {
                            None
                        } else {
                            Some(result)
                        };
                    }
                }
                "powerCancel" => {
                    self.power_notice = Some(run_shutdown_command("/a"));
                }
                "themeToggle" => {
                    let next_dark = !self.dark_theme;
                    let _ = self.set_dark_theme(next_dark);
                    self.callbacks
                        .dark_theme
                        .store(next_dark, Ordering::Release);
                    thread::spawn(move || {
                        if let Ok(mut store) = crate::storage::SettingsStore::open_default() {
                            let theme = if next_dark {
                                crate::storage::StoredTheme::Dark
                            } else {
                                crate::storage::StoredTheme::Light
                            };
                            let _ = store.set_theme(theme);
                        }
                    });
                    self.callbacks.context.set_theme(if next_dark {
                        egui::ThemePreference::Dark
                    } else {
                        egui::ThemePreference::Light
                    });
                    self.callbacks
                        .theme_toggle_requested
                        .store(true, Ordering::Release);
                    self.callbacks.context.request_repaint();
                    self.push_react_state();
                }
                _ => continue,
            }
            if redraw {
                let _ = self.render_frame();
            }
        }
        self.drain_screen_ocr_results();
        self.drain_screen_qr_results();
        self.drain_speech_results();
    }

    fn send_screen_ocr_result(&self, request_id: u64, result: Result<String, String>) {
        let payload = match result {
            Ok(text) => serde_json::json!({ "requestId": request_id, "mode": "ocr", "text": text }),
            Err(error) => {
                serde_json::json!({ "requestId": request_id, "mode": "ocr", "error": error })
            }
        };
        if let Some(view) = &self.react_view {
            let _ = view.evaluate_script(&format!("window.islandScreenToolResult?.({payload}); window.islandScreenOcrResult?.({payload});"));
        }
    }

    fn start_screen_overlay(
        &mut self,
        mode: crate::platform::windows::screen_overlay::Mode,
        full_screen: bool,
        quality: crate::platform::windows::screen_overlay::RecordQuality,
    ) {
        if self.screen_overlay_active || self.pending_screen_overlay.is_some() {
            return;
        }
        if !self.is_open() {
            self.remember_foreground();
        }
        let now = std::time::Instant::now();
        if self.reducer.snapshot().mode == ShellMode::Tucked {
            self.reducer.dispatch(ShellEvent::Untuck, now);
        }
        self.reducer.dispatch(ShellEvent::Close, now);
        self.callbacks.is_open.store(false, Ordering::Release);
        self.callbacks.is_pinned.store(false, Ordering::Release);
        self.callbacks
            .pointer_inside
            .store(false, Ordering::Release);
        self.callbacks
            .pointer_poll_needed
            .store(false, Ordering::Release);
        self.hover_intent_deadline = None;
        self.pending_screen_overlay = Some((mode, full_screen, quality));
        self.request_tick();
    }

    fn launch_screen_overlay(
        &mut self,
        mode: crate::platform::windows::screen_overlay::Mode,
        full_screen: bool,
        quality: crate::platform::windows::screen_overlay::RecordQuality,
    ) {
        let visible = |window: HWND| {
            !window.is_null()
                && unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(window) }
                    != 0
        };
        let mut hidden_windows = Vec::new();
        if !self.drop_catcher.is_null() {
            hidden_windows.push((self.drop_catcher as isize, visible(self.drop_catcher)));
        }
        let desk = self.callbacks.desk_window.load(Ordering::Acquire);
        if !desk.is_null() {
            hidden_windows.push((desk as isize, visible(desk)));
        }
        self.screen_overlay_active = true;
        let (kind, label) = match mode {
            crate::platform::windows::screen_overlay::Mode::Capture => ("capture", "Select area"),
            crate::platform::windows::screen_overlay::Mode::Ocr => ("ocr", "Select text"),
            crate::platform::windows::screen_overlay::Mode::Qr => ("qr", "Select QR code"),
            crate::platform::windows::screen_overlay::Mode::Color => ("color", "Pick color"),
            crate::platform::windows::screen_overlay::Mode::Record => {
                ("record", "Select recording area")
            }
        };
        let already_processing = match mode {
            crate::platform::windows::screen_overlay::Mode::Ocr => self.screen_ocr_pending,
            crate::platform::windows::screen_overlay::Mode::Qr => self.screen_qr_pending,
            _ => false,
        };
        self.screen_overlay_feedback_kind = if already_processing { None } else { Some(kind) };
        if !already_processing {
            self.set_compact_screen_activity(kind, label);
        }
        crate::platform::windows::screen_overlay::launch(
            hidden_windows,
            self.window as isize,
            self.screen_overlay_sender.clone(),
            mode,
            full_screen,
            quality,
        );
        self.request_tick();
    }

    fn selector_busy(&self) -> bool {
        self.screen_overlay_active || self.pending_screen_overlay.is_some()
    }

    fn set_compact_screen_feedback(
        &mut self,
        kind: &'static str,
        label: impl Into<String>,
        copy_value: Option<String>,
        color: Option<(u8, u8, u8)>,
        duration: Option<std::time::Duration>,
    ) -> u64 {
        if kind != "notice" {
            self.clear_compact_screen_activity(kind);
        }
        let duration = if kind == "reminder" {
            Some(std::time::Duration::from_secs(15))
        } else {
            duration
        };
        self.enqueue_compact_feedback(kind, label.into(), copy_value, color, "result", duration)
    }

    fn set_compact_screen_activity(&mut self, kind: &'static str, label: impl Into<String>) {
        self.clear_compact_screen_activity(kind);
        self.enqueue_compact_feedback(kind, label.into(), None, None, "active", None);
    }

    fn clear_compact_screen_activity(&mut self, kind: &'static str) {
        let removed = self
            .compact_feedbacks
            .iter()
            .filter(|feedback| feedback.phase == "active" && feedback.kind == kind)
            .map(|feedback| feedback.id)
            .collect::<Vec<_>>();
        self.compact_feedbacks
            .retain(|feedback| !(feedback.phase == "active" && feedback.kind == kind));
        if self
            .compact_feedback_visible_id
            .is_some_and(|id| removed.contains(&id))
        {
            self.compact_feedback_visible_id = None;
        }
        if !removed.is_empty() {
            self.sync_compact_feedback_clocks();
        }
    }

    fn enqueue_compact_feedback(
        &mut self,
        kind: &'static str,
        label: String,
        copy_value: Option<String>,
        color: Option<(u8, u8, u8)>,
        phase: &'static str,
        display_duration: Option<std::time::Duration>,
    ) -> u64 {
        let now = std::time::Instant::now();
        self.status_label = label.clone();
        if phase == "result"
            && kind != "reminder"
            && let Some(feedback) = self.compact_feedbacks.iter_mut().find(|feedback| {
                feedback.phase == "result"
                    && feedback.kind == kind
                    && feedback.label == label
                    && feedback.copy_value == copy_value
                    && feedback.color == color
            })
        {
            feedback.repeat_count = feedback.repeat_count.saturating_add(1);
            feedback.display_duration = display_duration;
            if feedback.expires_at.is_some() {
                feedback.expires_at = display_duration.map(|duration| now + duration);
                feedback.display_remaining = None;
            } else {
                feedback.display_remaining = display_duration;
            }
            if feedback.stale_at.is_some() {
                feedback.stale_at = Some(now + COMPACT_FEEDBACK_MAX_WAIT);
                feedback.stale_remaining = None;
            } else if feedback.stale_remaining.is_some() {
                feedback.stale_remaining = Some(COMPACT_FEEDBACK_MAX_WAIT);
            }
            let id = feedback.id;
            self.sync_compact_feedback_clocks();
            self.push_react_state();
            self.request_tick();
            return id;
        }
        let id = self.next_compact_feedback_id;
        self.next_compact_feedback_id = self.next_compact_feedback_id.wrapping_add(1).max(1);
        let stale_remaining = if phase == "active" || kind == "reminder" {
            None
        } else {
            Some(COMPACT_FEEDBACK_MAX_WAIT)
        };
        let clocks_paused =
            !self.compact_presentation_available() || self.compact_protected_work_active();
        self.compact_feedbacks.push(CompactScreenFeedback {
            id,
            label,
            kind,
            phase,
            copy_value,
            color,
            file_path: None,
            display_duration,
            display_remaining: display_duration,
            stale_at: if clocks_paused {
                None
            } else {
                stale_remaining.map(|remaining| now + remaining)
            },
            stale_remaining: if clocks_paused { stale_remaining } else { None },
            next_visible_at: None,
            next_visible_remaining: None,
            appeared_at: now,
            expires_at: None,
            repeat_count: 1,
        });
        loop {
            let transient_count = self
                .compact_feedbacks
                .iter()
                .filter(|feedback| feedback.phase != "active" && feedback.kind != "reminder")
                .count();
            if transient_count <= COMPACT_FEEDBACK_CAPACITY {
                break;
            }
            let Some(index) = self.compact_feedbacks.iter().position(|feedback| {
                feedback.phase != "active"
                    && feedback.kind != "reminder"
                    && Some(feedback.id) != self.compact_feedback_visible_id
            }) else {
                break;
            };
            self.compact_feedbacks.remove(index);
        }
        self.sync_compact_feedback_clocks();
        self.push_react_state();
        self.request_tick();
        id
    }

    fn set_compact_feedback_file_path(&mut self, id: u64, path: String) {
        if let Some(feedback) = self
            .compact_feedbacks
            .iter_mut()
            .find(|feedback| feedback.id == id)
        {
            feedback.file_path = Some(path);
        }
    }

    fn compact_protected_work_active(&self) -> bool {
        self.screen_recording_stop.is_some()
            || self
                .compact_feedbacks
                .iter()
                .any(|feedback| feedback.phase == "active")
    }

    fn compact_presentation_available(&self) -> bool {
        self.compact_status_presentation_visible
            && self.react_compact_visible
            && !self.react_compact_faded
            && !self.fullscreen_hidden
    }

    /// Advances queue, snooze, and visible dwell timers only when compact content can be seen.
    /// Results retain a bounded wait after they are displaced, rather than expiring invisibly.
    fn sync_compact_feedback_clocks(&mut self) {
        let now = std::time::Instant::now();
        let clocks_paused =
            !self.compact_presentation_available() || self.compact_protected_work_active();
        let selected_id = self.compact_feedback_visible_id;
        let status = self.compact_status_visible.as_str();

        for feedback in &mut self.compact_feedbacks {
            if clocks_paused {
                if let Some(deadline) = feedback.stale_at.take() {
                    feedback.stale_remaining = Some(deadline.saturating_duration_since(now));
                }
                if let Some(deadline) = feedback.next_visible_at.take() {
                    feedback.next_visible_remaining = Some(deadline.saturating_duration_since(now));
                }
            } else {
                if feedback.stale_at.is_none()
                    && let Some(remaining) = feedback.stale_remaining.take()
                {
                    feedback.stale_at = Some(now + remaining);
                }
                if feedback.next_visible_at.is_none()
                    && let Some(remaining) = feedback.next_visible_remaining.take()
                {
                    feedback.next_visible_at = Some(now + remaining);
                }
            }

            if feedback.phase == "active" {
                continue;
            }
            let selected_and_visible = !clocks_paused
                && selected_id == Some(feedback.id)
                && Self::feedback_matches_compact_status(feedback, &status)
                && feedback.next_visible_at.is_none();
            if selected_and_visible {
                if feedback.expires_at.is_none()
                    && let Some(remaining) = feedback.display_remaining.take()
                {
                    feedback.expires_at = Some(now + remaining);
                    feedback.appeared_at = now;
                    feedback.stale_at = None;
                    feedback.stale_remaining = None;
                }
            } else if let Some(deadline) = feedback.expires_at.take() {
                let remaining = deadline.saturating_duration_since(now);
                if remaining.is_zero() {
                    feedback.expires_at = Some(deadline);
                } else {
                    feedback.display_remaining = Some(remaining);
                    if feedback.kind != "reminder"
                        && feedback.stale_at.is_none()
                        && feedback.stale_remaining.is_none()
                    {
                        if clocks_paused {
                            feedback.stale_remaining = Some(COMPACT_FEEDBACK_MAX_WAIT);
                        } else {
                            feedback.stale_at = Some(now + COMPACT_FEEDBACK_MAX_WAIT);
                        }
                    }
                }
            }
        }
    }

    fn add_screen_clipboard_value(&mut self, text: &str) {
        if text.is_empty() || text.len() > 64_000 {
            return;
        }
        self.screen_clipboard_values.insert(text.to_owned());
        self.add_clipboard_item(text);
    }

    fn add_clipboard_item(&mut self, text: &str) {
        if text.is_empty() || text.len() > 64_000 {
            return;
        }
        if self
            .clipboard_items
            .first()
            .is_none_or(|first| first != text)
        {
            self.clipboard_items.retain(|item| item != text);
            self.clipboard_items.insert(0, text.to_owned());
            self.clipboard_items.truncate(self.clipboard_limit);
        }
        self.push_react_state();
    }

    fn finish_screen_overlay(&mut self) {
        self.screen_overlay_active = false;
        while let Ok(event) = self.screen_overlay_receiver.try_recv() {
            use crate::platform::windows::screen_overlay::{Mode, ResultEvent};
            match event {
                ResultEvent::Selected {
                    mode: Mode::Capture,
                    png,
                    width,
                    height,
                    preview_png,
                } => match crate::platform::windows::screen_overlay::save_png(&png) {
                    Ok(path) => {
                        use base64::Engine as _;
                        let preview = preview_png.map(|bytes| {
                            format!(
                                "data:image/png;base64,{}",
                                base64::engine::general_purpose::STANDARD.encode(bytes)
                            )
                        });
                        let size_bytes = std::fs::metadata(&path)
                            .map(|metadata| metadata.len())
                            .unwrap_or(0);
                        self.send_screen_tool_result(serde_json::json!({ "mode": "capture", "path": path, "width": width, "height": height, "sizeBytes": size_bytes, "preview": preview }));
                        let feedback_id = self.set_compact_screen_feedback(
                            "capture",
                            "Screenshot saved",
                            None,
                            None,
                            Some(std::time::Duration::from_millis(3500)),
                        );
                        self.screen_overlay_feedback_kind = None;
                        self.set_compact_feedback_file_path(feedback_id, path.clone());
                        self.push_react_state();
                    }
                    Err(error) => {
                        self.clear_compact_screen_activity("capture");
                        self.screen_overlay_feedback_kind = None;
                        self.set_compact_screen_feedback(
                            "notice",
                            "Capture unavailable",
                            None,
                            None,
                            Some(std::time::Duration::from_millis(2400)),
                        );
                        self.send_screen_tool_result(
                            serde_json::json!({ "mode": "capture", "error": error }),
                        );
                    }
                },
                ResultEvent::Selected {
                    mode: Mode::Ocr,
                    png,
                    ..
                } => {
                    if self.screen_ocr_pending {
                        self.screen_overlay_feedback_kind = None;
                        self.send_screen_tool_result(serde_json::json!({ "mode": "ocr", "error": "OCR is already processing another selection." }));
                        continue;
                    }
                    self.set_compact_screen_activity("ocr", "Reading screen text…");
                    self.screen_overlay_feedback_kind = None;
                    self.screen_ocr_pending = true;
                    let request_id = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let sender = self.screen_ocr_sender.clone();
                    let window = self.window as usize;
                    thread::spawn(move || {
                        let result = crate::platform::windows::screen_ocr::recognize_image(&png);
                        let _ = sender.try_send((request_id, result));
                        unsafe { PostMessageW(window as HWND, WM_APP_REACT, 0, 0) };
                    });
                    self.send_screen_tool_result(
                        serde_json::json!({ "mode": "ocr", "working": true }),
                    );
                }
                ResultEvent::Selected {
                    mode: Mode::Qr,
                    png,
                    ..
                } => {
                    if self.screen_qr_pending {
                        self.screen_overlay_feedback_kind = None;
                        self.set_compact_screen_feedback(
                            "notice",
                            "QR scan already running",
                            None,
                            None,
                            Some(std::time::Duration::from_millis(2200)),
                        );
                        continue;
                    }
                    self.set_compact_screen_activity("qr", "Reading QR code…");
                    self.screen_overlay_feedback_kind = None;
                    self.screen_qr_pending = true;
                    let sender = self.screen_qr_sender.clone();
                    let window = self.window as usize;
                    thread::spawn(move || {
                        let result = crate::platform::windows::screen_qr::decode_image(&png);
                        let _ = sender.try_send(result);
                        unsafe { PostMessageW(window as HWND, WM_APP_REACT, 0, 0) };
                    });
                }
                ResultEvent::Selected {
                    mode: Mode::Color, ..
                } => {}
                ResultEvent::Selected {
                    mode: Mode::Record, ..
                } => {}
                ResultEvent::Color { hex, rgb } => {
                    let _ = write_clipboard_text(&hex);
                    self.add_screen_clipboard_value(&hex);
                    let color = parse_hex_color(&hex);
                    self.set_compact_screen_feedback(
                        "color",
                        hex.clone(),
                        Some(hex.clone()),
                        color,
                        Some(std::time::Duration::from_millis(2400)),
                    );
                    self.screen_overlay_feedback_kind = None;
                    self.send_screen_tool_result(
                        serde_json::json!({ "mode": "color", "hex": hex, "rgb": rgb }),
                    );
                }
                ResultEvent::RecordingStarted {
                    path,
                    stop,
                    start: _,
                } => {
                    self.screen_recording_stop = Some(stop);
                    self.clear_compact_screen_activity("record");
                    self.screen_overlay_feedback_kind = None;
                    self.update_status_label();
                    let _ = self.apply_display_affinity();
                    self.hover_intent_deadline = None;
                    self.reducer
                        .dispatch(ShellEvent::Close, std::time::Instant::now());
                    self.callbacks.is_open.store(false, Ordering::Release);
                    self.callbacks.is_pinned.store(false, Ordering::Release);
                    self.callbacks
                        .pointer_inside
                        .store(false, Ordering::Release);
                    self.send_screen_tool_result(
                        serde_json::json!({ "mode": "record", "recording": true, "path": path }),
                    );
                    self.push_react_state();
                }
                ResultEvent::Recorded {
                    path,
                    duration_ms,
                    size_bytes,
                } => {
                    self.screen_recording_stop = None;
                    let _ = self.apply_display_affinity();
                    self.send_screen_tool_result(serde_json::json!({ "mode": "record", "path": path, "durationMs": duration_ms, "sizeBytes": size_bytes }));
                    let feedback_id = self.set_compact_screen_feedback(
                        "record",
                        "Recording saved",
                        None,
                        None,
                        Some(std::time::Duration::from_millis(4000)),
                    );
                    self.set_compact_feedback_file_path(feedback_id, path.clone());
                    self.push_react_state();
                }
                ResultEvent::Error { mode, message } => {
                    if mode == Some(Mode::Record) {
                        self.screen_recording_stop = None;
                        let _ = self.apply_display_affinity();
                    }
                    if let Some(mode) = mode {
                        let label = match mode {
                            Mode::Ocr => "Text scan unavailable",
                            Mode::Qr => "QR scan unavailable",
                            Mode::Color => "Color picker unavailable",
                            Mode::Capture => "Capture unavailable",
                            Mode::Record => "Recording unavailable",
                        };
                        let kind = match mode {
                            Mode::Ocr => "ocr",
                            Mode::Qr => "qr",
                            Mode::Color => "color",
                            Mode::Capture => "capture",
                            Mode::Record => "record",
                        };
                        self.clear_compact_screen_activity(kind);
                        self.set_compact_screen_feedback(
                            "notice",
                            label,
                            None,
                            None,
                            Some(std::time::Duration::from_millis(2400)),
                        );
                    }
                    self.screen_overlay_feedback_kind = None;
                    self.send_screen_tool_result(serde_json::json!({ "mode": mode.map(|mode| match mode { Mode::Capture => "capture", Mode::Ocr => "ocr", Mode::Qr => "qr", Mode::Color => "color", Mode::Record => "record" }), "error": message }));
                }
                ResultEvent::Canceled => {
                    if let Some(kind) = self.screen_overlay_feedback_kind.take() {
                        self.clear_compact_screen_activity(kind);
                        self.update_status_label();
                        self.push_react_state();
                    }
                }
            }
        }
        // Let the next native tick retarget the compact spring before drawing
        // the result. Painting here would show the new label in the old,
        // narrower notch for one frame.
        self.sync_drop_catcher();
        self.request_tick();
    }

    fn send_screen_tool_result(&self, payload: serde_json::Value) {
        if let Some(view) = &self.react_view {
            if let Ok(encoded) = serde_json::to_string(&payload) {
                let _ =
                    view.evaluate_script(&format!("window.islandScreenToolResult?.({encoded});"));
            }
        }
    }

    fn drain_screen_ocr_results(&mut self) {
        while let Ok((request_id, result)) = self.screen_ocr_receiver.try_recv() {
            self.screen_ocr_pending = false;
            match &result {
                Ok(text) if !text.trim().is_empty() => {
                    let text = text.trim();
                    let _ = write_clipboard_text(text);
                    self.add_screen_clipboard_value(text);
                    let first_line = text
                        .lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or(text);
                    let mut chars = first_line.chars();
                    let snippet = chars.by_ref().take(26).collect::<String>();
                    let suffix = if chars.next().is_some() { "…" } else { "" };
                    self.set_compact_screen_feedback(
                        "ocr",
                        format!("{snippet}{suffix}"),
                        Some(text.to_owned()),
                        None,
                        Some(std::time::Duration::from_millis(2400)),
                    );
                }
                Ok(_) => {
                    self.clear_compact_screen_activity("ocr");
                    self.set_compact_screen_feedback(
                        "notice",
                        "No text found",
                        None,
                        None,
                        Some(std::time::Duration::from_millis(2200)),
                    );
                }
                Err(_) => {
                    self.clear_compact_screen_activity("ocr");
                    self.set_compact_screen_feedback(
                        "notice",
                        "Text scan failed",
                        None,
                        None,
                        Some(std::time::Duration::from_millis(2200)),
                    );
                }
            }
            self.send_screen_ocr_result(request_id, result);
        }
    }

    fn drain_screen_qr_results(&mut self) {
        while let Ok(result) = self.screen_qr_receiver.try_recv() {
            self.screen_qr_pending = false;
            match result {
                Ok(Some(text)) => {
                    let text = text.trim().to_owned();
                    let _ = write_clipboard_text(&text);
                    self.add_screen_clipboard_value(&text);
                    let snippet: String = text.chars().take(28).collect();
                    self.set_compact_screen_feedback(
                        "qr",
                        snippet,
                        Some(text),
                        None,
                        Some(std::time::Duration::from_millis(3200)),
                    );
                }
                Ok(None) => {
                    self.clear_compact_screen_activity("qr");
                    self.set_compact_screen_feedback(
                        "notice",
                        "No QR code in selection",
                        None,
                        None,
                        Some(std::time::Duration::from_millis(2800)),
                    );
                }
                Err(_) => {
                    self.clear_compact_screen_activity("qr");
                    self.set_compact_screen_feedback(
                        "notice",
                        "QR scan failed",
                        None,
                        None,
                        Some(std::time::Duration::from_millis(2800)),
                    );
                }
            }
        }
    }

    fn send_speech_settings_result(&self, result: Result<serde_json::Value, String>) {
        let payload = match result {
            Ok(settings) => serde_json::json!({ "settings": settings }),
            Err(error) => serde_json::json!({ "error": error }),
        };
        if let Some(view) = &self.react_view {
            let _ =
                view.evaluate_script(&format!("window.islandSpeechSettingsResult?.({payload});"));
        }
    }

    fn send_speech_result(&self, request_id: u64, result: Result<String, String>) {
        let payload = match result {
            Ok(text) => serde_json::json!({ "requestId": request_id, "text": text }),
            Err(error) => serde_json::json!({ "requestId": request_id, "error": error }),
        };
        if let Some(view) = &self.react_view {
            let _ = view.evaluate_script(&format!("window.islandSpeechResult?.({payload});"));
        }
    }

    fn drain_speech_results(&mut self) {
        while let Ok((request_id, result)) = self.speech_receiver.try_recv() {
            self.speech_pending = false;
            self.send_speech_result(request_id, result);
        }
    }

    fn drain_wake_word_events(&mut self) {
        use crate::platform::windows::wake_word::Event;
        let mut changed = false;
        while let Ok(event) = self.wake_word.try_recv() {
            match event {
                Event::Listening { phrase } => {
                    self.wake_word_status = "listening".into();
                    self.wake_word_phrase = phrase;
                    self.wake_word_error = None;
                }
                Event::Triggered { phrase } => {
                    if self
                        .observed_activity
                        .as_ref()
                        .and_then(|activity| activity.privacy.as_ref())
                        .is_some_and(|privacy| privacy.external_microphone_active)
                    {
                        self.wake_word.disarm();
                        self.wake_word_status = "paused".into();
                        self.wake_mic_clear_samples = 0;
                        changed = true;
                        continue;
                    }
                    self.wake_word_status = "triggered".into();
                    self.wake_word_phrase = phrase;
                    self.wake_word_error = None;
                    if self.modules.agents {
                        self.wake_surface = true;
                        self.wake_closing = false;
                        self.expanded_body_width = 480.0;
                        self.expanded_body_height = 90.0;
                        self.page = ShellPage::Agents;
                        self.open_from_click();
                    }
                }
                Event::Off if self.wake_word_status == "paused" => {}
                Event::Off => {
                    self.wake_word_status = "off".into();
                    self.wake_word_error = None;
                }
                Event::Error { phrase, message } => {
                    self.wake_word_status = "error".into();
                    self.wake_word_phrase = phrase;
                    self.wake_word_error = Some(message);
                }
            }
            self.wake_word_revision = self.wake_word_revision.wrapping_add(1);
            changed = true;
        }
        if changed {
            self.push_react_state();
        }
    }

    fn start_command_app_scan(&mut self) {
        if self.command_apps_loading
            || (self.command_apps_loaded
                && self
                    .command_apps_updated_at
                    .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)))
        {
            return;
        }
        let (sender, receiver) = sync_channel(1);
        self.command_apps_receiver = Some(receiver);
        self.command_apps_loading = true;
        let window = self.window as usize;
        thread::spawn(move || {
            let _ = sender.send(crate::commands::discover_apps());
            unsafe { PostMessageW(window as HWND, WM_APP_SHELL_TICK, 0, 0) };
        });
    }

    fn poll_command_app_scan(&mut self) -> bool {
        let Some(receiver) = self.command_apps_receiver.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok(apps) => {
                self.command_apps = apps;
                self.command_apps_loaded = true;
                self.command_apps_loading = false;
                self.command_apps_updated_at = Some(std::time::Instant::now());
                self.command_apps_receiver = None;
                let query = self.command_app_query.clone();
                self.update_command_app_results(&query);
                true
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Let the next expansion retry instead of treating a failed
                // background scan as a fresh cache for the full TTL.
                self.command_apps_loaded = false;
                self.command_apps_loading = false;
                self.command_apps_updated_at = None;
                self.command_apps_receiver = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
        }
    }

    fn update_command_app_results(&mut self, query: &str) {
        self.command_app_query = query.to_owned();
        let needle = query.trim().to_lowercase();
        let matches = if needle.is_empty() {
            Vec::new()
        } else {
            self.command_apps
                .iter()
                .filter(|app| app.name.to_lowercase().contains(&needle))
                .take(8)
                .map(|app| (app.name.clone(), app.icon_path().to_path_buf()))
                .collect::<Vec<_>>()
        };
        self.command_app_results = matches.iter().map(|(name, _)| name.clone()).collect();
        self.command_app_icons.clear();
        let icon_query = self.command_app_query.clone();
        let generation = self
            .command_app_icon_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let generation_counter = Arc::clone(&self.command_app_icon_generation);
        let icon_cache = Arc::clone(&self.command_icon_cache);
        let sender = self.command_app_icon_sender.clone();
        let window = self.window as usize;
        thread::spawn(move || {
            let mut icons = Vec::with_capacity(matches.len());
            for (name, path) in matches {
                if generation_counter.load(Ordering::Relaxed) != generation {
                    return;
                }
                icons.push((name, cached_command_icon_data_url(&icon_cache, &path)));
            }
            if generation_counter.load(Ordering::Relaxed) == generation {
                let _ = sender.send((icon_query, icons));
                unsafe { PostMessageW(window as HWND, WM_APP_SHELL_TICK, 0, 0) };
            }
        });
    }

    fn poll_command_app_icons(&mut self) -> bool {
        let mut changed = false;
        while let Ok((query, icons)) = self.command_app_icon_receiver.try_recv() {
            if query == self.command_app_query {
                self.command_app_icons = icons.into_iter().collect();
                changed = true;
            }
        }
        changed
    }

    fn start_command_file_search(&mut self, query: &str) {
        self.command_file_query = query.trim().to_owned();
        if self.command_file_query.chars().count() < 2 || self.command_file_receiver.is_some() {
            return;
        }
        let query = self.command_file_query.clone();
        let (sender, receiver) = sync_channel(1);
        self.command_file_receiver = Some(receiver);
        let icon_cache = Arc::clone(&self.command_icon_cache);
        let window = self.window as usize;
        thread::spawn(move || {
            let results = crate::commands::search_files(&query)
                .into_iter()
                .map(|file| {
                    let path = std::path::PathBuf::from(&file.path);
                    let kind = shelf_item_kind(&path);
                    let icon = cached_command_icon_data_url(&icon_cache, &path);
                    CommandFileResult {
                        name: file.name,
                        path: file.path,
                        kind,
                        icon,
                    }
                })
                .collect();
            let _ = sender.send((query, results));
            unsafe { PostMessageW(window as HWND, WM_APP_SHELL_TICK, 0, 0) };
        });
    }

    fn poll_command_file_search(&mut self) -> bool {
        let Some(receiver) = self.command_file_receiver.as_ref() else {
            return false;
        };
        match receiver.try_recv() {
            Ok((query, results)) => {
                self.command_file_receiver = None;
                if query == self.command_file_query {
                    self.command_file_results = results;
                } else if self.command_file_query.chars().count() >= 2 {
                    let latest = self.command_file_query.clone();
                    self.start_command_file_search(&latest);
                }
                true
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.command_file_receiver = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
        }
    }

    fn set_dark_theme(&mut self, dark_theme: bool) -> Result<(), String> {
        if self.dark_theme != dark_theme {
            self.dark_theme = dark_theme;
            if let Some(view) = &self.react_view {
                let _ = view.set_background_color((0, 0, 0, 0));
            }
            self.render_frame()?;
        }
        Ok(())
    }

    fn set_reduced_motion(&mut self, reduced_motion: bool) -> Result<(), String> {
        if self.reduced_motion == reduced_motion {
            return Ok(());
        }
        self.reduced_motion = reduced_motion;
        if reduced_motion {
            let target = self.visual_target();
            self.width_motion.snap_to(target.0);
            self.height_motion.snap_to(target.1);
            self.animation_target = target;
            self.render_frame()?;
        }
        self.request_tick();
        Ok(())
    }

    fn refresh_display(&mut self, dpi_override: Option<u32>) -> Result<(), String> {
        self.refresh_period = composition_refresh_period();
        self.frame_interval =
            animation_frame_interval(self.refresh_period, self.render_cost_estimate);
        let dpi = dpi_override.unwrap_or_else(|| unsafe { GetDpiForWindow(self.window) });
        let mut redraw = false;
        if dpi != 0 {
            redraw = (f64::from(dpi) / 96.0 - self.scale).abs() >= f64::EPSILON;
            self.apply_dpi(dpi)?;
        }
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        if screen_width > 0 && screen_width != self.screen_width {
            self.screen_width = screen_width;
            redraw = true;
        }
        if redraw {
            self.render_frame()?;
        }
        self.request_tick();
        Ok(())
    }

    fn apply_dpi(&mut self, dpi: u32) -> Result<(), String> {
        let Some((scale, max_width, max_height)) = shell_surface_metrics(dpi) else {
            return Err("Windows reported an unsupported display scale for the shell".into());
        };
        if (scale - self.scale).abs() < f64::EPSILON {
            return Ok(());
        }

        let pixel_count = (max_width as usize)
            .checked_mul(max_height as usize)
            .ok_or_else(|| "display scale exceeds shell memory limits".to_owned())?;
        let mut alpha = Vec::new();
        if alpha.try_reserve_exact(pixel_count).is_err() {
            return Err("could not allocate the scaled shell hit surface".into());
        }
        let (dc, bitmap, previous_bitmap, font, previous_font, pixels) =
            create_surface(max_width, max_height, scale)?;

        destroy_surface(
            self.dc,
            self.bitmap,
            self.previous_bitmap,
            self.font,
            self.previous_font,
        );
        self.dc = dc;
        self.bitmap = bitmap;
        self.previous_bitmap = previous_bitmap;
        self.font = font;
        self.previous_font = previous_font;
        self.pixels = pixels;
        self.alpha = alpha;
        self.max_width = max_width;
        self.max_height = max_height;
        self.scale = scale;
        Ok(())
    }

    fn set_shape(&mut self, shape: ShapeMode) {
        if self.shape != shape {
            self.shape = shape;
            self.set_expanded_body_width(self.wide_layout);
            self.request_tick();
        }
    }

    fn set_expanded_body_width(&mut self, wide: bool) {
        self.wide_layout = wide;
        if self.wake_surface {
            self.expanded_body_width = 480.0;
            return;
        }
        let base = self.shape.expanded_body_width();
        let screen_limit = (f64::from(self.screen_width) / self.scale - 32.0).max(base);
        self.expanded_body_width =
            (base + if wide { PROFILE_PANEL_EXTRA_WIDTH } else { 0.0 }).min(screen_limit);
    }

    fn set_focus_restore_target(&mut self, target: HWND) {
        if !self.is_open() && self.is_external_window(target) {
            self.previous_foreground = target;
        }
    }

    fn set_hide_in_fullscreen(&mut self, enabled: bool) {
        if self.hide_in_fullscreen == enabled {
            return;
        }
        self.hide_in_fullscreen = enabled;
        self.fullscreen_poll_at = std::time::Instant::now();
        if !enabled {
            if self.reducer.snapshot().fullscreen {
                self.reducer.dispatch(
                    ShellEvent::FullscreenChanged(false),
                    self.fullscreen_poll_at,
                );
                let media_playing = self.media.as_ref().is_some_and(|media| {
                    media.playing || media.sessions.iter().any(|session| session.playing)
                });
                let browser_foreground = self
                    .foreground_app
                    .as_ref()
                    .is_some_and(|app| app.kind == "browser");
                if media_playing || browser_foreground {
                    self.reducer
                        .dispatch(ShellEvent::Tuck, self.fullscreen_poll_at);
                }
            }
        }
        self.update_suppression_visibility();
        self.request_tick();
    }

    fn apply_display_affinity(&self) -> bool {
        let affinity = if self.exclude_from_screen_capture || self.screen_recording_stop.is_some() {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            0
        };
        unsafe { SetWindowDisplayAffinity(self.window, affinity) != 0 }
    }

    fn set_exclude_from_screen_capture(&mut self, enabled: bool) -> bool {
        if enabled == self.exclude_from_screen_capture {
            return true;
        }
        let previous = self.exclude_from_screen_capture;
        self.exclude_from_screen_capture = enabled;
        if self.apply_display_affinity() {
            true
        } else {
            self.exclude_from_screen_capture = previous;
            false
        }
    }

    fn poll_fullscreen(&mut self, now: std::time::Instant) {
        let media_playing = self.media.as_ref().is_some_and(|media| {
            media.playing || media.sessions.iter().any(|session| session.playing)
        });
        if now < self.fullscreen_poll_at {
            return;
        }
        let browser_foreground = self
            .foreground_app
            .as_ref()
            .is_some_and(|app| app.kind == "browser");
        let browser_fullscreen = browser_foreground
            && self
                .foreground_app
                .as_ref()
                .is_some_and(|app| app.fullscreen);
        if !self.hide_in_fullscreen
            && !media_playing
            && !browser_fullscreen
            && !self.reducer.snapshot().fullscreen
        {
            return;
        }
        let fullscreen = foreground_covers_primary(self.window);
        // Keep a small top-edge reveal target even while the overlay is hidden.
        let mut cursor = POINT::default();
        let reveal = unsafe { GetCursorPos(&mut cursor) } != 0
            && cursor.x >= self.position.x
            && cursor.x < self.position.x + self.width
            && cursor.y >= 0
            && cursor.y
                < if self.fullscreen_hidden {
                    (f64::from(FOLDED_HEIGHT) * self.scale).ceil() as i32
                } else {
                    (self.position.y + self.height)
                        .max((f64::from(FOLDED_HEIGHT) * self.scale).ceil() as i32)
                };
        let suppress = fullscreen
            && (self.hide_in_fullscreen || media_playing || browser_foreground)
            && !reveal;
        self.reducer
            .dispatch(ShellEvent::FullscreenChanged(suppress), now);
        if suppress {
            self.callbacks
                .pointer_inside
                .store(false, Ordering::Release);
            self.callbacks
                .pointer_poll_needed
                .store(false, Ordering::Release);
        }
        self.fullscreen_poll_at = now + FULLSCREEN_POLL_INTERVAL;
        self.update_suppression_visibility();
    }

    fn update_suppression_visibility(&mut self) {
        let suspended = self.reducer.snapshot().mode == ShellMode::Suspended;
        if suspended == self.fullscreen_hidden {
            return;
        }
        self.fullscreen_hidden = suspended;
        self.callbacks.is_open.store(false, Ordering::Release);
        unsafe {
            ShowWindow(
                self.window,
                if suspended {
                    SW_HIDE
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
        }
        self.sync_drop_catcher();
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            let result = match command {
                ShellCommand::ShowSettings => {
                    self.show_settings();
                    Ok(())
                }
                ShellCommand::ShowCommand => {
                    self.page = ShellPage::Home;
                    self.open_from_click();
                    if let Some(view) = &self.react_view {
                        let _ = view.evaluate_script("window.islandOpenCommand?.();");
                    }
                    self.push_react_state();
                    self.request_tick();
                    Ok(())
                }
                ShellCommand::FocusRestoreTarget(target) => {
                    self.set_focus_restore_target(target as HWND);
                    Ok(())
                }
                ShellCommand::DarkTheme(dark_theme) => self.set_dark_theme(dark_theme),
                ShellCommand::ReducedMotion(reduced_motion) => {
                    self.set_reduced_motion(reduced_motion)
                }
            };
            if result.is_err() {
                self.callbacks
                    .animation_failed
                    .store(true, Ordering::Release);
                self.callbacks.context.request_repaint();
            }
        }
    }

    fn request_tick(&self) {
        post_shell_tick(self.window, &self.callbacks);
    }

    fn merge_activity(&mut self) -> Result<(), String> {
        let mut next = ActivityIndicators::default();
        let mut lines = Vec::new();
        if self.privacy_indicator_enabled {
            if let Some(privacy) = self
                .observed_activity
                .as_ref()
                .and_then(|sample| sample.privacy.as_ref())
            {
                next.microphone_active = privacy.microphone_active();
                next.camera_active = privacy.camera_active();
                if next.microphone_active {
                    lines.push(format!(
                        "Microphone · {}",
                        privacy.microphone_apps.join(", ")
                    ));
                }
                if next.camera_active {
                    lines.push(format!("Camera · {}", privacy.camera_apps.join(", ")));
                }
            }
        }
        next.agent_runs = [("opencode", &self.acp_opencode), ("codex", &self.acp_codex)]
            .into_iter()
            .flat_map(|(provider, snapshot)| {
                snapshot
                    .sessions
                    .iter()
                    .filter(|session| session.active)
                    .map(move |session| AgentRun {
                        provider: provider.into(),
                        source: "acp".into(),
                        session_id: session.id.clone(),
                        label: session.title.clone(),
                        status: "running".into(),
                    })
            })
            .collect();
        next.opencode_active_count = self.acp_opencode.active_count;
        for run in &next.agent_runs {
            lines.insert(
                0,
                format!(
                    "{} · {}",
                    if run.provider == "codex" {
                        "Codex"
                    } else {
                        "OpenCode"
                    },
                    run.label
                ),
            );
        }
        next.details = lines.join("\n");
        if self.activity == next {
            return Ok(());
        }
        self.activity = next;
        self.update_status_label();
        self.push_react_state();
        self.render_frame()
    }
    fn poll_activity(&mut self, now: std::time::Instant) {
        if let Some(receiver) = &self.activity_receiver {
            match receiver.try_recv() {
                Ok(sample) => {
                    self.activity_receiver = None;
                    if let Some(privacy) = &sample.privacy {
                        if privacy.external_microphone_active {
                            self.wake_mic_clear_samples = 0;
                            if matches!(self.wake_word_status.as_str(), "arming" | "listening") {
                                self.wake_word.disarm();
                                self.wake_word_status = "paused".into();
                                self.push_react_state();
                            }
                        } else if self.wake_word_status == "paused" {
                            self.wake_mic_clear_samples =
                                self.wake_mic_clear_samples.saturating_add(1);
                            if self.wake_mic_clear_samples >= 2 {
                                match self
                                    .wake_word
                                    .arm(&self.wake_word_phrase, wake_sensitivity())
                                {
                                    Ok(_) => {
                                        self.wake_word_status = "arming".into();
                                        self.wake_word_error = None;
                                    }
                                    Err(error) => {
                                        self.wake_word_status = "error".into();
                                        self.wake_word_error = Some(error);
                                    }
                                }
                                self.push_react_state();
                            }
                        }
                    }
                    self.observed_activity = Some(sample);
                    let _ = self.merge_activity();
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.activity_receiver = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.activity_receiver.is_none() && now >= self.activity_poll_at {
            let (sender, receiver) = sync_channel(1);
            self.activity_receiver = Some(receiver);
            self.activity_poll_at = now + std::time::Duration::from_secs(2);
            thread::spawn(move || {
                let sample = ObservedActivity {
                    privacy: crate::platform::windows::privacy::read_current_activity().ok(),
                };
                let _ = sender.send(sample);
            });
        }
    }

    fn poll_foreground_app(&mut self, now: std::time::Instant) {
        if now < self.foreground_app_poll_at {
            return;
        }
        self.foreground_app_poll_at = now + std::time::Duration::from_millis(750);
        let next = foreground_app_snapshot(self.window, self.previous_foreground);
        if self.foreground_app != next {
            self.foreground_app = next;
            self.push_react_state();
        }
    }

    fn refresh_radios(&mut self, toggle: Option<RadioToggle>) {
        if self.radio_busy {
            return;
        }
        if toggle.is_none()
            && self
                .radio_last_check
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(10))
        {
            return;
        }
        self.radio_busy = true;
        self.radio_last_check = Some(std::time::Instant::now());
        let sender = self.radio_sender.clone();
        let cached = self.radios.clone();
        let window = self.window as usize;
        thread::spawn(move || {
            let _ = sender.send(read_radios(toggle, cached));
            unsafe { PostMessageW(window as HWND, WM_APP_SHELL_TICK, 0, 0) };
        });
        self.request_tick();
    }

    fn arm_timer(&self, delay: std::time::Duration) -> Result<(), String> {
        if self.frame_timer.is_null() {
            return Err("the edge shell frame scheduler is unavailable".into());
        }
        let ticks_100ns = delay.as_nanos().div_ceil(100).clamp(1, i64::MAX as u128) as i64;
        let due_time = ticks_100ns.saturating_neg();
        if unsafe { SetWaitableTimer(self.frame_timer, &due_time, 0, None, null(), 0) } == 0 {
            return Err(last_error("could not schedule the edge shell animation"));
        }
        Ok(())
    }

    fn request_startup_state(&mut self, enabled: Option<bool>) {
        if self.startup_pending {
            return;
        }
        self.startup_pending = true;
        self.push_react_state();
        let sender = self.startup_sender.clone();
        let window = self.window as usize;
        thread::spawn(move || {
            let status = match enabled {
                Some(enabled) => crate::platform::windows::startup::set_startup_task(enabled),
                None => crate::platform::windows::startup::read_startup_task(),
            }
            .unwrap_or_else(|error| error.status());
            let _ = sender.send(status);
            unsafe { PostMessageW(window as HWND, WM_APP_SHELL_TICK, 0, 0) };
        });
    }

    fn poll_startup_state(&mut self) {
        if let Ok(status) = self.startup_receiver.try_recv() {
            self.startup_status = status;
            self.startup_pending = false;
            self.push_react_state();
        }
    }

    fn run_native_tick(&mut self) -> Result<(), String> {
        let now = std::time::Instant::now();
        self.drain_wake_word_events();
        let compact_render_ready = std::mem::replace(&mut self.react_compact_render_ready, false);
        let compact_render_timed_out = self.pending_react_compact_render.is_some()
            && self.pending_react_compact_since.is_some_and(|started| {
                now.saturating_duration_since(started) >= REACT_COMPACT_RENDER_TIMEOUT
            });
        if compact_render_timed_out {
            trace_drop(format_args!(
                "react compact render acknowledgement timed out"
            ));
            self.pending_react_compact_render = None;
            self.pending_react_compact_since = None;
        }
        self.poll_startup_state();
        let previous_feedback_count = self.compact_feedbacks.len();
        let mut reminder_snoozed = false;
        let mut reminder_reactivated = false;
        for feedback in &mut self.compact_feedbacks {
            if feedback.kind == "reminder"
                && feedback.expires_at.is_some_and(|deadline| now >= deadline)
            {
                feedback.expires_at = None;
                feedback.display_remaining = feedback.display_duration;
                feedback.next_visible_at = Some(now + std::time::Duration::from_secs(90));
                feedback.next_visible_remaining = None;
                feedback.appeared_at = now;
                reminder_snoozed = true;
            } else if feedback.kind == "reminder"
                && feedback
                    .next_visible_at
                    .is_some_and(|deadline| now >= deadline)
            {
                feedback.next_visible_at = None;
                feedback.next_visible_remaining = None;
                feedback.appeared_at = now;
                reminder_reactivated = true;
            }
        }
        self.sync_compact_feedback_clocks();
        self.compact_feedbacks.retain(|feedback| {
            feedback.expires_at.is_none_or(|deadline| now < deadline)
                && feedback.stale_at.is_none_or(|deadline| now < deadline)
        });
        if self.compact_feedbacks.len() != previous_feedback_count
            || reminder_snoozed
            || reminder_reactivated
        {
            if self.compact_feedback_visible_id.is_some_and(|id| {
                !self
                    .compact_feedbacks
                    .iter()
                    .any(|feedback| feedback.id == id)
            }) {
                self.compact_feedback_visible_id = None;
            }
            self.update_status_label();
            self.push_react_state();
            self.request_tick();
        }
        if self.poll_command_app_scan() {
            self.push_react_state();
        }
        if self.poll_command_app_icons() {
            self.push_react_state();
        }
        if self.poll_command_file_search() {
            self.push_react_state();
        }
        self.poll_activity(now);
        self.poll_foreground_app(now);
        let mut latest_opencode = None;
        while let Ok(snapshot) = self.opencode_receiver.try_recv() {
            latest_opencode = Some(snapshot);
        }
        if let Some(snapshot) = latest_opencode {
            let reconnect_acp = self.opencode.installing
                && !snapshot.installing
                && snapshot.notice.contains("Connecting");
            let changed =
                serde_json::to_string(&snapshot).ok() != serde_json::to_string(&self.opencode).ok();
            self.opencode = snapshot;
            if reconnect_acp {
                let _ = self
                    .acp_opencode_actions
                    .send(crate::agent_acp::Action::Connect);
            }
            if changed {
                let _ = self.merge_activity();
                self.push_react_state();
            }
        }
        let mut latest_acp_opencode = None;
        while let Ok(snapshot) = self.acp_opencode_receiver.try_recv() {
            latest_acp_opencode = Some(snapshot);
        }
        if let Some(snapshot) = latest_acp_opencode {
            let previous_activity = self.activity.clone();
            self.acp_opencode = snapshot;
            let _ = self.merge_activity();
            if self.activity == previous_activity {
                self.push_react_state();
            }
        }
        let mut latest_acp_codex = None;
        while let Ok(snapshot) = self.acp_codex_receiver.try_recv() {
            latest_acp_codex = Some(snapshot);
        }
        if let Some(snapshot) = latest_acp_codex {
            let previous_activity = self.activity.clone();
            self.acp_codex = snapshot;
            let _ = self.merge_activity();
            if self.activity == previous_activity {
                self.push_react_state();
            }
        }
        if self.converter.poll() {
            self.push_react_state();
        }
        let conversion_paths = {
            let mut pending = self
                .callbacks
                .pending_conversion_drops
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *pending)
        };
        if !conversion_paths.is_empty() {
            self.open_conversion(conversion_paths, true);
        }
        let dropped_paths = {
            let mut pending = self
                .callbacks
                .pending_shelf_drops
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::take(&mut *pending)
        };
        if !dropped_paths.is_empty() {
            self.accept_dropped_paths(dropped_paths);
        }
        let mut latest_media = None;
        while let Ok(snapshot) = self.media_receiver.try_recv() {
            latest_media = Some(snapshot);
        }
        if let Some(snapshot) = latest_media {
            if self.media != snapshot {
                self.media = snapshot;
                self.media_playing.store(
                    self.media.as_ref().is_some_and(|media| media.playing),
                    Ordering::Relaxed,
                );
                if self.update_status_label() {
                    self.render_frame()?;
                }
                self.push_react_media();
            }
        }
        let mut shelf_icons_changed = false;
        while let Ok((generation, index, icon)) = self.shelf_icon_receiver.try_recv() {
            if generation == self.shelf_generation
                && let Some(slot) = self.shelf_icon_data.get_mut(index)
                && *slot != icon
            {
                *slot = icon;
                self.shelf_revision = self.shelf_revision.wrapping_add(1);
                shelf_icons_changed = true;
            }
        }
        if shelf_icons_changed && self.react_visible {
            self.push_react_state();
        }
        let mut latest_output = None;
        while let Ok(snapshot) = self.output_receiver.try_recv() {
            latest_output = Some(snapshot);
        }
        if let Some(snapshot) = latest_output {
            let peak_changed = self.output_peak != snapshot.peak;
            let controls_changed = self.output_volume != snapshot.volume
                || self.audio_devices != snapshot.devices
                || self.default_output_device_id != snapshot.default_device_id
                || self.selected_output_device_id != snapshot.selected_device_id;
            if peak_changed || controls_changed {
                self.output_volume = snapshot.volume;
                self.output_peak = snapshot.peak;
                self.audio_devices = snapshot.devices;
                self.default_output_device_id = snapshot.default_device_id;
                self.selected_output_device_id = snapshot.selected_device_id;
                // A meter tick only changes six CSS bars in the compact
                // WebView. Repainting the layered native surface at audio
                // rate wastes CPU and can disturb the shape spring.
                if controls_changed || (!self.react_compact_visible && !self.react_visible) {
                    self.render_frame()?;
                }
                if peak_changed
                    && self.react_compact_visible
                    && let Some(view) = &self.react_view
                {
                    let _ = view.evaluate_script(&format!(
                        "document.documentElement.style.setProperty('--audio-height','{}px');",
                        3.0 + f64::from(self.output_peak) * 0.14
                    ));
                }
            }
        }
        if let Ok(snapshot) = self.radio_receiver.try_recv() {
            self.radio_busy = false;
            self.radios = snapshot;
            self.push_react_state();
        }
        let selector_busy = self.selector_busy();
        if !selector_busy {
            self.poll_fullscreen(now);
        }
        let compact_hit_area = matches!(
            self.reducer.snapshot().mode,
            ShellMode::Tucked | ShellMode::Resting | ShellMode::HoverPending
        );
        if !selector_busy && compact_hit_area {
            let mut cursor = POINT::default();
            if unsafe { GetCursorPos(&mut cursor) } != 0
                && cursor.y < (f64::from(FOLDED_HEIGHT) * self.scale).ceil() as i32
                && cursor.y >= 0
                && cursor.x >= self.position.x
                && cursor.x < self.position.x + self.width
            {
                self.callbacks.pointer_inside.store(true, Ordering::Release);
            } else {
                self.callbacks
                    .pointer_inside
                    .store(false, Ordering::Release);
            }
        }
        if !selector_busy
            && !compact_hit_area
            && (self.callbacks.pointer_poll_needed.load(Ordering::Acquire)
                || self.react_visible
                || self.react_compact_visible)
        {
            let mut point = POINT::default();
            if unsafe { GetCursorPos(&mut point) } != 0 {
                let under = unsafe { WindowFromPoint(point) };
                if under == self.window || unsafe { IsChild(self.window, under) } != 0 {
                    self.callbacks.pointer_inside.store(true, Ordering::Release);
                } else {
                    self.callbacks
                        .pointer_inside
                        .store(false, Ordering::Release);
                    self.callbacks
                        .pointer_poll_needed
                        .store(false, Ordering::Release);
                }
            }
        }
        let interaction_delay = if self.screen_overlay_active {
            None
        } else {
            self.process_interaction(now, compact_render_ready || compact_render_timed_out)?
        };
        if let Some((mode, full_screen, quality)) = self.pending_screen_overlay
            && self.reducer.snapshot().mode == ShellMode::Resting
            && self.width_motion.value() <= self.folded_target_width() + 0.75
            && self.height_motion.value() <= f64::from(FOLDED_HEIGHT) + 0.75
        {
            self.pending_screen_overlay = None;
            self.launch_screen_overlay(mode, full_screen, quality);
        }
        let clock_delay = self.refresh_clock()?;
        let mut next = interaction_delay.map_or(clock_delay, |delay| delay.min(clock_delay));
        next = next.min(std::time::Duration::from_secs(2));
        if self.converter.state.busy || self.converter.state.engine_busy {
            next = next.min(std::time::Duration::from_millis(200));
        }
        if self.pending_screen_overlay.is_some() {
            next = next.min(self.frame_interval);
        }
        let feedback_deadline = self
            .compact_feedbacks
            .iter()
            .flat_map(|feedback| {
                [
                    feedback.expires_at,
                    feedback.stale_at,
                    feedback.next_visible_at,
                ]
            })
            .flatten()
            .min();
        if let Some(deadline) = feedback_deadline {
            next = next.min(deadline.saturating_duration_since(std::time::Instant::now()));
        }
        if let Some(deadline) = self
            .selected_compact_feedback()
            .and_then(|feedback| feedback.expires_at)
        {
            if let Some(fade_start) = deadline.checked_sub(SCREEN_FEEDBACK_EXIT)
                && now < fade_start
            {
                next = next.min(fade_start.saturating_duration_since(now));
            }
        }
        let selector_busy = self.selector_busy();
        if !selector_busy
            && (self.callbacks.pointer_poll_needed.load(Ordering::Acquire)
                || self.react_visible
                || self.react_compact_visible)
        {
            next = next.min(if self.react_compact_visible {
                std::time::Duration::from_millis(32)
            } else {
                pointer_poll_interval(self.refresh_period)
            });
        }
        if !selector_busy && self.reducer.snapshot().mode == ShellMode::Tucked {
            next = next.min(pointer_poll_interval(self.refresh_period));
        }
        if !selector_busy
            && (self.hide_in_fullscreen
                || self.media.as_ref().is_some_and(|media| {
                    media.playing || media.sessions.iter().any(|session| session.playing)
                })
                || self
                    .foreground_app
                    .as_ref()
                    .is_some_and(|app| app.kind == "browser" && app.fullscreen)
                || self.reducer.snapshot().fullscreen)
        {
            next = next.min(FULLSCREEN_POLL_INTERVAL);
        }
        next = next.min(self.foreground_app_poll_at.saturating_duration_since(now));
        if self.reducer.snapshot().mode == ShellMode::Tucked || self.auto_tuck_deadline.is_some() {
            next = next.min(std::time::Duration::from_millis(50));
        }
        // Keep the cadence anchored to this frame's start. Scheduling a full
        // 16 ms after rasterization adds the CPU render cost to every frame
        // (about 9 ms in the optimized build), dropping the motion to ~40 fps.
        self.arm_timer(next.saturating_sub(now.elapsed()))
    }

    fn open_from_click(&mut self) {
        if !self.is_open() && self.previous_foreground.is_null() {
            self.remember_foreground();
        }
        self.reducer
            .dispatch(ShellEvent::Open, std::time::Instant::now());
        self.callbacks
            .is_open
            .store(self.is_open(), Ordering::Release);
        self.callbacks.is_pinned.store(
            self.reducer.snapshot().mode == ShellMode::Pinned,
            Ordering::Release,
        );
    }

    fn show_settings(&mut self) {
        self.open_from_click();
        if let Some(view) = &self.react_view {
            let _ = view.evaluate_script("window.islandOpenSettings?.();");
        }
        self.push_react_state();
        self.request_tick();
    }

    fn handle_native_click(&mut self) {
        // The shaped parent still receives clicks outside WebView2 child HWNDs.
        // It only opens the React surface; all page actions belong to React.
        if !self.is_open() {
            self.page = ShellPage::Home;
            self.open_from_click();
            self.push_react_state();
        }
    }

    fn handle_outside_click(&mut self) {
        if self.wake_surface {
            if let Some(view) = &self.react_view {
                let _ = view
                    .evaluate_script("window.dispatchEvent(new Event('island:voice-dismiss'));");
            }
            self.wake_surface = false;
            self.wake_closing = true;
            self.reducer
                .dispatch(ShellEvent::Close, std::time::Instant::now());
            self.push_react_state();
            self.request_tick();
            return;
        }
        if !self.click_to_open
            || !self.close_on_outside_click
            || !self.is_open()
            || self.reducer.snapshot().mode == ShellMode::Pinned
            || self.selector_busy()
        {
            return;
        }
        self.reducer
            .dispatch(ShellEvent::Close, std::time::Instant::now());
        self.request_tick();
    }
    fn save_note(&mut self) {
        if !self.note_dirty {
            return;
        }
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let directory = std::path::PathBuf::from(local).join("Island");
        if let Some(folder) = self.notebook.folders.get_mut(self.notebook.selected) {
            folder.text = self.note.clone();
        }
        if std::fs::create_dir_all(&directory).is_ok()
            && serde_json::to_vec(&self.notebook)
                .ok()
                .is_some_and(|data| std::fs::write(directory.join("notes.json"), data).is_ok())
        {
            self.note_dirty = false;
        }
    }

    fn refresh_weather_if_stale(&mut self) {
        if self.weather_city.trim().is_empty() || self.weather_receiver.is_some() {
            return;
        }
        if self
            .weather_updated_at
            .is_some_and(|at| unix_seconds().saturating_sub(at) < 15 * 60)
        {
            return;
        }
        if self
            .weather_last_attempt
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2 * 60))
        {
            return;
        }
        self.start_weather_request();
    }

    fn start_weather_request(&mut self) {
        if self.weather_receiver.is_some() {
            return;
        }
        let city = self.weather_city.trim().to_owned();
        if city.is_empty() {
            return;
        }
        let _ = save_weather_city(&city);
        self.weather_last_attempt = Some(std::time::Instant::now());
        self.weather_error = None;
        let units = self.weather_units;
        let cached_place = self.weather_data.as_ref().and_then(|data| {
            Some((
                data["latitude"].as_f64()?,
                data["longitude"].as_f64()?,
                data["location"].as_str()?.to_owned(),
                data["country"].as_str().unwrap_or("").to_owned(),
            ))
        });
        let (sender, receiver) = sync_channel(1);
        self.weather_receiver = Some(receiver);
        if self.weather_data.is_none() {
            self.weather_lines = vec!["Fetching forecast…".to_owned()];
        }
        thread::spawn(move || {
            let _ = sender.send(fetch_weather(&city, units, cached_place));
        });
        unsafe { SetTimer(self.window, WEATHER_POLL_TIMER_ID, 250, None) };
        self.redraw_page();
    }

    fn capture_clipboard_update(&mut self) {
        if !self.clipboard_history_enabled || clipboard_source_is_private() {
            return;
        }
        let Some(text) = read_clipboard_text() else {
            return;
        };
        if text.is_empty() || self.clipboard_items.first() == Some(&text) {
            return;
        }
        let show_feedback = !self.selector_busy()
            && !self.screen_clipboard_values.contains(&text)
            && !self.callbacks.is_open.load(Ordering::Acquire);
        let preview = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or(&text)
            .trim()
            .chars()
            .take(42)
            .collect::<String>();
        self.clipboard_items.insert(0, text);
        self.clipboard_items.truncate(self.clipboard_limit);
        if show_feedback {
            let copy = self.clipboard_items.first().cloned();
            self.set_compact_screen_feedback(
                "copy",
                preview,
                copy,
                None,
                Some(std::time::Duration::from_millis(2100)),
            );
        }
        if self.page == ShellPage::Clipboard || self.page == ShellPage::Home {
            self.redraw_page();
        }
    }

    fn poll_weather(&mut self) {
        let Some(receiver) = self.weather_receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok((lines, data))) => {
                let fetched_at = unix_seconds();
                save_weather_cache(&WeatherCache {
                    city: self.weather_city.clone(),
                    units: self.weather_units,
                    fetched_at,
                    lines: lines.clone(),
                    data: data.clone(),
                });
                self.weather_lines = lines;
                self.weather_data = Some(data);
                self.weather_error = None;
                self.weather_updated_at = Some(fetched_at);
            }
            Ok(Err(error)) => {
                self.weather_lines = vec![error.clone()];
                self.weather_error = Some(error);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.weather_lines = vec!["Forecast request stopped.".to_owned()];
                self.weather_error = self.weather_lines.first().cloned();
            }
        }
        self.weather_receiver = None;
        unsafe { KillTimer(self.window, WEATHER_POLL_TIMER_ID) };
        self.redraw_page();
    }

    fn accept_dropped_files(&mut self, drop: windows_sys::Win32::UI::Shell::HDROP) {
        let count = unsafe { DragQueryFileW(drop, u32::MAX, null_mut(), 0) }.min(32);
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = unsafe { DragQueryFileW(drop, index, null_mut(), 0) } as usize;
            if length == 0 || length > 32_767 {
                continue;
            }
            let mut buffer = vec![0u16; length + 1];
            let copied =
                unsafe { DragQueryFileW(drop, index, buffer.as_mut_ptr(), buffer.len() as u32) }
                    as usize;
            paths.push(std::path::PathBuf::from(String::from_utf16_lossy(
                &buffer[..copied],
            )));
        }
        unsafe { DragFinish(drop) };
        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point);
        }
        self.callbacks.drop_convertible.store(
            paths.iter().all(|p| crate::conversion::could_convert(p)),
            Ordering::Release,
        );
        match resolve_drop_intent(&self.callbacks, point.x, point.y, true) {
            1 => self.open_conversion(paths, true),
            2 => self.accept_dropped_paths(paths),
            _ => {}
        }
    }

    fn open_conversion(&mut self, paths: Vec<std::path::PathBuf>, from_drop: bool) {
        self.converter.add(paths);
        if from_drop {
            self.drop_handoff_sequence = self.drop_handoff_sequence.wrapping_add(1).max(1);
        }
        self.callbacks
            .shelf_drag_hover_hint
            .store(false, Ordering::Release);
        self.callbacks.drop_intent.store(0, Ordering::Release);
        self.shelf_drop_transition_at = Some(std::time::Instant::now());
        self.drop_open_hold_until = Some(std::time::Instant::now() + DROP_HANDOFF_DURATION);
        self.page = ShellPage::Convert;
        if !self.is_open() {
            self.reducer
                .dispatch(ShellEvent::Open, std::time::Instant::now());
        }
        self.last_pointer_inside = true;
        self.callbacks
            .is_open
            .store(self.is_open(), Ordering::Release);
        self.redraw_page();
        self.push_react_state();
    }

    fn accept_dropped_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        if !self.modules.shelf || paths.is_empty() {
            return;
        }
        let mut added = Vec::new();
        for path in paths {
            if path.is_absolute()
                && path.exists()
                && self.shelf_items.len() < 32
                && !self.shelf_items.contains(&path)
            {
                let shelf_index = self.shelf_items.len();
                self.shelf_items.push(path.clone());
                self.shelf_icon_data.push(None);
                added.push((shelf_index, path));
            }
        }
        if !added.is_empty() {
            self.shelf_revision = self.shelf_revision.wrapping_add(1);
            let generation = self.shelf_generation;
            let icon_sender = self.shelf_icon_sender.clone();
            thread::spawn(move || {
                for (index, path) in added {
                    if icon_sender
                        .send((generation, index, shelf_item_visual_data_url(&path)))
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        save_shelf_paths(&self.shelf_items);
        self.callbacks
            .shelf_drag_hover_hint
            .store(false, Ordering::Release);
        self.callbacks.drop_intent.store(0, Ordering::Release);
        self.shelf_drop_transition_at = Some(std::time::Instant::now());
        self.drop_open_hold_until = Some(std::time::Instant::now() + DROP_HANDOFF_DURATION);
        self.page = ShellPage::Shelf;
        if !self.is_open() {
            self.reducer
                .dispatch(ShellEvent::Open, std::time::Instant::now());
        }
        self.last_pointer_inside = true;
        self.callbacks
            .is_open
            .store(self.is_open(), Ordering::Release);
        self.redraw_page();
        self.push_react_state();
    }

    fn refresh_missing_shelf_icons(&self) {
        let generation = self.shelf_generation;
        let missing: Vec<_> = self
            .shelf_items
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                self.shelf_icon_data
                    .get(*index)
                    .map_or(true, Option::is_none)
            })
            .map(|(index, path)| (index, path.clone()))
            .collect();
        if missing.is_empty() {
            return;
        }
        let sender = self.shelf_icon_sender.clone();
        thread::spawn(move || {
            for (index, path) in missing {
                if sender
                    .send((generation, index, shelf_item_visual_data_url(&path)))
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    fn redraw_page(&mut self) {
        if self.render_frame().is_err() {
            self.callbacks
                .animation_failed
                .store(true, Ordering::Release);
            self.callbacks.context.request_repaint();
        }
    }

    fn toggle_from_hotkey(&mut self) {
        let was_open = self.is_open();
        if !was_open && self.previous_foreground.is_null() {
            self.remember_foreground();
        }
        self.reducer
            .dispatch(ShellEvent::ToggleOpen, std::time::Instant::now());
        self.callbacks
            .is_open
            .store(self.is_open(), Ordering::Release);
        if was_open && !self.is_open() {
            self.schedule_focus_restore();
        }
    }

    fn is_open(&self) -> bool {
        matches!(
            self.reducer.snapshot().mode,
            ShellMode::Expanded | ShellMode::Pinned | ShellMode::TextEntry
        )
    }

    fn update_status_label(&mut self) -> bool {
        if self.screen_feedback_active() {
            return false;
        }
        let status = if self
            .notebook
            .reminder_at
            .is_some_and(|at| at <= unix_seconds())
        {
            "Reminder due".to_owned()
        } else {
            self.activity
                .label()
                .or_else(|| {
                    self.media
                        .as_ref()
                        .filter(|media| media.playing)
                        .map(|media| format!("♫ {}", bounded_shell_line(&media.title, 30)))
                })
                .unwrap_or_else(|| {
                    current_status_label(
                        self.clock_indicator_enabled,
                        self.battery_indicator_enabled,
                    )
                })
        };
        if self.status_label == status {
            return false;
        }
        self.status_label = status;
        true
    }

    fn remember_foreground(&mut self) {
        let foreground = unsafe { GetForegroundWindow() };
        if self.is_external_window(foreground) {
            self.previous_foreground = foreground;
        }
    }

    fn is_external_window(&self, candidate: HWND) -> bool {
        if candidate.is_null() || unsafe { IsWindow(candidate) } == 0 {
            return false;
        }
        let mut candidate_process = 0;
        let mut shell_process = 0;
        unsafe {
            GetWindowThreadProcessId(candidate, &mut candidate_process);
            GetWindowThreadProcessId(self.window, &mut shell_process);
        }
        candidate_process != 0 && candidate_process != shell_process
    }

    fn schedule_focus_restore(&mut self) {
        if !self.is_external_window(self.previous_foreground) {
            self.previous_foreground = null_mut();
            return;
        }
        let delay = u32::try_from(FOCUS_RESTORE_DELAY.as_millis()).unwrap_or(u32::MAX);
        if unsafe { SetTimer(self.window, FOCUS_RESTORE_TIMER_ID, delay.max(1), None) } == 0 {
            self.restore_foreground();
        }
    }

    fn restore_foreground(&mut self) {
        unsafe { KillTimer(self.window, FOCUS_RESTORE_TIMER_ID) };
        let target = std::mem::replace(&mut self.previous_foreground, null_mut());
        if !self.is_external_window(target) {
            return;
        }
        if unsafe { SetForegroundWindow(target) } == 0 {
            crate::platform::windows::report_diagnostic(
                crate::diagnostics::DiagnosticCode::ForegroundRestoreDenied,
            );
        }
    }

    /// Consume coalesced native pointer state and apply any due reducer timer.
    /// The returned delay is scheduled on the shell HWND's native timer.
    pub fn process_interaction(
        &mut self,
        now: std::time::Instant,
        force_react_sync: bool,
    ) -> Result<Option<std::time::Duration>, String> {
        let was_tucked = self.reducer.snapshot().mode == ShellMode::Tucked;
        let selector_pending = self.pending_screen_overlay.is_some();
        let hover_suppressed = selector_pending
            || self.screen_feedback_active()
            || (self.react_compact_visible && self.compact_feedback_selected());
        let pointer_inside =
            !hover_suppressed && self.callbacks.pointer_inside.load(Ordering::Acquire);
        let media_playing = self.media.as_ref().is_some_and(|media| {
            media.playing || media.sessions.iter().any(|session| session.playing)
        });
        let browser_foreground = self
            .foreground_app
            .as_ref()
            .is_some_and(|app| app.kind == "browser");
        let auto_tucked = (browser_foreground || media_playing)
            && !hover_suppressed
            && self.screen_recording_stop.is_none()
            && !self.callbacks.shelf_drag_active.load(Ordering::Acquire)
            && !self.callbacks.shelf_drag_hover_hint.load(Ordering::Acquire);
        if auto_tucked && !pointer_inside && self.reducer.snapshot().mode == ShellMode::Resting {
            let leave_delay = if self.compact_context_tools > 0 {
                700
            } else {
                350
            };
            let deadline = *self
                .auto_tuck_deadline
                .get_or_insert(now + std::time::Duration::from_millis(leave_delay));
            if now >= deadline {
                self.reducer.dispatch(ShellEvent::Tuck, now);
                self.hover_intent_deadline = None;
                self.auto_tuck_deadline = None;
                self.compact_context_tools = 0;
            }
        } else if self.reducer.snapshot().mode == ShellMode::Tucked
            && (!auto_tucked || pointer_inside)
        {
            self.auto_tuck_deadline = None;
            self.reducer.dispatch(ShellEvent::Untuck, now);
        } else {
            self.auto_tuck_deadline = None;
        }
        let drag_active = !hover_suppressed
            && (self.callbacks.shelf_drag_active.load(Ordering::Acquire)
                || self.callbacks.shelf_drag_hover_hint.load(Ordering::Acquire));
        let drag_active = drag_active && !self.fullscreen_hidden;
        let react_drop_active = self.callbacks.shelf_drag_active.load(Ordering::Acquire);
        let react_drop_intent = self.callbacks.drop_intent.load(Ordering::Acquire);
        if react_drop_active != self.last_react_drop_active
            || react_drop_intent != self.last_react_drop_intent
        {
            self.last_react_drop_active = react_drop_active;
            self.last_react_drop_intent = react_drop_intent;
            self.push_react_state();
        }
        let drop_open_hold_active = self
            .drop_open_hold_until
            .is_some_and(|deadline| now < deadline);
        if self
            .drop_open_hold_until
            .is_some_and(|deadline| now >= deadline)
        {
            self.drop_open_hold_until = None;
        }
        if drag_active {
            self.hover_intent_deadline = None;
            if self.reducer.snapshot().mode == ShellMode::HoverPending {
                self.reducer.dispatch(ShellEvent::PointerLeft, now);
            }
        }
        if pointer_inside != self.last_pointer_inside {
            if !pointer_inside && drop_open_hold_active && self.is_open() {
                // External drag sources can briefly report the cursor outside
                // the layered HWND as OLE releases capture. Keep the drop
                // destination available through that handoff; the deadline
                // below then applies the normal leave behavior if it stayed out.
            } else {
                self.last_pointer_inside = pointer_inside;
                if drag_active {
                    // A real file drag owns the hover interaction. Do not begin the
                    // normal shell-open timer while the user is aiming at Shelf.
                } else if pointer_inside && self.screen_recording_stop.is_some() {
                    // Keep the collapsed recorder pill visible when selection
                    // closes under a cursor that is still over the Island.
                    self.hover_intent_deadline = None;
                } else if pointer_inside && self.compact_status_visible == "updates" {
                    // Keep the actionable update pill under the pointer long enough
                    // for its click to reach React instead of opening on hover.
                    self.hover_intent_deadline = None;
                } else if pointer_inside
                    && !self.click_to_open
                    && self.reducer.snapshot().mode == ShellMode::Resting
                    && !self.fullscreen_hidden
                {
                    self.hover_intent_deadline = Some(now + HOVER_INTENT_DWELL);
                } else {
                    self.hover_intent_deadline = None;
                    if !pointer_inside {
                        self.reducer.dispatch(ShellEvent::PointerLeft, now);
                    } else {
                        self.reducer.dispatch(ShellEvent::PointerEntered, now);
                    }
                }
            }
        }
        if self.last_shelf_drag_active
            && !drag_active
            && pointer_inside
            && !self.click_to_open
            && self.reducer.snapshot().mode == ShellMode::Resting
            && !self.fullscreen_hidden
        {
            // DragLeave restores the ordinary hover path, with its short
            // intent dwell, rather than immediately opening under the cursor.
            self.hover_intent_deadline = Some(now + HOVER_INTENT_DWELL);
        }
        self.last_shelf_drag_active = drag_active;
        if !drag_active
            && !self.click_to_open
            && let Some(deadline) = self.hover_intent_deadline
            && now >= deadline
        {
            self.hover_intent_deadline = None;
            if pointer_inside
                && self.compact_status_visible != "updates"
                && self.reducer.snapshot().mode == ShellMode::Resting
                && !self.fullscreen_hidden
            {
                self.reducer.dispatch(ShellEvent::PointerEntered, now);
            }
        }
        if let Some(timer) = self.reducer.snapshot().timer
            && now >= timer.deadline
        {
            self.reducer.dispatch(
                ShellEvent::TimerElapsed {
                    generation: timer.generation,
                },
                now,
            );
        }
        self.callbacks
            .is_open
            .store(self.is_open(), Ordering::Release);

        if self.wake_closing && !self.react_visible {
            self.wake_closing = false;
            self.set_expanded_body_width(self.wide_layout);
            self.push_react_state();
        }
        if self.wants_expanded_without_exit() {
            if self.content_exit_deadline.take().is_some() && self.react_visible {
                if let Some(view) = &self.react_view {
                    let _ = view.evaluate_script("window.islandShellPhase?.('enter');");
                }
            }
        } else if self.react_visible
            && self.content_exit_deadline.is_none()
            && !self.reduced_motion
            && !self.reducer.snapshot().fullscreen
        {
            self.content_exit_deadline = Some(now + CONTENT_EXIT_DURATION);
            if let Some(view) = &self.react_view {
                let _ = view.evaluate_script("window.islandShellPhase?.('exit');");
            }
        }

        // Prime the first Home target from its React layout before the opening
        // spring starts. Otherwise the first visible React measurement retargets
        // the native contour near the end of that spring, which looks like a
        // second, small expansion. Subsequent measurements remain authoritative.
        if !self.react_ever_visible
            && self.page == ShellPage::Home
            && self.animation_target.1 < f64::from(FOLDED_HEIGHT) + 1.0
        {
            self.expanded_body_height = self.initial_home_expanded_body_height();
        }
        let target = self.visual_target();
        let mut snapped = false;
        let mut retargeted = false;
        if target != self.animation_target {
            trace_drop(format_args!(
                "shell target height={:.0} light={}",
                target.1, !self.dark_theme
            ));
            if self.wants_expanded_without_exit()
                && self.animation_target.1 <= f64::from(FOLDED_HEIGHT) + 1.0
                && target.1 > f64::from(FOLDED_HEIGHT) + 1.0
            {
                // Refresh the active page before its opening spring starts.
                // Cached UI stays visible while any slow provider refreshes.
                self.prepare_page_content();
            }
            if self.reduced_motion {
                self.width_motion.snap_to(target.0);
                self.height_motion.snap_to(target.1);
                snapped = true;
            } else {
                let already_expanded = matches!(
                    self.reducer.snapshot().mode,
                    ShellMode::Expanded | ShellMode::Pinned | ShellMode::TextEntry
                ) && self.animation_target.1
                    >= f64::from(FOLDED_HEIGHT) + 1.0;
                let compact_change = target.1 <= f64::from(FOLDED_HEIGHT) + 1.0
                    && self.animation_target.1 <= f64::from(FOLDED_HEIGHT) + 1.0;
                let spring = if compact_change {
                    COMPACT_STATUS_SPRING
                } else if already_expanded {
                    IN_PLACE_LAYOUT_SPRING
                } else if target.0 > self.width_motion.value()
                    || target.1 > self.height_motion.value()
                {
                    SHELL_OPEN_SPRING
                } else {
                    SHELL_COLLAPSE_SPRING
                };
                self.width_motion
                    .retarget_without_reversal(target.0, spring);
                self.height_motion
                    .retarget_without_reversal(target.1, spring);
            }
            self.animation_target = target;
            // Ignore idle time (including the hover delay) when beginning a
            // spring. Otherwise its first integration step consumes a stale
            // 50 ms chunk and visibly jumps into motion.
            self.last_frame = now;
            retargeted = true;
        }

        let elapsed = if retargeted {
            std::time::Duration::ZERO
        } else {
            now.saturating_duration_since(self.last_frame)
        };
        self.last_frame = now;
        let previous_size = (self.width_motion.value(), self.height_motion.value());
        let width_animating = self.width_motion.advance(elapsed);
        let height_animating = self.height_motion.advance(elapsed);
        let previous_tuck = self.tuck_motion.value();
        let tuck_target = if self.reducer.snapshot().mode == ShellMode::Tucked {
            1.0
        } else {
            0.0
        };
        if self.reduced_motion {
            self.tuck_motion.snap_to(tuck_target);
        } else {
            self.tuck_motion
                .retarget_without_reversal(tuck_target, COMPACT_STATUS_SPRING);
        }
        let tuck_elapsed = if was_tucked != (self.reducer.snapshot().mode == ShellMode::Tucked) {
            std::time::Duration::ZERO
        } else {
            elapsed
        };
        let tuck_animating = self.tuck_motion.advance(tuck_elapsed);
        let drop_target = match self.callbacks.drop_intent.load(Ordering::Acquire) {
            1 => -1.0,
            2 => 1.0,
            _ => 0.0,
        };
        if self.reduced_motion {
            self.drop_motion.snap_to(drop_target);
        } else {
            self.drop_motion
                .retarget_without_reversal(drop_target, IN_PLACE_LAYOUT_SPRING);
        }
        let drop_animating = self.drop_motion.advance(elapsed);
        let feedback_animating = self.screen_feedback_animation(now).1;
        let live_compact_animation = !self.reduced_motion
            && !self.screen_feedback_active()
            && self.reducer.snapshot().mode == ShellMode::Resting
            && self.opencode.active_count > 0
            && !self.react_compact_visible
            && !self.media.as_ref().is_some_and(|media| media.playing);
        // Reconcile a completed compact handoff even on a tick with no size
        // change. Visibility must not depend on another animation frame.
        let compact_needs_reveal = self.react_compact_visible
            && self.react_compact_faded
            && self.pending_react_compact_render.is_none()
            && self.width_motion.is_settled()
            && self.height_motion.is_settled();
        if was_tucked != (self.reducer.snapshot().mode == ShellMode::Tucked)
            || snapped
            || compact_needs_reveal
            || force_react_sync
            || width_animating
            || height_animating
            || tuck_animating
            || previous_tuck != self.tuck_motion.value()
            || drop_animating
            || feedback_animating
            || live_compact_animation
            || previous_size != (self.width_motion.value(), self.height_motion.value())
        {
            self.render_frame()?;
        }
        if width_animating
            || height_animating
            || tuck_animating
            || feedback_animating
            || drop_animating
        {
            return Ok(Some(self.frame_interval));
        }
        if live_compact_animation {
            return Ok(Some(std::time::Duration::from_millis(240)));
        }
        if self.pending_react_compact_render.is_some() {
            // Keep a low-frequency native timer alive if WebView2 throttles
            // requestAnimationFrame while occluded (for example, on lock).
            // The timeout above then restores the compact viewport reliably.
            return Ok(Some(std::time::Duration::from_millis(32)));
        }

        let timer_delay = self
            .reducer
            .snapshot()
            .timer
            .map(|timer| timer.deadline.saturating_duration_since(now));
        let exit_delay = self
            .content_exit_deadline
            // Retain the expired marker to avoid restarting the exit phase,
            // but never rearm a waitable timer for an already consumed exit.
            .filter(|deadline| *deadline > now)
            .map(|deadline| deadline.saturating_duration_since(now));
        let hover_intent_delay = self
            .hover_intent_deadline
            .map(|deadline| deadline.saturating_duration_since(now));
        let drop_open_hold_delay = self
            .drop_open_hold_until
            .map(|deadline| deadline.saturating_duration_since(now));
        Ok([
            timer_delay,
            exit_delay,
            hover_intent_delay,
            drop_open_hold_delay,
        ]
        .into_iter()
        .flatten()
        .min())
    }

    fn wants_expanded_without_exit(&self) -> bool {
        if self.wake_surface {
            return true;
        }
        let mode = self.reducer.snapshot().mode;
        let folded_width = self.folded_target_width();
        let remain_expanded_while_closing =
            mode == ShellMode::HoverPending && self.width_motion.value() > folded_width + 0.5;
        matches!(
            mode,
            ShellMode::Expanded | ShellMode::Pinned | ShellMode::TextEntry
        ) || remain_expanded_while_closing
    }

    fn folded_target_width(&self) -> f64 {
        let privacy_width = 20.0
            * f64::from(
                u8::from(self.activity.microphone_active) + u8::from(self.activity.camera_active),
            );
        if self.click_to_open
            && self.compact_context_tools > 0
            && matches!(self.compact_status_visible.as_str(), "idle" | "media")
        {
            // Five fixed icon targets, an app label, and an independent privacy lane.
            let context_width = 110.0
                + self.compact_context_tools as f64 * 27.0
                + if privacy_width > 0.0 {
                    privacy_width + 20.0
                } else {
                    0.0
                };
            let idle_width = self
                .measure_text(&self.status_label)
                .map(|size| f64::from(size.cx) / self.scale)
                .unwrap_or_default();
            return self
                .shape
                .folded_body_width()
                .max((idle_width + privacy_width + 58.0).min(420.0))
                .max(context_width);
        }
        if self.compact_feedback_selected() {
            let detail_width = self
                .selected_compact_feedback()
                .and_then(|feedback| self.measure_text(&feedback.label).ok())
                .map(|size| f64::from(size.cx) / self.scale)
                .unwrap_or(0.0);
            return self
                .shape
                .folded_body_width()
                .max((detail_width + 122.0 + privacy_width).clamp(260.0, 380.0));
        }
        if self.compact_status_visible == "media"
            && self.media.as_ref().is_some_and(|media| media.playing)
        {
            return self.shape.folded_body_width().max(286.0 + privacy_width);
        }
        if self.compact_status_visible == "agent"
            && self.acp_opencode.active_count + self.acp_codex.active_count > 0
        {
            return self.shape.folded_body_width().max(234.0 + privacy_width);
        }
        if self.compact_status_visible == "agent-approval" {
            return self.shape.folded_body_width().max(255.0 + privacy_width);
        }
        if self.compact_status_visible == "recording" && self.screen_recording_stop.is_some() {
            return self.shape.folded_body_width().max(210.0 + privacy_width);
        }
        if self.compact_status_visible == "weather" {
            return self.shape.folded_body_width().max(205.0 + privacy_width);
        }
        let text_width = self
            .measure_text(&self.status_label)
            .map(|size| f64::from(size.cx) / self.scale)
            .unwrap_or_default();
        self.shape
            .folded_body_width()
            .max((text_width + privacy_width + 58.0).min(420.0))
    }

    fn initial_home_expanded_body_height(&self) -> f64 {
        INITIAL_HOME_EXPANDED_BODY_HEIGHT
            + if self.media.is_some() {
                HOME_MEDIA_HEIGHT_DELTA
            } else {
                0.0
            }
    }

    fn expanded_layout_height(&self) -> f64 {
        if self.settings_layout_open {
            self.settings_layout_height
                .unwrap_or(self.expanded_body_height)
        } else {
            self.expanded_body_height
        }
    }

    fn visual_target(&self) -> (f64, f64) {
        if (self.callbacks.shelf_drag_active.load(Ordering::Acquire)
            || self.callbacks.shelf_drag_hover_hint.load(Ordering::Acquire))
            && !self.fullscreen_hidden
            && self.reducer.snapshot().mode == ShellMode::Resting
        {
            (
                self.shape.folded_body_width().max(SHELF_PEEK_SIZE.0),
                SHELF_PEEK_SIZE.1,
            )
        } else if self.wants_expanded_without_exit()
            || self
                .content_exit_deadline
                .is_some_and(|deadline| std::time::Instant::now() < deadline)
        {
            let expanded_width = self.expanded_body_width;
            (expanded_width, self.expanded_layout_height())
        } else {
            let folded_width = self.folded_target_width();
            (folded_width, f64::from(FOLDED_HEIGHT))
        }
    }

    fn prepare_page_content(&mut self) {
        match self.page {
            ShellPage::Home => {
                let app_query = self.command_app_query.clone();
                let file_query = self.command_file_query.clone();
                // Keep the command palette's app index warm even when its
                // React state survived compaction and won't remount its effect.
                self.start_command_app_scan();
                self.update_command_app_results(&app_query);
                if !file_query.trim().is_empty() {
                    self.start_command_file_search(&file_query);
                }
            }
            ShellPage::Weather => self.refresh_weather_if_stale(),
            ShellPage::Controls => self.refresh_radios(None),
            ShellPage::Agents | ShellPage::Ask => {
                let _ = self
                    .opencode_actions
                    .send(crate::opencode_control::Action::Refresh);
            }
            // These pages are native snapshots, so pushing below supplies the
            // newest state before React content is revealed.
            ShellPage::Clipboard
            | ShellPage::Shelf
            | ShellPage::Convert
            | ShellPage::Text
            | ShellPage::Note => {}
        }
        self.push_react_state();
    }

    fn feedback_matches_compact_status(feedback: &CompactScreenFeedback, status: &str) -> bool {
        match status {
            "reminder" => feedback.kind == "reminder",
            "notification" => feedback.kind == "notice" && feedback.phase != "active",
            "tool-active" => feedback.phase == "active" && feedback.kind != "record",
            "tool-result" => {
                !["reminder", "notice"].contains(&feedback.kind) && feedback.phase != "active"
            }
            _ => false,
        }
    }

    fn selected_compact_feedback(&self) -> Option<&CompactScreenFeedback> {
        let now = std::time::Instant::now();
        let valid = |feedback: &&CompactScreenFeedback| {
            feedback.expires_at.is_none_or(|deadline| now < deadline)
                && feedback.stale_at.is_none_or(|deadline| now < deadline)
                && feedback
                    .next_visible_at
                    .is_none_or(|deadline| now >= deadline)
                && Self::feedback_matches_compact_status(feedback, &self.compact_status_visible)
        };
        self.compact_feedbacks
            .iter()
            .find(|feedback| Some(feedback.id) == self.compact_feedback_visible_id)
            .filter(valid)
            .or_else(|| self.compact_feedbacks.iter().find(valid))
    }

    fn screen_feedback_active(&self) -> bool {
        if !self.compact_presentation_available() {
            return false;
        }
        let now = std::time::Instant::now();
        self.selected_compact_feedback().is_some_and(|feedback| {
            feedback.phase == "active"
                || feedback.kind == "reminder"
                || feedback.expires_at.is_some_and(|deadline| now < deadline)
        })
    }

    fn compact_feedback_selected(&self) -> bool {
        self.selected_compact_feedback().is_some()
    }

    fn screen_feedback_animation(&self, now: std::time::Instant) -> (f64, bool) {
        if self.reduced_motion {
            return (1.0, false);
        }
        let Some(feedback) = self.selected_compact_feedback() else {
            return (1.0, false);
        };
        let smoothstep = |value: f64| {
            let value = value.clamp(0.0, 1.0);
            value * value * (3.0 - 2.0 * value)
        };
        let enter_age = now.saturating_duration_since(feedback.appeared_at);
        let entering = enter_age < SCREEN_FEEDBACK_ENTER;
        let enter = smoothstep(enter_age.as_secs_f64() / SCREEN_FEEDBACK_ENTER.as_secs_f64());
        let (exit, exiting) = feedback.expires_at.map_or((1.0, false), |deadline| {
            let remaining = deadline.saturating_duration_since(now);
            (
                smoothstep(remaining.as_secs_f64() / SCREEN_FEEDBACK_EXIT.as_secs_f64()),
                remaining < SCREEN_FEEDBACK_EXIT,
            )
        });
        (enter * exit, entering || exiting)
    }

    /// Refreshes only when the displayed minute changes; returns time until next UI wake.
    pub fn refresh_clock(&mut self) -> Result<std::time::Duration, String> {
        let instant_now = std::time::Instant::now();
        if instant_now < self.clock_refresh_at {
            return Ok(self.clock_refresh_at.saturating_duration_since(instant_now));
        }
        if let Some(at) = self.notebook.reminder_at
            && at <= unix_seconds()
            && self.reminder_notified_at != Some(at)
        {
            self.reminder_notified_at = Some(at);
            let preview: String = self.note.trim().chars().take(32).collect();
            self.set_compact_screen_feedback(
                "reminder",
                if preview.is_empty() {
                    "Your note is due".to_owned()
                } else {
                    preview
                },
                None,
                None,
                Some(std::time::Duration::from_secs(7)),
            );
        }
        let needs_refresh = self.clock_indicator_enabled || self.battery_indicator_enabled;
        let mut now = SYSTEMTIME::default();
        let mut next_update = if needs_refresh {
            unsafe { GetLocalTime(&mut now) };
            let elapsed_in_minute = u64::from(now.wSecond) * 1000 + u64::from(now.wMilliseconds);
            std::time::Duration::from_millis(60_000_u64.saturating_sub(elapsed_in_minute).max(250))
        } else {
            std::time::Duration::from_secs(24 * 60 * 60)
        };
        if self.battery_indicator_enabled {
            next_update = next_update.min(std::time::Duration::from_secs(15));
        }
        if let Some(at) = self.notebook.reminder_at {
            if at > unix_seconds() {
                next_update = next_update.min(std::time::Duration::from_secs(at - unix_seconds()));
            }
        }
        let clock = self.clock_indicator_enabled.then(|| {
            let hour = match now.wHour % 12 {
                0 => 12,
                value => value,
            };
            format!("{hour}:{:02}", now.wMinute)
        });
        let status = if self
            .notebook
            .reminder_at
            .is_some_and(|at| at <= unix_seconds())
        {
            "Reminder due".to_owned()
        } else {
            self.activity
                .label()
                .or_else(|| {
                    self.media
                        .as_ref()
                        .filter(|media| media.playing)
                        .map(|media| format!("♫ {}", bounded_shell_line(&media.title, 30)))
                })
                .unwrap_or_else(|| {
                    current_status_label_from_clock(clock, self.battery_indicator_enabled)
                })
        };
        if !self.screen_feedback_active() {
            let changed = self.status_label != status;
            if changed {
                self.status_label = status;
            }
            if changed || self.battery_indicator_enabled {
                self.render_frame()?;
                if self.react_compact_visible {
                    self.push_react_state();
                }
            }
        }
        self.clock_refresh_at = instant_now + next_update;
        Ok(next_update)
    }

    fn render_frame(&mut self) -> Result<(), String> {
        if self.screen_overlay_active {
            return Ok(());
        }
        let render_started = std::time::Instant::now();
        let body_width = self
            .width_motion
            .value()
            .clamp(self.folded_target_width(), self.expanded_body_width);
        // Both edges move together. Alternating odd/even HWND widths shifts
        // the integer-positioned window's center by half a pixel each frame.
        let body_width = f64::from(centered_pixel_width(
            body_width * self.scale,
            self.screen_width,
        )) / self.scale;
        let body_height = self
            .height_motion
            .value()
            .clamp(f64::from(FOLDED_HEIGHT), MAX_EXPANDED_BODY_HEIGHT);
        let Some(layout) =
            ShellLayout::with_body_height(self.shape, body_width, body_height, self.scale)
        else {
            return Err("invalid animated shell geometry".into());
        };
        let (width, height) = layout.physical_size();
        self.width = i32::try_from(width).map_err(|_| "animated shell width exceeds limits")?;
        self.height = i32::try_from(height).map_err(|_| "animated shell height exceeds limits")?;
        if self.width > self.max_width || self.height > self.max_height {
            return Err("animated shell exceeded its bounded backing surface".into());
        }
        self.position.x = centered_pixel_origin(self.screen_width, self.width);
        // Move the entire silhouette beyond the screen edge: leaving a few
        // pixels exposed produced a bright seam before the next reveal.
        self.position.y =
            -((self.height + 2) as f64 * self.tuck_motion.value().clamp(0.0, 1.0)).round() as i32;
        self.layout = layout;
        let width = width as usize;
        let height = height as usize;
        self.alpha.resize(width * height, 0);
        if !layout.fill_alpha_plane(&mut self.alpha, ANIMATION_COVERAGE_SAMPLES) {
            return Err("could not fill the bounded shell alpha plane".into());
        }
        // Keep the contour in sync with the active theme in both compact and
        // expanded states; React's compact palette uses the same cream tokens.
        let light_mix = if self.dark_theme { 0.0 } else { 1.0 };
        let (red, green, blue) = (
            (f64::from(LIGHT_SHELL_BACKGROUND.0) * light_mix).round() as u8,
            (f64::from(LIGHT_SHELL_BACKGROUND.1) * light_mix).round() as u8,
            (f64::from(LIGHT_SHELL_BACKGROUND.2) * light_mix).round() as u8,
        );

        let max_stride = self.max_width as usize * 4;
        let pixels = unsafe {
            std::slice::from_raw_parts_mut(self.pixels, max_stride * self.max_height as usize)
        };
        for y in 0..height {
            let row = &mut pixels[y * max_stride..y * max_stride + width * 4];
            for (pixel, alpha) in row
                .chunks_exact_mut(4)
                .zip(&self.alpha[y * width..(y + 1) * width])
            {
                pixel[0] = blue;
                pixel[1] = green;
                pixel[2] = red;
                pixel[3] = *alpha;
            }
        }

        let body_height = f64::from(FOLDED_HEIGHT).max(body_height);
        let body_top = f64::from(self.shape.top_gap()) * self.scale;
        let shelf_drop_target = (self.callbacks.shelf_drag_active.load(Ordering::Acquire)
            || self.callbacks.shelf_drag_hover_hint.load(Ordering::Acquire))
            && !self.fullscreen_hidden
            && !self.react_visible
            && self.reducer.snapshot().mode == ShellMode::Resting;
        let peek_progress = ((body_height - f64::from(FOLDED_HEIGHT))
            / (SHELF_PEEK_SIZE.1 - f64::from(FOLDED_HEIGHT)))
        .clamp(0.0, 1.0);
        if shelf_drop_target {
            let opacity = peek_progress * peek_progress * (3.0 - 2.0 * peek_progress);
            let scale = self.scale;
            let center_x = (self.layout.logical_width() * 0.5 * scale).round() as i32;
            let card_width = (158.0 * scale).round() as i32;
            let card_height = (58.0 * scale).round() as i32;
            let gap = (4.0 * scale).round() as i32;
            let center_y = body_top
                + ((body_height.min(SHELF_PEEK_SIZE.1) * scale - f64::from(card_height)) / 2.0)
                    .max(0.0);
            let press = self.drop_motion.value();
            let convert_y = if self.reduced_motion {
                0.0
            } else {
                ((-press).max(0.0) * 5.0 - press.max(0.0) * 2.0) * scale
            };
            let shelf_y = if self.reduced_motion {
                0.0
            } else {
                (press.max(0.0) * 5.0 - (-press).max(0.0) * 2.0) * scale
            };
            let intent = self.callbacks.drop_intent.load(Ordering::Acquire);
            let convert_enabled = self.callbacks.drop_convertible.load(Ordering::Acquire);
            let shelf_enabled = self.modules.shelf;
            let left_x = center_x - gap / 2 - card_width;
            let right_x = center_x + gap / 2;
            // One quiet well makes the two halves read as drop destinations,
            // rather than as a pair of small buttons floating in the notch.
            self.fill_native_round_rect(
                left_x - (4.0 * scale).round() as i32,
                center_y.round() as i32 - (4.0 * scale).round() as i32,
                card_width * 2 + gap + (8.0 * scale).round() as i32,
                card_height + (8.0 * scale).round() as i32,
                (32.0 * scale).round() as i32,
                if self.dark_theme {
                    (24, 25, 29)
                } else {
                    (236, 238, 243)
                },
                opacity,
            );
            self.draw_native_drop_target(
                left_x,
                (center_y + convert_y).round() as i32,
                card_width,
                card_height,
                "Convert",
                if convert_enabled {
                    "Resize · format"
                } else {
                    "Files only"
                },
                intent == 1,
                convert_enabled,
                opacity,
            );
            self.draw_native_drop_target(
                right_x,
                (center_y + shelf_y).round() as i32,
                card_width,
                card_height,
                "Shelf",
                "Save for later",
                intent == 2,
                shelf_enabled,
                opacity,
            );
        }
        // Apply the user's folded/open transparency to the same alpha plane
        // that Windows uses for hit testing. Fade continuously with height so
        // crossing the two settings never flashes during the spring.
        let expanded = self.expanded_layout_height();
        let progress = ((body_height - f64::from(FOLDED_HEIGHT))
            / (expanded - f64::from(FOLDED_HEIGHT)))
        .clamp(0.0, 1.0);
        let transparency = f64::from(self.folded_transparency_percent) * (1.0 - progress)
            + f64::from(self.open_transparency_percent) * progress;
        let opacity = ((100.0 - transparency) * 255.0 / 100.0).round() as u16;
        // GDI can touch the reserved alpha byte only where shell text and the
        // open card were drawn. Restore those interior regions, then make the
        // entire layered DIB premultiplied, including fully transparent pixels.
        // Leaving light RGB in alpha-zero corners exposes a rectangular host.
        if opacity == 255 {
            if shelf_drop_target {
                // GDI FillRgn/TextOut overwrite the DIB's alpha byte. The
                // normal text repair starts below y=40, while the drop well
                // begins above that; restoring this whole area prevents torn
                // and dark corners on the layered window.
                let drop_top = (body_top + 4.0 * self.scale).floor().max(0.0) as usize;
                let drop_bottom = (body_top + SHELF_PEEK_SIZE.1 * self.scale).ceil() as usize;
                restore_gdi_alpha_rect(
                    pixels,
                    &self.alpha,
                    max_stride,
                    width,
                    (0, width, drop_top, drop_bottom),
                );
            }
            let body_top = (f64::from(self.shape.top_gap()) * self.scale).round() as usize;
            let center_x = width / 2;
            let folded_text_half_width =
                (self.folded_target_width() * self.scale / 2.0).ceil() as usize;
            restore_opaque_alpha_rect(
                pixels,
                &self.alpha,
                max_stride,
                width,
                (
                    center_x.saturating_sub(folded_text_half_width),
                    center_x.saturating_add(folded_text_half_width).min(width),
                    body_top,
                    (body_top as f64 + body_height * self.scale).ceil() as usize,
                ),
            );
            if body_height > 40.0 {
                let inset = (f64::from(self.shape.body_inset()) + 18.0) * self.scale;
                let logical_width = self.layout.logical_width();
                let x_start = (inset.floor() as usize).min(width);
                let x_end = ((logical_width - f64::from(self.shape.body_inset()) - 18.0)
                    * self.scale)
                    .ceil()
                    .clamp(0.0, width as f64) as usize;
                restore_opaque_alpha_rect(
                    pixels,
                    &self.alpha,
                    max_stride,
                    width,
                    (
                        x_start,
                        x_end,
                        (body_top as f64 + 40.0 * self.scale).floor() as usize,
                        (body_top as f64 + (body_height - 11.0) * self.scale).ceil() as usize,
                    ),
                );
            }
        }
        for y in 0..height {
            let row = &mut pixels[y * max_stride..y * max_stride + width * 4];
            for (pixel, alpha) in row
                .chunks_exact_mut(4)
                .zip(&mut self.alpha[y * width..(y + 1) * width])
            {
                if opacity != 255 || *alpha != 255 {
                    composite_shell_pixel(pixel, alpha, opacity);
                }
            }
        }

        let destination = self.position;
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = windows_sys::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        if unsafe {
            UpdateLayeredWindow(
                self.window,
                null_mut(),
                &destination,
                &size,
                self.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            )
        } == 0
        {
            return Err(last_error("could not update the shell surface"));
        }
        self.sync_drop_catcher();
        self.render_cost_estimate =
            smoothed_render_cost(self.render_cost_estimate, render_started.elapsed());
        self.frame_interval =
            animation_frame_interval(self.refresh_period, self.render_cost_estimate);
        self.sync_react_view(body_width, body_height);
        Ok(())
    }

    fn measure_text(&self, value: &str) -> Result<SIZE, String> {
        let mut text_size = SIZE::default();
        let text: Vec<u16> = value.encode_utf16().collect();
        let measured = unsafe {
            GetTextExtentPoint32W(
                self.dc,
                text.as_ptr(),
                i32::try_from(text.len()).unwrap_or(i32::MAX),
                &mut text_size,
            )
        };
        if measured == 0 {
            return Err(last_error("could not measure the shell status text"));
        }
        Ok(text_size)
    }

    fn blend_status_color(&self, foreground: (u8, u8, u8), opacity: f64) -> u32 {
        let background = if self.dark_theme {
            (0, 0, 0)
        } else {
            LIGHT_SHELL_BACKGROUND
        };
        let blend = |front: u8, back: u8| {
            (f64::from(back) + (f64::from(front) - f64::from(back)) * opacity)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        color_ref(
            blend(foreground.0, background.0),
            blend(foreground.1, background.1),
            blend(foreground.2, background.2),
        )
    }

    fn native_drop_accent(&self) -> ((u8, u8, u8), (u8, u8, u8)) {
        match self.accent_gradient {
            crate::storage::StoredAccentGradient::Lime => ((190, 239, 133), (27, 39, 15)),
            crate::storage::StoredAccentGradient::Ice => ((154, 226, 250), (12, 39, 52)),
            crate::storage::StoredAccentGradient::Rose => ((255, 193, 207), (52, 25, 34)),
            crate::storage::StoredAccentGradient::Violet => ((207, 183, 255), (40, 27, 63)),
        }
    }

    fn fill_native_round_rect(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        radius: i32,
        color: (u8, u8, u8),
        opacity: f64,
    ) {
        if width <= 0 || height <= 0 || opacity <= 0.0 {
            return;
        }
        let color = self.blend_status_color(color, opacity);
        let (region, brush): (HRGN, HBRUSH) = unsafe {
            (
                CreateRoundRectRgn(x, y, x + width, y + height, radius, radius),
                CreateSolidBrush(color),
            )
        };
        if region.is_null() || brush.is_null() {
            unsafe {
                if !region.is_null() {
                    DeleteObject(region as HGDIOBJ);
                }
                if !brush.is_null() {
                    DeleteObject(brush as HGDIOBJ);
                }
            }
            return;
        }
        unsafe {
            FillRgn(self.dc, region, brush);
            DeleteObject(brush as HGDIOBJ);
            DeleteObject(region as HGDIOBJ);
        }
    }

    fn draw_native_drop_target(
        &self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        title: &str,
        detail: &str,
        selected: bool,
        enabled: bool,
        opacity: f64,
    ) {
        let disabled_opacity = if enabled { 1.0 } else { 0.46 };
        let opacity = opacity * disabled_opacity;
        let (accent, _) = self.native_drop_accent();
        if selected {
            self.fill_native_round_rect(
                x,
                y,
                width,
                height,
                (26.0 * self.scale).round() as i32,
                if self.dark_theme {
                    (37, 43, 47)
                } else {
                    (218, 230, 236)
                },
                opacity,
            );
        }
        let title_color = if !enabled {
            (130, 133, 140)
        } else if selected {
            accent
        } else if self.dark_theme {
            (246, 247, 250)
        } else {
            (35, 39, 48)
        };
        let detail_color = if self.dark_theme {
            (158, 162, 171)
        } else {
            (96, 101, 111)
        };
        let center_x = x + width / 2;
        let _ = self.draw_text_color_opacity(
            title,
            center_x,
            y + (10.0 * self.scale).round() as i32,
            TA_CENTER,
            title_color,
            opacity,
        );
        let _ = self.draw_text_color_opacity(
            detail,
            center_x,
            y + (32.0 * self.scale).round() as i32,
            TA_CENTER,
            detail_color,
            opacity * 0.84,
        );
    }

    fn draw_text_color_opacity(
        &self,
        value: &str,
        x: i32,
        y: i32,
        alignment: u32,
        foreground: (u8, u8, u8),
        opacity: f64,
    ) -> Result<(), String> {
        let mut text: Vec<u16> = value.encode_utf16().collect();
        unsafe {
            SetBkMode(self.dc, TRANSPARENT as i32);
            SetTextAlign(self.dc, alignment | TA_TOP);
            SetTextColor(self.dc, self.blend_status_color(foreground, opacity));
            if TextOutW(
                self.dc,
                x,
                y.max(0),
                text.as_mut_ptr(),
                i32::try_from(text.len()).unwrap_or(i32::MAX),
            ) == 0
            {
                return Err(last_error("could not paint the shell clock text"));
            }
        }
        Ok(())
    }
}

impl Drop for EdgeShell {
    fn drop(&mut self) {
        if let Some(stop) = self.screen_recording_stop.take() {
            let _ = stop.send(());
        }
        if !self.window.is_null() {
            unsafe {
                SetWindowDisplayAffinity(self.window, 0);
            }
        }
        self.drag_hover_stop.store(true, Ordering::Release);
        if let Some(worker) = self.drag_hover_worker.take() {
            let _ = worker.join();
        }
        if self.raw_input_registered {
            let mouse_device = RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x02,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: null_mut(),
            };
            unsafe {
                RegisterRawInputDevices(&mouse_device, 1, size_of::<RAWINPUTDEVICE>() as u32)
            };
        }
        self.save_note();
        if !self.drop_catcher.is_null() {
            let catcher = std::mem::replace(&mut self.drop_catcher, null_mut());
            if self.drop_target.is_some() {
                let _ = unsafe {
                    windows_startup::Win32::System::Ole::RevokeDragDrop(
                        windows_startup::Win32::Foundation::HWND(catcher as _),
                    )
                };
            }
            unsafe { DestroyWindow(catcher) };
        }
        for (child, _) in self.child_drop_targets.drain(..).rev() {
            if unsafe { IsWindow(child) } != 0 {
                let _ = unsafe {
                    windows_startup::Win32::System::Ole::RevokeDragDrop(
                        windows_startup::Win32::Foundation::HWND(child as _),
                    )
                };
            }
        }
        if self.drop_target.is_some() {
            let _ = unsafe {
                windows_startup::Win32::System::Ole::RevokeDragDrop(
                    windows_startup::Win32::Foundation::HWND(self.window as _),
                )
            };
            self.drop_target.take();
        }
        self.react_view.take();
        self.callbacks.owner.store(null_mut(), Ordering::Release);
        self.callbacks.window.store(null_mut(), Ordering::Release);
        unsafe {
            if !self.frame_timer.is_null() {
                CancelWaitableTimer(self.frame_timer);
                CloseHandle(self.frame_timer);
                self.frame_timer = null_mut();
            }
            KillTimer(self.window, FOCUS_RESTORE_TIMER_ID);
            KillTimer(self.window, HOLD_SPACE_TIMER_ID);
            KillTimer(self.window, NOTE_SAVE_TIMER_ID);
            KillTimer(self.window, WEATHER_POLL_TIMER_ID);
            if self.clipboard_listener_registered {
                windows_sys::Win32::System::DataExchange::RemoveClipboardFormatListener(
                    self.window,
                );
            }
            unregister_shortcuts(self.window, &self.shortcut_registered);
            DestroyWindow(self.window);
        }
        destroy_surface(
            self.dc,
            self.bitmap,
            self.previous_bitmap,
            self.font,
            self.previous_font,
        );
    }
}

unsafe extern "system" fn drop_catcher_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe {
            &*(lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
        };
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    if message == WM_NCHITTEST {
        return HTTRANSPARENT as LRESULT;
    }
    if message == WM_DROPFILES {
        trace_drop(format_args!("catcher WM_DROPFILES"));
        let shell = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as HWND };
        if shell.is_null() || unsafe { PostMessageW(shell, WM_DROPFILES, wparam, lparam) } == 0 {
            unsafe { DragFinish(wparam as _) };
        }
        return 0;
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

unsafe extern "system" fn shell_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: WM_NCCREATE supplies the CREATESTRUCTW whose lpCreateParams is
        // the stable callback state shared through Arc by controller and shell.
        let create = unsafe {
            &*(lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
        };
        unsafe { SetLastError(0) };
        let previous =
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
        if previous == 0 && unsafe { GetLastError() } != 0 {
            return 0;
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let callbacks = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut ShellCallbacks };
    if callbacks.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_INPUT {
        let mut input = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        let read = unsafe {
            GetRawInputData(
                lparam as _,
                RID_INPUT,
                &mut input as *mut RAWINPUT as *mut c_void,
                &mut size,
                size_of::<windows_sys::Win32::UI::Input::RAWINPUTHEADER>() as u32,
            )
        };
        if read != u32::MAX && input.header.dwType == RIM_TYPEMOUSE {
            let flags = unsafe { input.data.mouse.Anonymous.Anonymous.usButtonFlags };
            let callbacks = unsafe { &*callbacks };
            if u32::from(flags) & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                let mut point = POINT::default();
                let under = if unsafe { GetCursorPos(&mut point) } != 0 {
                    unsafe { WindowFromPoint(point) }
                } else {
                    null_mut()
                };
                let external = under != window
                    && unsafe { IsChild(window, under) } == 0
                    && explorer_owns_window(under);
                callbacks
                    .raw_external_press
                    .store(external, Ordering::Release);
                callbacks.raw_mouse_down.store(true, Ordering::Release);
                callbacks.raw_press_x.store(point.x, Ordering::Release);
                callbacks.raw_press_y.store(point.y, Ordering::Release);
                trace_drop(format_args!(
                    "raw mouse down external={external} x={} y={}",
                    point.x, point.y
                ));
                if !under.is_null() && under != window && unsafe { IsChild(window, under) } == 0 {
                    let owner = callbacks.owner.load(Ordering::Acquire);
                    if !owner.is_null() {
                        // Raw input reaches this HWND even when another app receives the click.
                        unsafe { (&mut *owner).handle_outside_click() };
                        post_shell_tick(window, callbacks);
                    }
                }
            }
            if u32::from(flags) & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                callbacks.raw_mouse_down.store(false, Ordering::Release);
                callbacks.raw_external_press.store(false, Ordering::Release);
                trace_drop(format_args!("raw mouse up"));
            }
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_MOUSEACTIVATE {
        return MA_NOACTIVATE;
    }
    if message == WM_DROPFILES {
        trace_drop(format_args!("shell WM_DROPFILES"));
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).accept_dropped_files(wparam as _) };
        }
        return 0;
    }
    if message == WM_CLIPBOARDUPDATE {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).capture_clipboard_update() };
        }
        return 0;
    }
    if message == WM_APP_REACT {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).drain_react_messages() };
        }
        return 0;
    }
    if message == WM_APP_SCREEN_OVERLAY_DONE {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).finish_screen_overlay() };
        }
        return 0;
    }
    if message == WM_LBUTTONUP {
        // SAFETY: callbacks remains in its Arc until the owned HWND is destroyed.
        let callbacks = unsafe { &*callbacks };
        let owner = callbacks.owner.load(Ordering::Acquire);
        if owner.is_null() {
            return 0;
        }
        // SAFETY: click messages execute on the native shell owner thread.
        unsafe { (&mut *owner).handle_native_click() };
        post_shell_tick(window, callbacks);
        return 0;
    }
    if message == WM_HOTKEY {
        let callbacks = unsafe { &*callbacks };
        let index = (wparam as i32).checked_sub(HOTKEY_ID_BASE);
        let Some(action) = index.and_then(|index| ShortcutAction::ALL.get(index as usize).copied())
        else {
            return 0;
        };
        if action == ShortcutAction::ToggleIsland {
            // Preserve rapid repeated toggles if the UI hasn't drained one.
            callbacks.toggle_requested.fetch_add(1, Ordering::Release);
        } else {
            callbacks.shortcut_requested[action.index()].store(true, Ordering::Release);
            let desk_window = callbacks.desk_window.load(Ordering::Acquire);
            if !desk_window.is_null() {
                // Waking a hidden eframe viewport ensures its UI loop can
                // consume the queued shortcut request.
                unsafe { ShowWindowAsync(desk_window, SW_RESTORE) };
            }
        }
        callbacks.context.request_repaint();
        let owner = callbacks.owner.load(Ordering::Acquire);
        if action == ShortcutAction::ToggleIsland && !owner.is_null() {
            // SAFETY: this WndProc runs on the shell's dedicated owner thread;
            // Drop clears owner before destroying the HWND.
            unsafe {
                (&mut *owner).drain_commands();
                (&mut *owner).toggle_from_hotkey();
            }
            post_shell_tick(window, callbacks);
        }
        return 0;
    }
    if message == WM_MOUSEMOVE {
        // Layered-window hit testing ensures WM_MOUSEMOVE is delivered only for
        // a nonzero-alpha point. Track leave once, without activating the HWND.
        let callbacks = unsafe { &*callbacks };
        if !callbacks.pointer_inside.swap(true, Ordering::AcqRel) {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            let leave_tracking_available = unsafe { TrackMouseEvent(&mut tracking) } != 0;
            callbacks
                .pointer_poll_needed
                .store(!leave_tracking_available, Ordering::Release);
            post_shell_tick(window, callbacks);
        }
        return 0;
    }
    if message == WM_MOUSELEAVE {
        // SAFETY: callbacks remains boxed until the owned HWND is destroyed.
        let callbacks = unsafe { &*callbacks };
        // Moving from the layered parent onto its WebView2 child also sends
        // WM_MOUSELEAVE. Resolve the HWND under the pointer on the next tick.
        callbacks.pointer_poll_needed.store(true, Ordering::Release);
        post_shell_tick(window, callbacks);
        return 0;
    }
    if message == WM_APP_SHELL_TICK {
        unsafe { &*callbacks }
            .tick_queued
            .store(false, Ordering::Release);
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            // SAFETY: the queued message is dispatched on the shell thread.
            if unsafe { (&mut *owner).run_native_tick() }.is_err() {
                let callbacks = unsafe { &*callbacks };
                callbacks.animation_failed.store(true, Ordering::Release);
                callbacks.context.request_repaint();
            }
        }
        return 0;
    }
    if message == WM_APP_SHELL_COMMAND {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            // SAFETY: command messages execute serially on the shell owner thread.
            unsafe { (&mut *owner).drain_commands() };
        }
        return 0;
    }
    if message == WM_POWERBROADCAST {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            let event = match wparam {
                PBT_APMSUSPEND => Some(ShellEvent::PowerSuspended),
                PBT_APMRESUMECRITICAL | PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => {
                    Some(ShellEvent::PowerResumed)
                }
                _ => None,
            };
            if let Some(event) = event {
                // SAFETY: power notifications are dispatched on the shell's owner thread.
                let owner = unsafe { &mut *owner };
                owner.reducer.dispatch(event, std::time::Instant::now());
                owner.update_suppression_visibility();
                owner.request_tick();
                return 1;
            }
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if message == WM_DISPLAYCHANGE {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            // SAFETY: display notifications are dispatched on the shell owner thread.
            if unsafe { (&mut *owner).refresh_display(None) }.is_err() {
                let callbacks = unsafe { &*callbacks };
                callbacks.animation_failed.store(true, Ordering::Release);
                callbacks.context.request_repaint();
            }
        }
        return 0;
    }
    if message == WM_DPICHANGED {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            // The high word is the vertical DPI; desktop displays use a square
            // pixel grid. Keep the pill centered at the top of the primary display.
            let dpi = (wparam as u32) & 0xffff;
            if unsafe { (&mut *owner).refresh_display(Some(dpi)) }.is_err() {
                let callbacks = unsafe { &*callbacks };
                callbacks.animation_failed.store(true, Ordering::Release);
                callbacks.context.request_repaint();
            }
        }
        return 0;
    }
    if message == WM_APP_SHELL_STOP {
        unsafe { PostQuitMessage(0) };
        return 0;
    }
    if message == WM_TIMER && wparam == FOCUS_RESTORE_TIMER_ID {
        unsafe { KillTimer(window, FOCUS_RESTORE_TIMER_ID) };
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            // SAFETY: this timer is dispatched on the shell owner's UI thread.
            unsafe { (&mut *owner).restore_foreground() };
        }
        return 0;
    }
    if message == WM_TIMER && wparam == HOLD_SPACE_TIMER_ID {
        if unsafe { GetAsyncKeyState(0x20) } >= 0 {
            unsafe {
                KillTimer(window, HOLD_SPACE_TIMER_ID);
            }
            let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
            if !owner.is_null() {
                if let Some(view) = unsafe { &(*owner).react_view } {
                    let _ = view.evaluate_script("window.islandHoldReleased = true; window.dispatchEvent(new Event('island:hold-release'));");
                }
            }
        }
        return 0;
    }
    if message == WM_TIMER && wparam == NOTE_SAVE_TIMER_ID {
        unsafe { KillTimer(window, NOTE_SAVE_TIMER_ID) };
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).save_note() };
        }
        return 0;
    }
    if message == WM_TIMER && wparam == WEATHER_POLL_TIMER_ID {
        let owner = unsafe { &*callbacks }.owner.load(Ordering::Acquire);
        if !owner.is_null() {
            unsafe { (&mut *owner).poll_weather() };
        }
        return 0;
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn post_shell_tick(window: HWND, callbacks: &ShellCallbacks) {
    if callbacks.tick_queued.swap(true, Ordering::AcqRel) {
        return;
    }
    // SAFETY: PostMessageW queues work for this HWND; it does not invoke
    // WndProc synchronously.
    if unsafe { PostMessageW(window, WM_APP_SHELL_TICK, 0, 0) } == 0 {
        callbacks.tick_queued.store(false, Ordering::Release);
        callbacks.animation_failed.store(true, Ordering::Release);
        callbacks.context.request_repaint();
    }
}

/// Explorer's legacy WM_DROPFILES route accepts drops on the layered notch,
/// but gives no drag-enter event. Observe only drags that began outside Island
/// so the native spring can offer a target before the file is released.
fn explorer_owns_window(window: HWND) -> bool {
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    if window.is_null() {
        return false;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return false;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return false;
    }
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let readable =
        unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    readable
        && String::from_utf16_lossy(&path[..length as usize])
            .rsplit(['\\', '/'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}

fn monitor_external_drag_hover(callbacks: Arc<ShellCallbacks>, stop: Arc<AtomicBool>) {
    let mut last_trace_state = (false, false, false);
    while !stop.load(Ordering::Acquire) {
        let pressed = callbacks.raw_mouse_down.load(Ordering::Acquire)
            && unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } < 0;
        if !pressed {
            callbacks.raw_mouse_down.store(false, Ordering::Release);
            callbacks.raw_external_press.store(false, Ordering::Release);
        }
        let external_press = callbacks.raw_external_press.load(Ordering::Acquire);
        let window = callbacks.window.load(Ordering::Acquire);
        let mut point = POINT::default();
        let under = if unsafe { GetCursorPos(&mut point) } != 0 {
            unsafe { WindowFromPoint(point) }
        } else {
            null_mut()
        };
        let over_island = !window.is_null()
            && !under.is_null()
            && (under == window || unsafe { IsChild(window, under) } != 0);
        let trace_state = (pressed, external_press, over_island);
        if trace_state != last_trace_state {
            trace_drop(format_args!(
                "pointer pressed={pressed} external={external_press} over_island={over_island} x={} y={}",
                point.x, point.y
            ));
            last_trace_state = trace_state;
        }

        let moved = (point.x - callbacks.raw_press_x.load(Ordering::Acquire)).abs() > 8
            || (point.y - callbacks.raw_press_y.load(Ordering::Acquire)).abs() > 8;
        let ole_drag_active = callbacks.shelf_drag_active.load(Ordering::Acquire);
        let hovering = pressed
            && external_press
            && over_island
            && moved
            && !callbacks.is_open.load(Ordering::Acquire)
            && !ole_drag_active;
        let was_hovering = callbacks
            .shelf_drag_hover_hint
            .swap(hovering, Ordering::AcqRel);
        if hovering {
            // WM_DROPFILES has no DragEnter/DragOver callbacks. Explorer's
            // legacy route still gets live left/right target feedback here;
            // the actual file list is validated again on release.
            callbacks.drop_convertible.store(true, Ordering::Release);
            resolve_drop_intent(&callbacks, point.x, point.y, false);
        } else if was_hovering && !ole_drag_active {
            callbacks.drop_convertible.store(false, Ordering::Release);
            if callbacks.drop_intent.swap(0, Ordering::AcqRel) != 0 {
                post_shell_tick(window, &callbacks);
            }
        }
        if was_hovering != hovering {
            trace_drop(format_args!(
                "external drag hover {}",
                if hovering { "entered" } else { "left" }
            ));
            post_shell_tick(window, &callbacks);
        }
        thread::sleep(DRAG_HOVER_POLL);
    }
}

fn composition_refresh_period() -> std::time::Duration {
    let mut timing = DWM_TIMING_INFO {
        cbSize: size_of::<DWM_TIMING_INFO>() as u32,
        ..DWM_TIMING_INFO::default()
    };
    // DWM requires a null HWND on Windows 8.1 and newer. If composition timing
    // is unavailable (for example, in a remote session), retain a 60 Hz cadence.
    if unsafe { DwmGetCompositionTimingInfo(null_mut(), &mut timing) } < 0 {
        return FALLBACK_FRAME_INTERVAL;
    }

    // These fields are packed in the Windows ABI structure. Copy the ratio by
    // value before reading it so Rust does not create an unaligned reference.
    let refresh_rate = timing.rateRefresh;
    refresh_period_for_rate(refresh_rate.uiNumerator, refresh_rate.uiDenominator)
}

fn refresh_period_for_rate(numerator: u32, denominator: u32) -> std::time::Duration {
    if numerator == 0 || denominator == 0 {
        return FALLBACK_FRAME_INTERVAL;
    }

    let interval_ns =
        (1_000_000_000_u128 * u128::from(denominator)).div_ceil(u128::from(numerator));
    if !(1_000_000..=100_000_000).contains(&interval_ns) {
        return FALLBACK_FRAME_INTERVAL;
    }
    std::time::Duration::from_nanos(interval_ns as u64)
}

fn animation_frame_interval(
    refresh_period: std::time::Duration,
    render_cost: std::time::Duration,
) -> std::time::Duration {
    let refresh_ns = refresh_period.as_nanos().max(1);
    // Present on a whole number of display refreshes. Leave headroom for the
    // CPU DIB upload; a fixed 60 Hz timer judders on 75/90/120/144 Hz screens.
    let cost_ns = render_cost.as_nanos();
    let required_ns = cost_ns.saturating_add((cost_ns / 8).max(1_000_000));
    let periods = required_ns.div_ceil(refresh_ns).max(1);
    std::time::Duration::from_nanos(
        refresh_ns.saturating_mul(periods).min(u128::from(u64::MAX)) as u64
    )
}

fn pointer_poll_interval(refresh_period: std::time::Duration) -> std::time::Duration {
    // Resolve transitions between the layered HWND and its WebView child at
    // display cadence, while bounding background cursor polling on very high
    // refresh-rate panels.
    refresh_period.clamp(
        std::time::Duration::from_millis(8),
        std::time::Duration::from_millis(16),
    )
}

fn smoothed_render_cost(
    previous: std::time::Duration,
    sample: std::time::Duration,
) -> std::time::Duration {
    // A one-off compositor or WebView stall must not lower every subsequent
    // spring frame to a slower display cadence. Sustained cost still catches
    // up exponentially over a few frames.
    let sample = sample.min(
        previous
            .saturating_mul(3)
            .max(std::time::Duration::from_millis(1)),
    );
    if sample >= previous {
        return sample;
    }
    let nanos = (previous.as_nanos() * 7 + sample.as_nanos()) / 8;
    std::time::Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

#[inline]
fn composite_shell_pixel(pixel: &mut [u8], coverage: &mut u8, opacity: u16) {
    let source_alpha = *coverage;
    if opacity == 255 && source_alpha == 255 {
        pixel[3] = 255;
        return;
    }

    let final_alpha = (u16::from(source_alpha) * opacity + 127) / 255;
    for channel in &mut pixel[..3] {
        *channel = ((u16::from(*channel) * final_alpha + 127) / 255) as u8;
    }
    *coverage = final_alpha as u8;
    pixel[3] = *coverage;
}

fn restore_gdi_alpha_rect(
    pixels: &mut [u8],
    source_alpha: &[u8],
    stride: usize,
    width: usize,
    bounds: (usize, usize, usize, usize),
) {
    if width == 0 {
        return;
    }
    let (x_start, x_end, y_start, y_end) = bounds;
    for y in y_start.min(source_alpha.len() / width)..y_end.min(source_alpha.len() / width) {
        for x in x_start.min(width)..x_end.min(width) {
            pixels[y * stride + x * 4 + 3] = source_alpha[y * width + x];
        }
    }
}

fn restore_opaque_alpha_rect(
    pixels: &mut [u8],
    source_alpha: &[u8],
    stride: usize,
    width: usize,
    bounds: (usize, usize, usize, usize),
) {
    if width == 0 {
        return;
    }
    let (x_start, x_end, y_start, y_end) = bounds;
    let height = source_alpha.len() / width;
    let x_start = x_start.min(width);
    let x_end = x_end.min(width);
    let y_start = y_start.min(height);
    let y_end = y_end.min(height);
    for y in y_start..y_end {
        for x in x_start..x_end {
            let pixel_offset = y * stride + x * 4;
            let alpha_offset = y * width + x;
            let mut coverage = source_alpha[alpha_offset];
            composite_shell_pixel(
                &mut pixels[pixel_offset..pixel_offset + 4],
                &mut coverage,
                255,
            );
        }
    }
}

fn run_native_message_loop(window: HWND, frame_timer: HANDLE, callbacks: *const ShellCallbacks) {
    let mut message = MSG::default();
    loop {
        let timer_or_message = unsafe {
            MsgWaitForMultipleObjectsEx(1, &frame_timer, u32::MAX, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if timer_or_message == WAIT_OBJECT_0 {
            if !callbacks.is_null() {
                // The raw callback pointer is owned by the controller and stays
                // alive until the stop message joins this thread.
                let callbacks = unsafe { &*callbacks };
                post_shell_tick(window, callbacks);
            }
            continue;
        }
        if timer_or_message == WAIT_FAILED {
            if !callbacks.is_null() {
                // The scheduler is part of the shell's animation path. Surface
                // a wait failure so the app can report it instead of silently
                // leaving the shell frozen.
                let callbacks = unsafe { &*callbacks };
                callbacks.animation_failed.store(true, Ordering::Release);
                callbacks.context.request_repaint();
            }
            break;
        }
        while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_QUIT {
                return;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

type Surface = (HDC, HBITMAP, HGDIOBJ, HGDIOBJ, HGDIOBJ, *mut u8);

fn foreground_covers_primary(shell_window: HWND) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() || foreground == shell_window {
        return false;
    }
    // Clicking empty desktop makes Explorer's desktop HWND the foreground
    // window. Its rectangle covers the screen, but it is not a fullscreen app.
    // Maximized app frames can also extend beyond the monitor via invisible
    // resize borders; do not hide the pill for those ordinary windows.
    let mut class_name = [0u16; 128];
    let length =
        unsafe { GetClassNameW(foreground, class_name.as_mut_ptr(), class_name.len() as i32) };
    let class_name = String::from_utf16_lossy(&class_name[..length.max(0) as usize]);
    if matches!(
        class_name.as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    ) || unsafe { IsZoomed(foreground) } != 0
    {
        return false;
    }
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if width <= 0 || height <= 0 {
        return false;
    }
    let mut bounds = RECT::default();
    if unsafe { GetWindowRect(foreground, &mut bounds) } == 0 {
        return false;
    }
    is_fullscreen_bounds(bounds, width, height) && !is_windows_capture_surface(foreground)
}

fn is_windows_capture_surface(window: HWND) -> bool {
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut process_id);
    }
    if process_id == 0 {
        return false;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return false;
    }

    // Win+Shift+S uses a temporary full-desktop window. Treating that picker
    // as a fullscreen app hides and re-shows the Island around every snip,
    // which needlessly tears down the compact WebView presentation.
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let queried =
        unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
    unsafe {
        CloseHandle(process);
    }
    if !queried || length == 0 {
        return false;
    }
    let image = String::from_utf16_lossy(&path[..(length as usize).min(path.len())]);
    let executable = image.rsplit(['\\', '/']).next().unwrap_or(&image);
    [
        "SnippingTool.exe",
        "ScreenClippingHost.exe",
        "ScreenSketch.exe",
        "ScreenCaptureHost.exe",
    ]
    .iter()
    .any(|name| executable.eq_ignore_ascii_case(name))
}

fn is_fullscreen_bounds(bounds: RECT, screen_width: i32, screen_height: i32) -> bool {
    screen_width > 0
        && screen_height > 0
        && bounds.left <= 2
        && bounds.top <= 2
        && bounds.right >= screen_width - 2
        && bounds.bottom >= screen_height - 2
}

fn shell_surface_metrics(dpi: u32) -> Option<(f64, i32, i32)> {
    if !(48..=960).contains(&dpi) {
        return None;
    }
    let scale = f64::from(dpi) / 96.0;
    let layout = ShellLayout::with_body_height(
        ShapeMode::Capsule,
        ShapeMode::Capsule.expanded_body_width() + PROFILE_PANEL_EXTRA_WIDTH,
        MAX_EXPANDED_BODY_HEIGHT,
        scale,
    )?;
    let (width, height) = layout.physical_size();
    let width = i32::try_from(width).ok()?;
    let height = i32::try_from(height).ok()?;
    let bytes = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(5)?;
    (bytes <= MAX_SHELL_SURFACE_BYTES).then_some((scale, width, height))
}

fn destroy_surface(
    dc: HDC,
    bitmap: HBITMAP,
    previous_bitmap: HGDIOBJ,
    font: HGDIOBJ,
    previous_font: HGDIOBJ,
) {
    // The stock objects are restored before their DIB/font are deleted.
    unsafe {
        SelectObject(dc, previous_font);
        SelectObject(dc, previous_bitmap);
        DeleteObject(font);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(dc);
    }
}

fn create_surface(width: i32, height: i32, scale: f64) -> Result<Surface, String> {
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        return Err(last_error("could not create the shell drawing context"));
    }
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut raw_pixels = null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            dc,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut raw_pixels,
            null_mut(),
            0,
        )
    };
    if bitmap.is_null() || raw_pixels.is_null() {
        unsafe {
            if !bitmap.is_null() {
                DeleteObject(bitmap as HGDIOBJ);
            }
            DeleteDC(dc);
        }
        return Err(last_error("could not allocate the shell pixel surface"));
    }
    let previous_bitmap = unsafe { SelectObject(dc, bitmap as HGDIOBJ) };
    if previous_bitmap.is_null() {
        unsafe {
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(dc);
        }
        return Err(last_error("could not select the shell pixel surface"));
    }
    let font_name = wide("Segoe UI Variable Display");
    let font = unsafe {
        CreateFontW(
            -(12.0 * scale).round().max(1.0) as i32,
            0,
            0,
            0,
            FW_SEMIBOLD as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32 | FF_DONTCARE as u32,
            font_name.as_ptr(),
        )
    };
    if font.is_null() {
        unsafe {
            SelectObject(dc, previous_bitmap);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(dc);
        }
        return Err(last_error("could not create the shell font"));
    }
    let previous_font = unsafe { SelectObject(dc, font as HGDIOBJ) };
    if previous_font.is_null() {
        unsafe {
            DeleteObject(font as HGDIOBJ);
            SelectObject(dc, previous_bitmap);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(dc);
        }
        return Err(last_error("could not select the shell font"));
    }
    Ok((
        dc,
        bitmap,
        previous_bitmap,
        font as HGDIOBJ,
        previous_font,
        raw_pixels.cast::<u8>(),
    ))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn bounded_shell_line(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .take(max_chars)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn read_local_note() -> String {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return String::new();
    };
    let path = std::path::PathBuf::from(local)
        .join("Island")
        .join("note.txt");
    std::fs::read_to_string(path)
        .ok()
        .filter(|note| note.len() <= 256 * 1024)
        .unwrap_or_default()
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn load_notebook() -> NoteBook {
    let saved = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .and_then(|local| std::fs::read(local.join("Island").join("notes.json")).ok())
        .filter(|bytes| bytes.len() <= 4 * 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice::<NoteBook>(&bytes).ok())
        .filter(|book| {
            !book.folders.is_empty()
                && book.folders.len() <= 16
                && book.selected < book.folders.len()
                && book
                    .folders
                    .iter()
                    .all(|folder| folder.name.len() <= 48 && folder.text.len() <= 256 * 1024)
        });
    saved.unwrap_or_else(|| NoteBook {
        folders: vec![NoteFolder {
            name: "Notes".to_owned(),
            text: read_local_note(),
        }],
        selected: 0,
        reminder_at: None,
    })
}

fn weather_city_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("weather_city.txt"))
}

fn enable_shutdown_privilege() -> bool {
    let mut token = null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return false;
    }
    let mut luid = windows_sys::Win32::Foundation::LUID::default();
    let name = wide("SeShutdownPrivilege");
    let found = unsafe { LookupPrivilegeValueW(null(), name.as_ptr(), &mut luid) } != 0;
    let enabled = if found {
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        unsafe {
            SetLastError(0);
            AdjustTokenPrivileges(token, 0, &privileges, 0, null_mut(), null_mut()) != 0
                && GetLastError() != ERROR_NOT_ALL_ASSIGNED
        }
    } else {
        false
    };
    unsafe { CloseHandle(token) };
    enabled
}

fn run_shutdown_command(flag: &str) -> String {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
    let mut directory = [0u16; 32768];
    let count =
        unsafe { GetSystemDirectoryW(directory.as_mut_ptr(), directory.len() as u32) } as usize;
    if count == 0 || count >= directory.len() {
        return "Windows system directory is unavailable.".into();
    }
    let path = std::path::PathBuf::from(String::from_utf16_lossy(&directory[..count]))
        .join("shutdown.exe");
    let args: &[&str] = match flag {
        "/r" => &["/r", "/t", "30"],
        "/s" => &["/s", "/t", "30"],
        "/a" => &["/a"],
        _ => return "Unknown power action.".into(),
    };
    match std::process::Command::new(path)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(status) if status.success() => match flag {
            "/r" => "Restart scheduled in 30 seconds. You can cancel it here.".into(),
            "/s" => "Shut down scheduled in 30 seconds. You can cancel it here.".into(),
            _ => "Scheduled power action cancelled.".into(),
        },
        _ => "Windows could not complete the power action.".into(),
    }
}

fn run_power_action(action: &str) -> String {
    match action {
        "lock" => {
            if unsafe { LockWorkStation() } != 0 {
                "PC locked.".into()
            } else {
                "Windows could not lock the PC.".into()
            }
        }
        "sleep" => {
            if !enable_shutdown_privilege() {
                return "Windows did not grant permission to sleep.".into();
            }
            if unsafe { SetSuspendState(false, false, false) } {
                "PC resumed from sleep.".into()
            } else {
                "Windows could not put the PC to sleep.".into()
            }
        }
        "restart" => run_shutdown_command("/r"),
        "shutdown" => run_shutdown_command("/s"),
        _ => "Unknown power action.".into(),
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct WeatherCache {
    city: String,
    units: WeatherUnits,
    fetched_at: u64,
    lines: Vec<String>,
    data: serde_json::Value,
}

fn weather_cache_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("weather_cache.json"))
}

fn load_weather_cache(city: &str, units: WeatherUnits) -> Option<WeatherCache> {
    let bytes = std::fs::read(weather_cache_path()?).ok()?;
    if bytes.len() > 512 * 1024 {
        return None;
    }
    let cache: WeatherCache = serde_json::from_slice(&bytes).ok()?;
    if !cache.city.eq_ignore_ascii_case(city)
        || cache.units != units
        || unix_seconds().saturating_sub(cache.fetched_at) > 6 * 60 * 60
        || cache.data["timezone"].as_str().is_none_or(str::is_empty)
        || ![
            "time",
            "weather_code",
            "precipitation_probability",
            "precipitation",
        ]
        .iter()
        .all(|key| {
            cache.data["hourly"][key]
                .as_array()
                .is_some_and(|values| !values.is_empty())
        })
    {
        return None;
    }
    Some(cache)
}

fn save_weather_cache(cache: &WeatherCache) {
    let Some(path) = weather_cache_path() else {
        return;
    };
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_ok()
        && let Ok(bytes) = serde_json::to_vec(cache)
        && bytes.len() <= 512 * 1024
    {
        let _ = std::fs::write(path, bytes);
    }
}

fn shelf_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("shelf.json"))
}

const HOME_ACTION_IDS: [&str; 17] = [
    "capture",
    "record",
    "ocr",
    "qr",
    "color",
    "command",
    "dictate",
    "sign",
    "remind",
    "task",
    "clipboard-page",
    "shelf-page",
    "text-page",
    "agents-page",
    "notes-page",
    "weather-page",
    "controls-page",
];

fn default_home_actions() -> Vec<Option<String>> {
    [
        "capture", "record", "ocr", "qr", "color", "command", "dictate", "sign", "remind", "task",
    ]
    .into_iter()
    .map(|action| Some(action.to_owned()))
    .collect()
}

fn home_actions_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("home_actions.json"))
}

fn valid_home_actions(actions: &[Option<String>]) -> bool {
    if actions.len() != 10 {
        return false;
    }
    let mut used = HashSet::new();
    actions
        .iter()
        .flatten()
        .all(|action| HOME_ACTION_IDS.contains(&action.as_str()) && used.insert(action.as_str()))
}

fn load_home_actions() -> Vec<Option<String>> {
    home_actions_path()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| bytes.len() <= 4096)
        .and_then(|bytes| serde_json::from_slice::<Vec<Option<String>>>(&bytes).ok())
        .filter(|actions| valid_home_actions(actions))
        .unwrap_or_else(default_home_actions)
}

fn save_home_actions(actions: &[Option<String>]) -> bool {
    if !valid_home_actions(actions) {
        return false;
    }
    let Some(path) = home_actions_path() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    serde_json::to_vec(actions)
        .ok()
        .is_some_and(|bytes| std::fs::write(path, bytes).is_ok())
}

fn load_shelf_paths() -> Vec<std::path::PathBuf> {
    let Some(path) = shelf_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    if bytes.len() > 256 * 1024 {
        return Vec::new();
    }
    serde_json::from_slice::<Vec<String>>(&bytes)
        .unwrap_or_default()
        .into_iter()
        .take(32)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute() && path.exists())
        .collect()
}

fn shelf_item_kind(path: &std::path::Path) -> &'static str {
    if path.is_dir() {
        return "folder";
    }
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" | "heif"
        | "avif" => "image",
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "m4v" | "mpg" | "mpeg" | "3gp" => "video",
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "wma" | "aif"
        | "aiff" | "m4b" | "ac3" | "amr" | "ape" | "mid" | "midi" | "mka" => "audio",
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" => "archive",
        "rs" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "py" | "js" | "mjs" | "cjs" | "ts"
        | "jsx" | "tsx" | "html" | "css" | "scss" | "json" | "xml" | "yaml" | "yml" | "toml"
        | "sql" | "sh" | "ps1" | "bat" | "cmd" | "go" | "rb" | "php" | "swift" | "kt" => "code",
        "txt" | "md" | "rtf" | "log" | "csv" => "text",
        "pdf" => "pdf",
        "doc" | "docx" | "odt" | "ppt" | "pptx" | "xls" | "xlsx" | "epub" => "document",
        "exe" | "lnk" | "url" | "appref-ms" | "msi" => "app",
        _ => "file",
    }
}

fn shelf_item_visual_data_url(path: &std::path::Path) -> Option<String> {
    match shelf_item_kind(path) {
        "image" | "video" => shell_file_thumbnail_data_url(path),
        "app" => shell_file_icon_data_url(path),
        _ => None,
    }
}

fn shell_file_thumbnail_data_url(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    use windows_startup::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
    };
    use windows_startup::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT,
        SIIGBF_THUMBNAILONLY,
    };

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() {
        return None;
    }
    let thumbnail = (|| {
        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let item = unsafe {
            SHCreateItemFromParsingName::<_, _, IShellItemImageFactory>(
                windows_startup::core::PCWSTR(path_wide.as_ptr()),
                None,
            )
        }
        .ok()?;
        let bitmap = unsafe {
            item.GetImage(
                windows_startup::Win32::Foundation::SIZE { cx: 80, cy: 80 },
                SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT,
            )
        }
        .ok()?;
        let raw_bitmap = bitmap.0 as HBITMAP;
        let result = (|| {
            let mut dimensions = BITMAP::default();
            if unsafe {
                GetObjectW(
                    raw_bitmap as HGDIOBJ,
                    size_of::<BITMAP>() as i32,
                    (&mut dimensions as *mut BITMAP).cast(),
                )
            } == 0
            {
                return None;
            }
            let width = dimensions.bmWidth.unsigned_abs();
            let height = dimensions.bmHeight.unsigned_abs();
            if width == 0 || height == 0 || width > 256 || height > 256 {
                return None;
            }
            let mut bitmap_info = BITMAPINFO::default();
            bitmap_info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            };
            let mut bgra = vec![0u8; (width * height * 4) as usize];
            let dc = unsafe { CreateCompatibleDC(null_mut()) };
            if dc.is_null() {
                return None;
            }
            let lines = unsafe {
                GetDIBits(
                    dc,
                    raw_bitmap,
                    0,
                    height,
                    bgra.as_mut_ptr().cast(),
                    &mut bitmap_info,
                    DIB_RGB_COLORS,
                )
            };
            unsafe { DeleteDC(dc) };
            if lines == 0 {
                return None;
            }
            let has_alpha = bgra.chunks_exact(4).any(|pixel| pixel[3] != 0);
            let mut rgba = Vec::with_capacity(bgra.len());
            for pixel in bgra.chunks_exact(4) {
                rgba.extend_from_slice(&[
                    pixel[2],
                    pixel[1],
                    pixel[0],
                    if has_alpha { pixel[3] } else { 255 },
                ]);
            }
            let mut png_bytes = Vec::new();
            let encoded = (|| {
                let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.set_compression(png::Compression::Fast);
                let mut writer = encoder.write_header().ok()?;
                writer.write_image_data(&rgba).ok()?;
                writer.finish().ok()
            })();
            encoded?;
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png_bytes)
            ))
        })();
        unsafe { DeleteObject(raw_bitmap as HGDIOBJ) };
        result
    })();
    unsafe { CoUninitialize() };
    thumbnail
}

fn shell_file_icon_data_url(path: &std::path::Path) -> Option<String> {
    use base64::Engine;

    let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut file_info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(
            path_wide.as_ptr(),
            0,
            &mut file_info,
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON,
        )
    };
    if found == 0 || file_info.hIcon.is_null() {
        return None;
    }
    let icon = file_info.hIcon;
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        unsafe { DestroyIcon(icon) };
        return None;
    }
    let mut bitmap_info = BITMAPINFO::default();
    bitmap_info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: 32,
        biHeight: -32,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..Default::default()
    };
    let mut pixels = null_mut();
    let bitmap =
        unsafe { CreateDIBSection(dc, &bitmap_info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0) };
    if bitmap.is_null() || pixels.is_null() {
        if !bitmap.is_null() {
            unsafe { DeleteObject(bitmap as HGDIOBJ) };
        }
        unsafe {
            DeleteDC(dc);
            DestroyIcon(icon);
        }
        return None;
    }
    let previous_bitmap = unsafe { SelectObject(dc, bitmap as HGDIOBJ) };
    let encoded = if previous_bitmap.is_null()
        || unsafe { DrawIconEx(dc, 0, 0, icon, 32, 32, 0, null_mut(), DI_NORMAL) } == 0
    {
        None
    } else {
        let bgra = unsafe { std::slice::from_raw_parts(pixels.cast::<u8>(), 32 * 32 * 4) };
        let mut rgba = Vec::with_capacity(bgra.len());
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        let mut png_bytes = Vec::new();
        let encoded = (|| {
            let mut encoder = png::Encoder::new(&mut png_bytes, 32, 32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&rgba).ok()?;
            writer.finish().ok()
        })();
        encoded.map(|()| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png_bytes)
            )
        })
    };
    if !previous_bitmap.is_null() {
        unsafe { SelectObject(dc, previous_bitmap) };
    }
    unsafe {
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(dc);
        DestroyIcon(icon);
    }
    encoded
}

fn cached_command_icon_data_url(
    cache: &Mutex<HashMap<std::path::PathBuf, Option<String>>>,
    path: &std::path::Path,
) -> Option<String> {
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(icon) = cache.get(path) {
        return icon.clone();
    }
    // At most 48 small 32px Shell icons are retained; search results never
    // cache the entire Start Menu or Windows Search index.
    if cache.len() >= 48 {
        cache.clear();
    }
    let icon = shell_file_icon_data_url(path);
    cache.insert(path.to_path_buf(), icon.clone());
    icon
}

fn save_shelf_paths(paths: &[std::path::PathBuf]) {
    let Some(path) = shelf_path() else { return };
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let entries: Vec<String> = paths
        .iter()
        .take(32)
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    if let Ok(bytes) = serde_json::to_vec(&entries) {
        let _ = std::fs::write(path, bytes);
    }
}

fn clipboard_history_setting_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("clipboard_history_enabled.txt"))
}

fn clipboard_settings_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("clipboard_settings.json"))
}

pub fn load_clipboard_settings() -> ClipboardSettings {
    let mut settings = clipboard_settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| bytes.len() <= 4096)
        .and_then(|bytes| serde_json::from_slice::<ClipboardSettings>(&bytes).ok())
        .unwrap_or_default();
    if !matches!(settings.history_limit, 50 | 200 | 500 | 1000) {
        settings.history_limit = 200;
    }
    settings.capture_enabled = load_clipboard_history_enabled();
    settings
}

pub fn save_clipboard_settings(settings: ClipboardSettings) -> bool {
    if !matches!(settings.history_limit, 50 | 200 | 500 | 1000) {
        return false;
    }
    let Some(path) = clipboard_settings_path() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(bytes) = serde_json::to_vec(&settings) else {
        return false;
    };
    std::fs::create_dir_all(parent).is_ok()
        && std::fs::write(path, bytes).is_ok()
        && save_clipboard_history_enabled(settings.capture_enabled)
}

fn module_settings_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("modules.json"))
}

pub fn load_module_settings() -> ModuleSettings {
    module_settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| bytes.len() <= 4096)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_module_settings(settings: ModuleSettings) -> bool {
    let Some(path) = module_settings_path() else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(bytes) = serde_json::to_vec(&settings) else {
        return false;
    };
    std::fs::create_dir_all(parent).is_ok() && std::fs::write(path, bytes).is_ok()
}

pub fn load_clipboard_history_enabled() -> bool {
    clipboard_history_setting_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|value| value.trim() != "0")
        .unwrap_or(true)
}

pub fn save_clipboard_history_enabled(enabled: bool) -> bool {
    let Some(path) = clipboard_history_setting_path() else {
        return false;
    };
    path.parent()
        .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
        && std::fs::write(path, if enabled { "1" } else { "0" }).is_ok()
}

fn clipboard_source_is_private() -> bool {
    use windows_sys::Win32::System::DataExchange::GetClipboardOwner;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    let owner = unsafe { GetClipboardOwner() };
    if owner.is_null() {
        return true;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(owner, &mut process_id) };
    if process_id == 0 {
        return true;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return true;
    }
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    let readable =
        unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    if !readable {
        return true;
    }
    let name = std::path::Path::new(&String::from_utf16_lossy(&path[..length as usize]))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "keepass.exe"
            | "keepassxc.exe"
            | "1password.exe"
            | "bitwarden.exe"
            | "protonpass.exe"
            | "dashlane.exe"
            | "lastpass.exe"
            | "enpass.exe"
    )
}

pub fn load_weather_city() -> String {
    weather_city_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|city| city.trim().to_owned())
        .filter(|city| !city.is_empty() && city.len() <= 80)
        .unwrap_or_default()
}

pub fn save_weather_city(city: &str) -> bool {
    let city = city.trim();
    if city.is_empty() || city.len() > 80 || city.chars().any(char::is_control) {
        return false;
    }
    let Some(path) = weather_city_path() else {
        return false;
    };
    path.parent()
        .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
        && std::fs::write(path, city).is_ok()
}

fn weather_units_path() -> Option<std::path::PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|local| local.join("Island").join("weather_units.txt"))
}

pub fn load_weather_units() -> WeatherUnits {
    weather_units_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .filter(|value| value.trim() == "imperial")
        .map(|_| WeatherUnits::Imperial)
        .unwrap_or_default()
}

pub fn save_weather_units(units: WeatherUnits) -> bool {
    let Some(path) = weather_units_path() else {
        return false;
    };
    path.parent()
        .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
        && std::fs::write(
            path,
            if units == WeatherUnits::Imperial {
                "imperial"
            } else {
                "metric"
            },
        )
        .is_ok()
}

fn read_clipboard_text() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    if unsafe { OpenClipboard(null_mut()) } == 0 {
        return None;
    }
    let text = (|| {
        // Respect the same registered opt-out formats used by Windows
        // clipboard history. Password managers can set either marker.
        let excluded = unsafe {
            RegisterClipboardFormatW(wide("ExcludeClipboardContentFromMonitorProcessing").as_ptr())
        };
        if excluded != 0 && unsafe { IsClipboardFormatAvailable(excluded) } != 0 {
            return None;
        }
        let may_include =
            unsafe { RegisterClipboardFormatW(wide("CanIncludeInClipboardHistory").as_ptr()) };
        if may_include != 0 && unsafe { IsClipboardFormatAvailable(may_include) } != 0 {
            let marker = unsafe { GetClipboardData(may_include) };
            if marker.is_null() || unsafe { GlobalSize(marker) } < size_of::<u32>() {
                return None;
            }
            let pointer = unsafe { GlobalLock(marker) } as *const u32;
            if pointer.is_null() {
                return None;
            }
            let include = unsafe { std::ptr::read_unaligned(pointer) };
            unsafe { GlobalUnlock(marker) };
            if include == 0 {
                return None;
            }
        }
        let handle = unsafe { GetClipboardData(13) }; // CF_UNICODETEXT
        if handle.is_null() {
            return None;
        }
        let byte_count = unsafe { GlobalSize(handle) }.min(128 * 1024);
        if byte_count < 2 {
            return None;
        }
        let pointer = unsafe { GlobalLock(handle) } as *const u16;
        if pointer.is_null() {
            return None;
        }
        let units = unsafe { std::slice::from_raw_parts(pointer, byte_count / 2) };
        let end = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        let result = String::from_utf16_lossy(&units[..end]);
        unsafe { GlobalUnlock(handle) };
        Some(result)
    })();
    unsafe { CloseClipboard() };
    text
}

fn write_clipboard_text(value: &str) -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    let units: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let Some(bytes) = units.len().checked_mul(size_of::<u16>()) else {
        return false;
    };
    if bytes > 128 * 1024 || unsafe { OpenClipboard(null_mut()) } == 0 {
        return false;
    }
    let mut success = false;
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
    if !handle.is_null() {
        let pointer = unsafe { GlobalLock(handle) } as *mut u16;
        if !pointer.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len());
                GlobalUnlock(handle);
            }
            if unsafe { EmptyClipboard() } != 0
                && !unsafe { SetClipboardData(13, handle) }.is_null()
            {
                success = true; // Windows owns the handle after SetClipboardData succeeds.
            }
        }
        if !success {
            unsafe { GlobalFree(handle) };
        }
    }
    unsafe { CloseClipboard() };
    success
}

fn copy_png_image(bytes: &[u8]) -> bool {
    use std::io::Cursor;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };

    if bytes.len() > 24 * 1024 * 1024 {
        return false;
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    let Some(input_size) = reader.output_buffer_size() else {
        return false;
    };
    if input_size > 64 * 1024 * 1024 {
        return false;
    }
    let mut input = vec![0; input_size];
    let Ok(frame) = reader.next_frame(&mut input) else {
        return false;
    };
    let width = frame.width as usize;
    let height = frame.height as usize;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return false,
    };
    let Some(pixel_bytes) = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return false;
    };
    if pixel_bytes > 64 * 1024 * 1024 {
        return false;
    }
    let Some(total_bytes) = pixel_bytes.checked_add(size_of::<BITMAPINFOHEADER>()) else {
        return false;
    };
    if unsafe { OpenClipboard(null_mut()) } == 0 {
        return false;
    }
    let mut success = false;
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, total_bytes) };
    if !handle.is_null() {
        let pointer = unsafe { GlobalLock(handle) } as *mut u8;
        if !pointer.is_null() {
            let mut header = BITMAPINFOHEADER::default();
            header.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            header.biWidth = frame.width as i32;
            header.biHeight = -(frame.height as i32);
            header.biPlanes = 1;
            header.biBitCount = 32;
            header.biCompression = BI_RGB;
            header.biSizeImage = pixel_bytes as u32;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (&header as *const BITMAPINFOHEADER).cast::<u8>(),
                    pointer,
                    size_of::<BITMAPINFOHEADER>(),
                );
                let output = std::slice::from_raw_parts_mut(
                    pointer.add(size_of::<BITMAPINFOHEADER>()),
                    pixel_bytes,
                );
                for (source, target) in input[..frame.buffer_size()]
                    .chunks_exact(channels)
                    .zip(output.chunks_exact_mut(4))
                {
                    target.copy_from_slice(&[
                        source[2],
                        source[1],
                        source[0],
                        if channels == 4 { source[3] } else { 255 },
                    ]);
                }
                GlobalUnlock(handle);
            }
            if unsafe { EmptyClipboard() } != 0 && !unsafe { SetClipboardData(8, handle) }.is_null()
            {
                success = true;
            }
        }
        if !success {
            unsafe { GlobalFree(handle) };
        }
    }
    unsafe { CloseClipboard() };
    success
}

fn copy_file_to_clipboard(path: &std::path::Path) -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };
    #[repr(C)]
    struct DropFiles {
        offset: u32,
        x: i32,
        y: i32,
        non_client: i32,
        wide: i32,
    }

    let units: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();
    let Some(total_bytes) = units
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|bytes| bytes.checked_add(size_of::<DropFiles>()))
    else {
        return false;
    };
    if total_bytes > 16 * 1024 || unsafe { OpenClipboard(null_mut()) } == 0 {
        return false;
    }
    let mut success = false;
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, total_bytes) };
    if !handle.is_null() {
        let pointer = unsafe { GlobalLock(handle) } as *mut u8;
        if !pointer.is_null() {
            let header = DropFiles {
                offset: size_of::<DropFiles>() as u32,
                x: 0,
                y: 0,
                non_client: 0,
                wide: 1,
            };
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (&header as *const DropFiles).cast::<u8>(),
                    pointer,
                    size_of::<DropFiles>(),
                );
                std::ptr::copy_nonoverlapping(
                    units.as_ptr(),
                    pointer.add(size_of::<DropFiles>()).cast::<u16>(),
                    units.len(),
                );
                GlobalUnlock(handle);
            }
            if unsafe { EmptyClipboard() } != 0
                && !unsafe { SetClipboardData(15, handle) }.is_null()
            {
                success = true;
            }
        }
        if !success {
            unsafe { GlobalFree(handle) };
        }
    }
    unsafe { CloseClipboard() };
    success
}

fn paste_clipboard_text(value: &str) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CONTROL,
    };
    if !write_clipboard_text(value) {
        return false;
    }
    let keys = [
        (VK_CONTROL, false),
        (b'V' as u16, false),
        (b'V' as u16, true),
        (VK_CONTROL, true),
    ];
    let mut inputs = vec![INPUT::default(); keys.len()];
    for (input, (key, up)) in inputs.iter_mut().zip(keys) {
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous = INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        };
    }
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        ) == inputs.len() as u32
    }
}

fn open_shelf_path(window: HWND, path: &std::path::Path) -> bool {
    let target: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let operation = wide("open");
    let result = unsafe {
        ShellExecuteW(
            window,
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOW,
        )
    };
    result as isize > 32
}

fn fetch_weather(
    city: &str,
    units: WeatherUnits,
    cached_place: Option<(f64, f64, String, String)>,
) -> Result<(Vec<String>, serde_json::Value), String> {
    fn request(url: &str, data: &[String]) -> Result<serde_json::Value, String> {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        let mut command = std::process::Command::new("curl.exe");
        command.creation_flags(CREATE_NO_WINDOW);
        command.args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--max-time",
            "10",
            "--max-filesize",
            "1000000",
            "--get",
        ]);
        for value in data {
            command.arg("--data-urlencode").arg(value);
        }
        let output = command
            .arg(url)
            .output()
            .map_err(|_| "Windows curl.exe is unavailable.".to_owned())?;
        if !output.status.success() {
            return Err("Could not fetch weather. Check the connection and try again.".to_owned());
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "Weather service returned invalid data.".to_owned())
    }

    let (latitude, longitude, name, country) = if let Some((latitude, longitude, name, country)) =
        cached_place
        && latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
    {
        (latitude, longitude, name, country)
    } else {
        let mut candidates = vec![city.trim().to_owned()];
        if let Some(before_comma) = city.split(',').next() {
            if !before_comma.eq_ignore_ascii_case(city) {
                candidates.push(before_comma.trim().to_owned());
            }
        }
        let words: Vec<_> = city.split_whitespace().collect();
        for count in (1..words.len()).rev() {
            let candidate = words[..count].join(" ");
            if !candidates
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&candidate))
            {
                candidates.push(candidate);
            }
        }
        let mut place = None;
        for candidate in candidates.into_iter().take(4) {
            let location = request(
                "https://geocoding-api.open-meteo.com/v1/search",
                &[
                    format!("name={candidate}"),
                    "count=1".to_owned(),
                    "language=en".to_owned(),
                    "format=json".to_owned(),
                ],
            )?;
            if let Some(found) = location["results"]
                .as_array()
                .and_then(|items| items.first())
            {
                place = Some(found.clone());
                break;
            }
        }
        let Some(place) = place else {
            return Err("City not found. Try a more specific name.".to_owned());
        };
        let (Some(latitude), Some(longitude)) =
            (place["latitude"].as_f64(), place["longitude"].as_f64())
        else {
            return Err("Weather service did not return coordinates.".to_owned());
        };
        let name = place["name"].as_str().unwrap_or(city).to_owned();
        let country = place["country_code"].as_str().unwrap_or("").to_owned();
        (latitude, longitude, name, country)
    };
    let forecast = request(
        "https://api.open-meteo.com/v1/forecast",
        &[
            format!("latitude={latitude:.4}"),
            format!("longitude={longitude:.4}"),
            "current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day".to_owned(),
            "hourly=weather_code,precipitation_probability,precipitation".to_owned(),
            "forecast_hours=12".to_owned(),
            "daily=temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code,sunrise,sunset".to_owned(),
            "timezone=auto".to_owned(),
            "forecast_days=7".to_owned(),
            format!(
                "temperature_unit={}",
                if units == WeatherUnits::Imperial {
                    "fahrenheit"
                } else {
                    "celsius"
                }
            ),
            format!(
                "wind_speed_unit={}",
                if units == WeatherUnits::Imperial {
                    "mph"
                } else {
                    "ms"
                }
            ),
            format!(
                "precipitation_unit={}",
                if units == WeatherUnits::Imperial {
                    "inch"
                } else {
                    "mm"
                }
            ),
        ],
    )?;
    let Some(temperature) = forecast["current"]["temperature_2m"].as_f64() else {
        return Err("Weather service did not return a current forecast.".to_owned());
    };
    let humidity = forecast["current"]["relative_humidity_2m"]
        .as_u64()
        .unwrap_or(0);
    let wind = forecast["current"]["wind_speed_10m"]
        .as_f64()
        .unwrap_or(0.0);
    let high = forecast["daily"]["temperature_2m_max"][0].as_f64();
    let low = forecast["daily"]["temperature_2m_min"][0].as_f64();
    let rain = forecast["daily"]["precipitation_sum"][0].as_f64();
    let (temperature_suffix, wind_suffix, rain_suffix) = if units == WeatherUnits::Imperial {
        ("°F", "mph", "in")
    } else {
        ("°C", "m/s", "mm")
    };
    let mut lines = vec![
        format!("{name}, {country} · {temperature:.0}{temperature_suffix} now"),
        format!("Humidity {humidity}% · Wind {wind:.0} {wind_suffix}"),
    ];
    if let (Some(low), Some(high), Some(rain)) = (low, high, rain) {
        lines.push(format!(
            "Today {low:.0}–{high:.0}{temperature_suffix} · Rain {rain:.1} {rain_suffix}"
        ));
    }
    let data = serde_json::json!({
        "location": name,
        "country": country,
        "latitude": latitude,
        "longitude": longitude,
        "timezone": forecast["timezone"],
        "utcOffsetSeconds": forecast["utc_offset_seconds"],
        "unit": temperature_suffix,
        "windUnit": wind_suffix,
        "rainUnit": rain_suffix,
        "current": forecast["current"],
        "hourly": forecast["hourly"],
        "daily": forecast["daily"],
    });
    Ok((lines, data))
}

fn color_ref(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let digits = value.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&digits[0..2], 16).ok()?,
        u8::from_str_radix(&digits[2..4], 16).ok()?,
        u8::from_str_radix(&digits[4..6], 16).ok()?,
    ))
}

fn current_clock_label() -> String {
    let mut now = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut now) };
    let hour = match now.wHour % 12 {
        0 => 12,
        value => value,
    };
    format!("{hour}:{:02}", now.wMinute)
}

fn current_status_label(clock_indicator_enabled: bool, battery_indicator_enabled: bool) -> String {
    let clock = clock_indicator_enabled.then(current_clock_label);
    current_status_label_from_clock(clock, battery_indicator_enabled)
}

fn current_status_label_from_clock(
    clock: Option<String>,
    battery_indicator_enabled: bool,
) -> String {
    let percent = if battery_indicator_enabled {
        let mut status = SYSTEM_POWER_STATUS::default();
        (unsafe { GetSystemPowerStatus(&mut status) } != 0)
            .then(|| battery_percentage(status.BatteryFlag, status.BatteryLifePercent))
            .flatten()
    } else {
        None
    };
    format_status_label(clock, percent)
}

fn format_status_label(clock: Option<String>, battery_percent: Option<u8>) -> String {
    match (clock, battery_percent) {
        (Some(clock), Some(percent)) => format!("{clock}   {percent}%"),
        (Some(clock), None) => clock,
        (None, Some(percent)) => format!("{percent}%"),
        (None, None) => String::new(),
    }
}

fn battery_percentage(battery_flag: u8, battery_life_percent: u8) -> Option<u8> {
    (!matches!(battery_flag, 128 | 255) && battery_life_percent <= 100)
        .then_some(battery_life_percent)
}

fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::{
        FALLBACK_FRAME_INTERVAL, MAX_SHELL_SURFACE_BYTES, animation_frame_interval,
        battery_percentage, composite_shell_pixel, format_status_label, is_fullscreen_bounds,
        refresh_period_for_rate, restore_opaque_alpha_rect, shell_surface_metrics,
        smoothed_render_cost,
    };
    use std::time::Duration;

    #[test]
    fn shell_surface_metrics_support_common_dpi_scales_with_a_memory_ceiling() {
        for dpi in [96, 120, 144, 168, 192, 384, 600] {
            let (scale, width, height) = shell_surface_metrics(dpi)
                .unwrap_or_else(|| panic!("common display DPI {dpi} should fit the shell budget"));
            assert!((scale - f64::from(dpi) / 96.0).abs() < f64::EPSILON);
            assert!(width > 0 && height > 0);
            assert!((width as usize) * (height as usize) * 5 <= MAX_SHELL_SURFACE_BYTES);
        }
        assert!(shell_surface_metrics(0).is_none());
        assert!(shell_surface_metrics(960).is_none());
    }

    #[test]
    fn fullscreen_detection_accepts_screen_covering_bounds_but_not_maximized_windows() {
        let screen = (1920, 1080);
        assert!(is_fullscreen_bounds(
            super::RECT {
                left: 0,
                top: 0,
                right: screen.0,
                bottom: screen.1,
            },
            screen.0,
            screen.1,
        ));
        assert!(is_fullscreen_bounds(
            super::RECT {
                left: -1,
                top: -1,
                right: screen.0 + 1,
                bottom: screen.1 + 1,
            },
            screen.0,
            screen.1,
        ));
        assert!(!is_fullscreen_bounds(
            super::RECT {
                left: 0,
                top: 0,
                right: screen.0,
                bottom: screen.1 - 40,
            },
            screen.0,
            screen.1,
        ));
        assert!(!is_fullscreen_bounds(
            super::RECT {
                left: 0,
                top: 0,
                right: screen.0,
                bottom: screen.1,
            },
            0,
            screen.1,
        ));
    }

    #[test]
    fn shell_cadence_tracks_whole_display_periods_with_render_headroom() {
        let cost = Duration::from_millis(9);
        for (rate, expected_periods) in [
            (24, 1),
            (30, 1),
            (50, 1),
            (60, 1),
            (75, 1),
            (90, 1),
            (120, 2),
            (144, 2),
            (165, 2),
            (240, 3),
            (360, 4),
        ] {
            let refresh = refresh_period_for_rate(rate, 1);
            assert_eq!(
                animation_frame_interval(refresh, cost).as_nanos(),
                refresh.as_nanos() * expected_periods,
                "unexpected cadence for {rate} Hz"
            );
        }
        assert_eq!(
            refresh_period_for_rate(60_000, 1_001),
            Duration::from_nanos(16_683_334)
        );
    }

    #[test]
    fn shell_cadence_falls_back_on_invalid_rates_and_slows_after_expensive_frames() {
        assert_eq!(refresh_period_for_rate(0, 1), FALLBACK_FRAME_INTERVAL);
        assert_eq!(refresh_period_for_rate(60, 0), FALLBACK_FRAME_INTERVAL);
        assert_eq!(refresh_period_for_rate(1, 1), FALLBACK_FRAME_INTERVAL);
        let refresh = refresh_period_for_rate(120, 1);
        let slow_cost = smoothed_render_cost(Duration::from_millis(9), Duration::from_millis(20));
        assert_eq!(slow_cost, Duration::from_millis(20));
        assert_eq!(
            animation_frame_interval(refresh, slow_cost).as_nanos(),
            refresh.as_nanos() * 3
        );
        assert!(smoothed_render_cost(slow_cost, Duration::from_millis(8)) < slow_cost);
    }

    #[test]
    fn battery_status_hides_unavailable_or_invalid_values() {
        assert_eq!(battery_percentage(0, 0), Some(0));
        assert_eq!(battery_percentage(0, 100), Some(100));
        assert_eq!(battery_percentage(8, 42), Some(42));
        assert_eq!(battery_percentage(128, 72), None);
        assert_eq!(battery_percentage(255, 72), None);
        assert_eq!(battery_percentage(0, 255), None);
        assert_eq!(battery_percentage(0, 101), None);
    }

    #[test]
    fn status_label_omits_the_battery_when_disabled_or_unavailable() {
        assert_eq!(format_status_label(Some("7:04".into()), None), "7:04");
        assert_eq!(
            format_status_label(Some("7:04".into()), Some(100)),
            "7:04   100%"
        );
        assert_eq!(format_status_label(None, Some(42)), "42%");
        assert_eq!(format_status_label(None, None), "");
    }

    #[test]
    fn shell_pixel_compositing_keeps_opaque_and_antialiased_bgra_valid() {
        let mut opaque = [0, 0, 0, 91];
        let mut alpha = 255;
        composite_shell_pixel(&mut opaque, &mut alpha, 255);
        assert_eq!(opaque, [0, 0, 0, 255]);
        assert_eq!(alpha, 255);

        let mut edge = [250, 200, 100, 33];
        let mut alpha = 128;
        composite_shell_pixel(&mut edge, &mut alpha, 255);
        assert_eq!(edge, [125, 100, 50, 128]);
        assert_eq!(alpha, 128);

        let mut faded = [246, 247, 250, 0];
        let mut alpha = 255;
        composite_shell_pixel(&mut faded, &mut alpha, 128);
        assert_eq!(faded, [123, 124, 125, 128]);
        assert_eq!(alpha, 128);

        let mut transparent_light = [246, 247, 250, 0];
        let mut alpha = 0;
        composite_shell_pixel(&mut transparent_light, &mut alpha, 255);
        assert_eq!(transparent_light, [0, 0, 0, 0]);
    }

    #[test]
    fn opaque_gdi_alpha_restore_changes_only_the_requested_inner_rectangle() {
        let mut pixels = [220, 100, 50, 7].repeat(6);
        let source_alpha = [255, 128, 0, 0, 255, 255];
        restore_opaque_alpha_rect(&mut pixels, &source_alpha, 12, 3, (1, 2, 0, 1));
        assert_eq!(&pixels[0..4], &[220, 100, 50, 7]);
        assert_eq!(&pixels[4..8], &[110, 50, 25, 128]);
        assert_eq!(&pixels[8..12], &[220, 100, 50, 7]);
        assert_eq!(&pixels[12..16], &[220, 100, 50, 7]);
    }
}
