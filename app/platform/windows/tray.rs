use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use eframe::egui;
use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, SetLastError, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GWLP_USERDATA, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, IDI_APPLICATION,
    LoadIconW, MF_STRING, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, SetWindowLongPtrW, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_CLOSE, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_NULL,
    WM_RBUTTONUP, WNDCLASSW,
};

const CLASS_NAME: &str = "IslandDesktopTrayWindow";
const WINDOW_TITLE: &str = "Island";
const TRAY_ID: u32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 17;
const COMMAND_OPEN: usize = 1;
const COMMAND_QUIT: usize = 2;

struct TrayCallbacks {
    open_requested: AtomicBool,
    quit_requested: AtomicBool,
    root_window: AtomicIsize,
    previous_foreground: AtomicIsize,
    context: egui::Context,
    taskbar_created: u32,
}

pub struct TrayController {
    window: HWND,
    _callbacks: Box<TrayCallbacks>,
}

impl TrayController {
    pub fn new(context: &egui::Context) -> Result<Self, String> {
        let class_name = wide(CLASS_NAME);
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance.is_null() {
            return Err(last_error("could not get the application instance"));
        }

        let window_class = WNDCLASSW {
            lpfnWndProc: Some(tray_window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&window_class) } == 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_CLASS_ALREADY_EXISTS as i32) {
                return Err(format!("could not register tray window: {error}"));
            }
        }

        let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        if taskbar_created == 0 {
            return Err(last_error(
                "could not register the Explorer restart notification",
            ));
        }

        let mut callbacks = Box::new(TrayCallbacks {
            open_requested: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
            root_window: AtomicIsize::new(0),
            previous_foreground: AtomicIsize::new(0),
            context: context.clone(),
            taskbar_created,
        });
        let callback_pointer = (&mut *callbacks as *mut TrayCallbacks).cast();
        let window = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                wide(WINDOW_TITLE).as_ptr(),
                0,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                callback_pointer,
            )
        };
        if window.is_null() {
            return Err(last_error("could not create the tray message window"));
        }

        let tray_data = match notify_icon_data(window) {
            Ok(tray_data) => tray_data,
            Err(error) => {
                unsafe { DestroyWindow(window) };
                return Err(error);
            }
        };
        unsafe { SetLastError(0) };
        if unsafe { Shell_NotifyIconW(NIM_ADD, &tray_data) } == 0 {
            let error_code = unsafe { GetLastError() };
            unsafe { DestroyWindow(window) };
            let reason = if error_code == 0 {
                "Windows did not add the tray icon (the API supplied no extended error)".to_owned()
            } else {
                format!(
                    "Windows did not add the tray icon: {}",
                    std::io::Error::from_raw_os_error(error_code as i32)
                )
            };
            return Err(reason);
        }

        Ok(Self {
            window,
            _callbacks: callbacks,
        })
    }

    pub fn take_show_request(&self) -> (bool, Option<HWND>) {
        if !self._callbacks.open_requested.swap(false, Ordering::AcqRel) {
            return (false, None);
        }
        let previous = self
            ._callbacks
            .previous_foreground
            .swap(0, Ordering::AcqRel);
        (true, (previous != 0).then_some(previous as HWND))
    }

    pub fn take_quit_request(&self) -> bool {
        self._callbacks.quit_requested.swap(false, Ordering::AcqRel)
    }

    pub fn set_root_window(&self, window: HWND) {
        self._callbacks
            .root_window
            .store(window as isize, Ordering::Release);
    }
}

impl Drop for TrayController {
    fn drop(&mut self) {
        unsafe {
            if let Ok(tray_data) = notify_icon_data(self.window) {
                Shell_NotifyIconW(NIM_DELETE, &tray_data);
            }
            DestroyWindow(self.window);
        }
    }
}

unsafe extern "system" fn tray_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: WM_NCCREATE's lparam points to the CREATESTRUCTW for this
        // window; lpCreateParams was set to the stable boxed callback state.
        let create = unsafe {
            &*(lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
        };
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    let state_pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut TrayCallbacks };
    if state_pointer.is_null() {
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    // SAFETY: the boxed state is kept alive by TrayController until after its
    // HWND is destroyed; this window and its callbacks share the UI thread.
    let state = unsafe { &*state_pointer };
    if state.taskbar_created != 0 && message == state.taskbar_created {
        if let Ok(tray_data) = notify_icon_data(window) {
            unsafe { Shell_NotifyIconW(NIM_ADD, &tray_data) };
        }
        return 0;
    }

    if message == TRAY_MESSAGE {
        match lparam as u32 {
            WM_LBUTTONUP | WM_LBUTTONDBLCLK => request_open(state),
            WM_RBUTTONUP => show_menu(window, state),
            _ => {}
        }
        return 0;
    }

    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn request_open(state: &TrayCallbacks) {
    remember_foreground(state);
    state.open_requested.store(true, Ordering::Release);
    state.context.request_repaint();
}

fn show_menu(window: HWND, state: &TrayCallbacks) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    let open_label = wide("Open Island");
    let quit_label = wide("Quit Island");
    let menu_ready = unsafe {
        AppendMenuW(menu, MF_STRING, COMMAND_OPEN, open_label.as_ptr()) != 0
            && AppendMenuW(menu, MF_STRING, COMMAND_QUIT, quit_label.as_ptr()) != 0
    };
    if !menu_ready {
        unsafe { DestroyMenu(menu) };
        return;
    }

    let mut cursor = windows_sys::Win32::Foundation::POINT::default();
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        unsafe { DestroyMenu(menu) };
        return;
    }

    let foreground = unsafe { GetForegroundWindow() };
    state
        .previous_foreground
        .store(foreground as isize, Ordering::Release);
    unsafe {
        SetForegroundWindow(window);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            window,
            null(),
        );
        PostMessageW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
        handle_menu_command(command, state);
    }
}

fn handle_menu_command(command: i32, state: &TrayCallbacks) {
    match command {
        value if value == COMMAND_OPEN as i32 => request_open(state),
        value if value == COMMAND_QUIT as i32 => {
            state.previous_foreground.store(0, Ordering::Release);
            state.quit_requested.store(true, Ordering::Release);
            let root = state.root_window.load(Ordering::Acquire) as HWND;
            if !root.is_null() {
                // A hidden eframe viewport can sleep through repaint requests.
                // WM_CLOSE wakes its native event loop; IslandApp consumes the
                // quit flag before deciding whether to hide or exit.
                unsafe { PostMessageW(root, WM_CLOSE, 0, 0) };
            }
            state.context.request_repaint();
        }
        _ => state.previous_foreground.store(0, Ordering::Release),
    }
}

fn remember_foreground(state: &TrayCallbacks) {
    let foreground = unsafe { GetForegroundWindow() };
    if !foreground.is_null() {
        let _ = state.previous_foreground.compare_exchange(
            0,
            foreground as isize,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

fn notify_icon_data(window: HWND) -> Result<NOTIFYICONDATAW, String> {
    let icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    if icon.is_null() {
        return Err(last_error("could not load the Windows application icon"));
    }

    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    copy_wide("Island — Settings", &mut data.szTip);
    Ok(data)
}

fn copy_wide<const N: usize>(value: &str, destination: &mut [u16; N]) {
    for (target, source) in destination.iter_mut().zip(value.encode_utf16()) {
        *target = source;
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_OPEN, COMMAND_QUIT, TRAY_MESSAGE, TrayCallbacks, TrayController, WM_LBUTTONUP,
        handle_menu_command,
    };
    use eframe::egui;
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::Foundation::LPARAM;

    fn callbacks() -> TrayCallbacks {
        TrayCallbacks {
            open_requested: Default::default(),
            quit_requested: Default::default(),
            root_window: Default::default(),
            previous_foreground: Default::default(),
            context: egui::Context::default(),
            taskbar_created: 0,
        }
    }

    #[test]
    fn open_menu_command_requests_open_without_quit() {
        let state = callbacks();
        handle_menu_command(COMMAND_OPEN as i32, &state);

        assert!(state.open_requested.load(Ordering::Acquire));
        assert!(!state.quit_requested.load(Ordering::Acquire));
    }

    #[test]
    fn quit_menu_command_requests_quit_and_clears_focus_restore() {
        let state = callbacks();
        state.previous_foreground.store(42, Ordering::Release);
        handle_menu_command(COMMAND_QUIT as i32, &state);

        assert!(!state.open_requested.load(Ordering::Acquire));
        assert!(state.quit_requested.load(Ordering::Acquire));
        assert_eq!(state.previous_foreground.load(Ordering::Acquire), 0);
    }

    #[test]
    fn dismissed_menu_clears_focus_restore_without_action() {
        let state = callbacks();
        state.previous_foreground.store(42, Ordering::Release);
        handle_menu_command(0, &state);

        assert!(!state.open_requested.load(Ordering::Acquire));
        assert!(!state.quit_requested.load(Ordering::Acquire));
        assert_eq!(state.previous_foreground.load(Ordering::Acquire), 0);
    }

    #[test]
    #[ignore = "requires an interactive Windows notification area"]
    fn native_tray_icon_registers_in_the_current_session() {
        let context = egui::Context::default();
        let tray = TrayController::new(&context)
            .unwrap_or_else(|error| panic!("could not initialize the Windows tray icon: {error}"));
        assert!(!tray.dispatch_window().is_null());

        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
                tray.dispatch_window(),
                TRAY_MESSAGE,
                0,
                WM_LBUTTONUP as LPARAM,
            );
        }
        assert!(tray.take_show_request().0);
        drop(tray);
    }
}
