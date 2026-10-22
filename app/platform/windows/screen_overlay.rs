//! Native desktop selection surface for screen tools.
//!
//! The Island reference uses a desktop DIB and a native selection overlay. This
//! module follows that model instead of asking WebView2 for a display stream.

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CreateCompatibleDC, CreateDIBSection,
    CreatePen, CreateSolidBrush, DEFAULT_GUI_FONT, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    EndPaint, FillRect, GetDC, GetStockObject, HBITMAP, HDC, HGDIOBJ, InvalidateRect, NULL_BRUSH,
    PAINTSTRUCT, Rectangle, ReleaseDC, RoundRect, SRCCOPY, SelectObject, SetBkMode, SetTextColor,
    StretchBlt, StretchDIBits, TRANSPARENT, TextOutW, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GWLP_USERDATA, GetMessageW, GetPhysicalCursorPos, GetSystemMetrics,
    HC_ACTION, HHOOK, HWND_TOPMOST, IDC_CROSS, LWA_COLORKEY, LoadCursorW, MSG, MSLLHOOKSTRUCT,
    PostMessageW, PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetForegroundWindow, SetLayeredWindowAttributes,
    SetWindowDisplayAffinity, SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, ShowWindow,
    TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SETCURSOR, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const CLASS_NAME: &str = "IslandNativeScreenOverlay";
const COLOR_PREVIEW_CLASS_NAME: &str = "IslandNativeColorPreview";
const RECORD_LIMIT: Duration = Duration::from_secs(120);
const MAX_RECORD_PIXELS: usize = 8_294_400;
const MAX_CAPTURE_PIXELS: usize = 24_000_000;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_QR_IMAGE_BYTES: usize = 6 * 1024 * 1024;
const TRANSPARENT_COLOR_KEY: u32 = 0x0003_0201;
const COLOR_PREVIEW_WIDTH: i32 = 312;
const COLOR_PREVIEW_HEIGHT: i32 = 172;
const WM_SELECTOR_MOVE: u32 = WM_APP + 0x56;
const WM_SELECTOR_LEFT_DOWN: u32 = WM_APP + 0x57;
const WM_SELECTOR_LEFT_UP: u32 = WM_APP + 0x58;
const WM_SELECTOR_CANCEL: u32 = WM_APP + 0x59;

thread_local! {
    static SELECTOR_HWND: Cell<isize> = const { Cell::new(0) };
    static SELECTOR_MOVE_QUEUED: Cell<bool> = const { Cell::new(false) };
    // Keep the newest physical pointer location even while a move message is
    // queued. The hook and this window's message loop run on the same thread,
    // so the queued message can safely consume the latest point instead of a
    // stale point from the first event in a burst.
    static SELECTOR_LATEST_POINT: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Capture,
    Ocr,
    Qr,
    Color,
    Record,
}

impl Mode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "capture" => Some(Self::Capture),
            "ocr" => Some(Self::Ocr),
            "qr" => Some(Self::Qr),
            "color" => Some(Self::Color),
            "record" => Some(Self::Record),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordQuality {
    Standard,
    #[default]
    High,
    Maximum,
}

impl RecordQuality {
    pub fn parse(value: &str) -> Self {
        match value {
            "standard" => Self::Standard,
            "maximum" => Self::Maximum,
            _ => Self::High,
        }
    }

    fn profile(self) -> (u32, u32, u32, u32) {
        match self {
            Self::Standard => (1280, 720, 24, 5_000_000),
            Self::High => (1920, 1080, 30, 12_000_000),
            Self::Maximum => (3840, 2160, 30, 35_000_000),
        }
    }
}

#[derive(Debug)]
pub enum ResultEvent {
    Selected {
        mode: Mode,
        png: Vec<u8>,
        width: u32,
        height: u32,
        preview_png: Option<Vec<u8>>,
    },
    Color {
        hex: String,
        rgb: String,
    },
    RecordingStarted {
        path: String,
        stop: Sender<()>,
        start: Sender<()>,
    },
    Recorded {
        path: String,
        duration_ms: u64,
        size_bytes: u64,
    },
    Error {
        mode: Option<Mode>,
        message: String,
    },
    Canceled,
}

pub fn launch(
    hidden_windows: Vec<(isize, bool)>,
    notify_window: isize,
    sender: Sender<ResultEvent>,
    mode: Mode,
    full_screen: bool,
    quality: RecordQuality,
) {
    let failure_sender = sender.clone();
    let result = thread::Builder::new()
        .name("island-screen-overlay".into())
        .spawn(move || {
            run(
                hidden_windows,
                notify_window,
                sender,
                mode,
                full_screen,
                quality,
            )
        });
    if result.is_err() {
        let _ = failure_sender.send(ResultEvent::Error {
            mode: Some(mode),
            message: "Island could not start the desktop selector.".into(),
        });
        unsafe { PostMessageW(notify_window as HWND, WM_APP + 0x55, 0, 0) };
    }
}

struct Capture {
    origin: POINT,
    width: i32,
    height: i32,
    pixels: Vec<u8>, // top-down BGRA
}

struct Overlay {
    hwnd: HWND,
    preview_hwnd: HWND,
    notify_window: isize,
    sender: Sender<ResultEvent>,
    mode: Mode,
    quality: RecordQuality,
    capture: Capture,
    cursor: POINT,
    color_patch: Option<Capture>,
    color_sampled_at: Instant,
    selection: Option<RECT>,
    drag_start: Option<POINT>,
    record_stop: Option<Sender<()>>,
    record_start: Option<Sender<()>>,
    record_path: Option<String>,
    canceled: bool,
}

fn run(
    hidden_windows: Vec<(isize, bool)>,
    notify_window: isize,
    sender: Sender<ResultEvent>,
    mode: Mode,
    full_screen: bool,
    quality: RecordQuality,
) {
    for (hwnd, _) in &hidden_windows {
        unsafe { ShowWindow(*hwnd as HWND, SW_HIDE) };
    }
    let _ = unsafe { windows_sys::Win32::Graphics::Dwm::DwmFlush() };
    let outcome = match virtual_desktop_bounds() {
        Ok(capture) => {
            let selection = (full_screen && mode != Mode::Color).then_some(RECT {
                left: 0,
                top: 0,
                right: capture.width,
                bottom: capture.height,
            });
            let mut overlay = Box::new(Overlay {
                hwnd: null_mut(),
                preview_hwnd: null_mut(),
                notify_window,
                sender: sender.clone(),
                mode,
                quality,
                capture,
                cursor: POINT::default(),
                color_patch: None,
                color_sampled_at: Instant::now() - Duration::from_secs(1),
                selection,
                drag_start: None,
                record_stop: None,
                record_start: None,
                record_path: None,
                canceled: false,
            });
            match run_window(&mut overlay) {
                Ok(event) => event,
                Err(message) => ResultEvent::Error {
                    mode: Some(mode),
                    message,
                },
            }
        }
        Err(message) => ResultEvent::Error {
            mode: Some(mode),
            message,
        },
    };
    let record_start = match &outcome {
        ResultEvent::RecordingStarted { start, .. } => Some(start.clone()),
        _ => None,
    };
    let _ = sender.send(outcome);
    // Queue the started event before the worker can fail initialization.
    if let Some(start) = record_start {
        let _ = start.send(());
    }
    for (hwnd, visible) in hidden_windows {
        if visible {
            unsafe {
                ShowWindow(
                    hwnd as HWND,
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
                )
            };
        }
    }
    unsafe { PostMessageW(notify_window as HWND, WM_APP + 0x55, 0, 0) };
}

fn run_window(overlay: &mut Box<Overlay>) -> Result<ResultEvent, String> {
    let class_name = wide(CLASS_NAME);
    let title = wide("Island screen selection");
    let module = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: module,
        hCursor: unsafe { LoadCursorW(null_mut(), IDC_CROSS) },
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    unsafe { RegisterClassW(&class) };
    let width = overlay.capture.width;
    let height = overlay.capture.height;
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            overlay.capture.origin.x,
            overlay.capture.origin.y,
            width,
            height,
            null_mut(),
            null_mut(),
            module,
            (&mut **overlay as *mut Overlay).cast(),
        )
    };
    if hwnd.is_null() {
        return Err("Island could not open the desktop selection surface.".into());
    }
    overlay.hwnd = hwnd;
    let mut cursor = POINT::default();
    if unsafe { GetPhysicalCursorPos(&mut cursor) } != 0 {
        overlay.cursor = POINT {
            x: cursor.x - overlay.capture.origin.x,
            y: cursor.y - overlay.capture.origin.y,
        };
    }
    if overlay.mode == Mode::Color {
        update_color_patch(overlay);
    }
    if unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_COLOR_KEY, 255, LWA_COLORKEY) } == 0 {
        unsafe {
            DestroyWindow(hwnd);
        }
        overlay.hwnd = null_mut();
        return Err("Island could not make the screen selector transparent.".into());
    }
    // Exclude this top-level overlay from subsequent screen-recording frames.
    unsafe { SetWindowDisplayAffinity(hwnd, 0x11) };
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            overlay.capture.origin.x,
            overlay.capture.origin.y,
            width,
            height,
            SWP_NOACTIVATE,
        );
    }
    if overlay.mode == Mode::Color {
        let preview = match create_color_preview(&mut **overlay, module) {
            Ok(preview) => preview,
            Err(error) => {
                unsafe {
                    DestroyWindow(hwnd);
                }
                overlay.hwnd = null_mut();
                return Err(error);
            }
        };
        overlay.preview_hwnd = preview;
    }
    {
        // A color-keyed layered surface intentionally lets Windows hit-test
        // through transparent pixels. Capture alone cannot reliably own the
        // first click across process/thread boundaries, so consume selection
        // input with a temporary low-level hook while the selector is alive.
        let _mouse_hook = match MouseHookGuard::install(hwnd, module) {
            Ok(hook) => hook,
            Err(error) => {
                if !overlay.preview_hwnd.is_null() {
                    unsafe {
                        DestroyWindow(overlay.preview_hwnd);
                    }
                }
                unsafe {
                    DestroyWindow(hwnd);
                }
                overlay.hwnd = null_mut();
                return Err(error);
            }
        };
        if overlay.selection.is_some() {
            // Full-screen actions were chosen in the React tool panel. Process
            // the selection without flashing a native desktop control surface.
            finish_selection(overlay);
        } else {
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
                UpdateWindow(hwnd);
                if !overlay.preview_hwnd.is_null() {
                    InvalidateRect(overlay.preview_hwnd, null(), 0);
                    UpdateWindow(overlay.preview_hwnd);
                    ShowWindow(overlay.preview_hwnd, SW_SHOWNOACTIVATE);
                }
            }
        }
        let mut message = MSG::default();
        loop {
            let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
            if result <= 0 {
                break;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    if !overlay.preview_hwnd.is_null() {
        unsafe {
            DestroyWindow(overlay.preview_hwnd);
        }
    }
    unsafe {
        DestroyWindow(hwnd);
    }
    overlay.hwnd = null_mut();
    if let (Some(stop), Some(start), Some(path)) = (
        overlay.record_stop.take(),
        overlay.record_start.take(),
        overlay.record_path.take(),
    ) {
        return Ok(ResultEvent::RecordingStarted { path, stop, start });
    }
    if overlay.canceled {
        return Ok(ResultEvent::Canceled);
    }
    if let Some(selection) = overlay.selection {
        if overlay.mode == Mode::Color {
            let point = POINT {
                x: overlay.capture.origin.x + overlay.cursor.x,
                y: overlay.capture.origin.y + overlay.cursor.y,
            };
            let pixel = RECT {
                left: point.x,
                top: point.y,
                right: point.x + 1,
                bottom: point.y + 1,
            };
            let pixels = capture_region(pixel, 1, 1)?;
            let (r, g, b) = sample_bgra(&pixels, 0);
            return Ok(ResultEvent::Color {
                hex: format!("#{r:02X}{g:02X}{b:02X}"),
                rgb: format!("rgb({r}, {g}, {b})"),
            });
        }
        let screen_rect = RECT {
            left: selection.left + overlay.capture.origin.x,
            top: selection.top + overlay.capture.origin.y,
            right: selection.right + overlay.capture.origin.x,
            bottom: selection.bottom + overlay.capture.origin.y,
        };
        let width = (screen_rect.right - screen_rect.left).max(1) as u32;
        let height = (screen_rect.bottom - screen_rect.top).max(1) as u32;
        if (width as usize).saturating_mul(height as usize) > MAX_CAPTURE_PIXELS {
            return Err("The selected area is too large to capture. Choose a smaller area.".into());
        }
        let fresh = Capture {
            origin: POINT {
                x: screen_rect.left,
                y: screen_rect.top,
            },
            width: width as i32,
            height: height as i32,
            pixels: capture_region(screen_rect, width as i32, height as i32)?,
        };
        let png = encode_png_region(&fresh, screen_rect, overlay.mode)?;
        let preview_png = if overlay.mode == Mode::Capture {
            Some(encode_preview_png(&fresh, screen_rect, 480, 270)?)
        } else {
            None
        };
        return Ok(ResultEvent::Selected {
            mode: overlay.mode,
            png,
            width,
            height,
            preview_png,
        });
    }
    Ok(ResultEvent::Canceled)
}

fn create_color_preview(
    overlay: &mut Overlay,
    module: windows_sys::Win32::Foundation::HINSTANCE,
) -> Result<HWND, String> {
    let class_name = wide(COLOR_PREVIEW_CLASS_NAME);
    let class = WNDCLASSW {
        lpfnWndProc: Some(color_preview_window_proc),
        hInstance: module,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    unsafe {
        RegisterClassW(&class);
    }
    let position = color_preview_rect(&overlay.capture, overlay.cursor);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            wide("Island color preview").as_ptr(),
            WS_POPUP,
            overlay.capture.origin.x + position.left,
            overlay.capture.origin.y + position.top,
            COLOR_PREVIEW_WIDTH,
            COLOR_PREVIEW_HEIGHT,
            null_mut(),
            null_mut(),
            module,
            (overlay as *mut Overlay).cast(),
        )
    };
    if hwnd.is_null() {
        return Err("Island could not open the color preview.".into());
    }
    if unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_COLOR_KEY, 255, LWA_COLORKEY) } == 0 {
        unsafe {
            DestroyWindow(hwnd);
        }
        return Err("Island could not make the color preview transparent.".into());
    }
    unsafe {
        SetWindowDisplayAffinity(hwnd, 0x11);
    }
    Ok(hwnd)
}

unsafe extern "system" fn color_preview_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
        if !create.is_null() {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            }
        }
        return 1;
    }
    let pointer = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    } as *mut Overlay;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let overlay = unsafe { &mut *pointer };
    match message {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_color_preview(hwnd, overlay);
            0
        }
        WM_CLOSE => {
            cancel_selection(overlay);
            0
        }
        WM_DESTROY => {
            overlay.preview_hwnd = null_mut();
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

struct MouseHookGuard(HHOOK);

impl MouseHookGuard {
    fn install(
        hwnd: HWND,
        module: windows_sys::Win32::Foundation::HINSTANCE,
    ) -> Result<Self, String> {
        SELECTOR_HWND.with(|active| active.set(hwnd as isize));
        SELECTOR_MOVE_QUEUED.with(|queued| queued.set(false));
        SELECTOR_LATEST_POINT.with(|latest| latest.set(0));
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), module, 0) };
        if hook.is_null() {
            SELECTOR_HWND.with(|active| active.set(0));
            return Err(
                "Island could not install the temporary screen-selection input hook.".into(),
            );
        }
        Ok(Self(hook))
    }
}

impl Drop for MouseHookGuard {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
        SELECTOR_HWND.with(|active| active.set(0));
        SELECTOR_MOVE_QUEUED.with(|queued| queued.set(false));
        SELECTOR_LATEST_POINT.with(|latest| latest.set(0));
    }
}

unsafe extern "system" fn low_level_mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < HC_ACTION as i32 {
        return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
    }
    let hwnd = SELECTOR_HWND.with(Cell::get) as HWND;
    if hwnd.is_null() {
        return unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) };
    }
    let event = wparam as u32;
    let hook = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
    let point = hook.pt;
    match event {
        WM_MOUSEMOVE => {
            SELECTOR_LATEST_POINT.with(|latest| latest.set(pack_screen_point(point) as u64));
            let already_queued = SELECTOR_MOVE_QUEUED.with(|queued| {
                if queued.get() {
                    true
                } else {
                    queued.set(true);
                    false
                }
            });
            if !already_queued {
                let posted = unsafe { PostMessageW(hwnd, WM_SELECTOR_MOVE, 0, 0) } != 0;
                if !posted {
                    SELECTOR_MOVE_QUEUED.with(|queued| queued.set(false));
                }
            }
            // Blocking WM_MOUSEMOVE here freezes the real Windows pointer.
            // Only mouse buttons are consumed; movement must reach Windows.
            unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
        }
        WM_LBUTTONDOWN => post_selector_mouse(hwnd, WM_SELECTOR_LEFT_DOWN, point),
        WM_LBUTTONUP => post_selector_mouse(hwnd, WM_SELECTOR_LEFT_UP, point),
        WM_RBUTTONDOWN => 1,
        WM_RBUTTONUP => {
            unsafe {
                PostMessageW(hwnd, WM_SELECTOR_CANCEL, 0, 0);
            }
            1
        }
        WM_MBUTTONDOWN | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP | WM_MOUSEWHEEL
        | WM_MOUSEHWHEEL => 1,
        _ => unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) },
    }
}

fn post_selector_mouse(hwnd: HWND, message: u32, point: POINT) -> LRESULT {
    unsafe {
        PostMessageW(hwnd, message, 0, pack_screen_point(point));
    }
    1
}

fn pack_screen_point(point: POINT) -> LPARAM {
    ((point.x as u32 as u64) | ((point.y as u32 as u64) << 32)) as LPARAM
}

fn point_from_screen_lparam(lparam: LPARAM, capture: &Capture) -> POINT {
    let packed = lparam as u64;
    POINT {
        x: ((packed as u32) as i32 - capture.origin.x).clamp(0, capture.width.saturating_sub(1)),
        y: (((packed >> 32) as u32) as i32 - capture.origin.y)
            .clamp(0, capture.height.saturating_sub(1)),
    }
}

fn virtual_desktop_bounds() -> Result<Capture, String> {
    let origin = POINT {
        x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
        y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
    };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0
        || height <= 0
        || (width as usize).saturating_mul(height as usize) > MAX_CAPTURE_PIXELS
    {
        return Err(
            "The desktop is too large for one capture. Try a smaller display resolution.".into(),
        );
    }
    Ok(Capture {
        origin,
        width,
        height,
        pixels: Vec::new(),
    })
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: (width as u32)
                .saturating_mul(height as u32)
                .saturating_mul(4),
            ..unsafe { std::mem::zeroed() }
        },
        ..unsafe { std::mem::zeroed() }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const windows_sys::Win32::UI::WindowsAndMessaging::CREATESTRUCTW;
        if !create.is_null() {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            }
        }
        return 1;
    }
    let pointer = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    } as *mut Overlay;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let overlay = unsafe { &mut *pointer };
    match message {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd, overlay);
            0
        }
        WM_SETCURSOR => {
            unsafe {
                SetCursor(LoadCursorW(null_mut(), IDC_CROSS));
            }
            1
        }
        WM_MOUSEMOVE => {
            let point =
                cursor_in_overlay(&overlay.capture).unwrap_or_else(|| point_from_lparam(lparam));
            handle_mouse_move(hwnd, overlay, point);
            0
        }
        WM_SELECTOR_MOVE => {
            SELECTOR_MOVE_QUEUED.with(|queued| queued.set(false));
            let latest = SELECTOR_LATEST_POINT.with(Cell::get) as LPARAM;
            unsafe {
                SetCursor(LoadCursorW(null_mut(), IDC_CROSS));
            }
            handle_mouse_move(
                hwnd,
                overlay,
                point_from_screen_lparam(latest, &overlay.capture),
            );
            0
        }
        WM_SELECTOR_LEFT_DOWN => {
            handle_left_down(
                hwnd,
                overlay,
                point_from_screen_lparam(lparam, &overlay.capture),
            );
            0
        }
        WM_SELECTOR_LEFT_UP => {
            handle_left_up(
                hwnd,
                overlay,
                point_from_screen_lparam(lparam, &overlay.capture),
            );
            0
        }
        WM_SELECTOR_CANCEL => {
            cancel_selection(overlay);
            0
        }
        WM_LBUTTONDOWN => {
            let point =
                cursor_in_overlay(&overlay.capture).unwrap_or_else(|| point_from_lparam(lparam));
            handle_left_down(hwnd, overlay, point);
            0
        }
        WM_LBUTTONUP => {
            let point =
                cursor_in_overlay(&overlay.capture).unwrap_or_else(|| point_from_lparam(lparam));
            handle_left_up(hwnd, overlay, point);
            0
        }
        WM_KEYDOWN if wparam as u16 == VK_ESCAPE => {
            cancel_selection(overlay);
            0
        }
        WM_RBUTTONDOWN => {
            cancel_selection(overlay);
            0
        }
        WM_CLOSE => {
            cancel_selection(overlay);
            0
        }
        WM_DESTROY => {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn handle_mouse_move(hwnd: HWND, overlay: &mut Overlay, point: POINT) {
    let previous_cursor = overlay.cursor;
    let previous_selection = overlay.selection;
    let cursor_moved = previous_cursor.x != point.x || previous_cursor.y != point.y;
    overlay.cursor = point;
    if !cursor_moved {
        return;
    }
    if let Some(start) = overlay.drag_start {
        overlay.selection = Some(rect_from_points(start, overlay.cursor));
    }
    if overlay.mode == Mode::Color {
        let sampled = update_color_patch(overlay);
        if !overlay.preview_hwnd.is_null() {
            let old = color_preview_rect(&overlay.capture, previous_cursor);
            let current = color_preview_rect(&overlay.capture, overlay.cursor);
            // Move the small native surface. The compositor removes its old
            // position, so the picker cannot leave painted copies behind.
            if old.left != current.left || old.top != current.top {
                unsafe {
                    SetWindowPos(
                        overlay.preview_hwnd,
                        null_mut(),
                        overlay.capture.origin.x + current.left,
                        overlay.capture.origin.y + current.top,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                    );
                }
            }
            if sampled {
                unsafe {
                    InvalidateRect(overlay.preview_hwnd, null(), 0);
                }
            }
        }
    } else if overlay.drag_start.is_some() {
        if let Some(previous) = previous_selection {
            invalidate_selection_visuals(hwnd, previous);
        }
        if let Some(current) = overlay.selection {
            invalidate_selection_visuals(hwnd, current);
        }
    }
}

fn handle_left_down(hwnd: HWND, overlay: &mut Overlay, point: POINT) {
    if overlay.mode == Mode::Color {
        overlay.cursor = point;
        update_color_patch(overlay);
        overlay.drag_start = Some(point);
        return;
    }
    overlay.drag_start = Some(point);
    overlay.selection = Some(rect_from_points(point, point));
    invalidate_selection_visuals(hwnd, overlay.selection.unwrap());
}

fn handle_left_up(hwnd: HWND, overlay: &mut Overlay, point: POINT) {
    if overlay.canceled {
        overlay.drag_start = None;
        unsafe {
            PostQuitMessage(0);
        }
        return;
    }
    if overlay.drag_start.is_none() {
        return;
    }
    overlay.cursor = point;
    if overlay.mode == Mode::Color {
        overlay.drag_start = None;
        overlay.selection = Some(RECT {
            left: point.x,
            top: point.y,
            right: point.x + 1,
            bottom: point.y + 1,
        });
        finish_selection(overlay);
        return;
    }
    overlay.selection = overlay
        .drag_start
        .map(|start| rect_from_points(start, overlay.cursor));
    overlay.drag_start = None;
    if overlay
        .selection
        .is_some_and(|rect| rect.right - rect.left >= 4 && rect.bottom - rect.top >= 4)
    {
        finish_selection(overlay);
    } else {
        if let Some(selection) = overlay.selection.take() {
            invalidate_selection_visuals(hwnd, selection);
        }
    }
}

fn cancel_selection(overlay: &mut Overlay) {
    overlay.canceled = true;
    // Keep the temporary input hook alive until an in-flight mouse gesture
    // releases, so its final mouse-up cannot fall through to another app.
    if overlay.drag_start.is_none() {
        unsafe {
            PostQuitMessage(0);
        }
    }
}

fn finish_selection(overlay: &mut Overlay) {
    let Some(rect) = overlay.selection else {
        return;
    };
    if overlay.mode == Mode::Record {
        match start_recording(overlay, rect) {
            Ok(()) => unsafe {
                PostQuitMessage(0);
            },
            Err(message) => {
                let _ = overlay.sender.send(ResultEvent::Error {
                    mode: Some(Mode::Record),
                    message: message.clone(),
                });
                overlay.canceled = true;
                unsafe {
                    PostQuitMessage(0);
                }
            }
        }
        return;
    }
    unsafe {
        PostQuitMessage(0);
    }
}

fn start_recording(overlay: &mut Overlay, rect: RECT) -> Result<(), String> {
    let screen_rect = RECT {
        left: rect.left + overlay.capture.origin.x,
        top: rect.top + overlay.capture.origin.y,
        right: rect.right + overlay.capture.origin.x,
        bottom: rect.bottom + overlay.capture.origin.y,
    };
    let output = downloads_path("Island-recording", "mp4")?;
    let path = output.clone();
    let worker_path = path.clone();
    let (stop_tx, stop_rx) = channel();
    let (start_tx, start_rx) = channel();
    let sender = overlay.sender.clone();
    let notify_window = overlay.notify_window;
    let quality = overlay.quality;
    thread::Builder::new()
        .name("island-screen-recording".into())
        .spawn(move || {
            if start_rx.recv().is_err() {
                return;
            }
            let result = record_mp4(&worker_path, screen_rect, quality, &stop_rx);
            match result {
                Ok((path, duration_ms, size_bytes)) => {
                    let _ = sender.send(ResultEvent::Recorded {
                        path,
                        duration_ms,
                        size_bytes,
                    });
                }
                Err(message) => {
                    // A failed capture/encode must not leave a broken MP4 that looks
                    // like a successfully saved recording to the user.
                    let _ = std::fs::remove_file(&worker_path);
                    let _ = sender.send(ResultEvent::Error {
                        mode: Some(Mode::Record),
                        message,
                    });
                }
            }
            unsafe {
                PostMessageW(notify_window as HWND, WM_APP + 0x55, 0, 0);
            }
        })
        .map_err(|_| "Island could not start the screen recorder worker.".to_owned())?;
    overlay.record_stop = Some(stop_tx);
    overlay.record_start = Some(start_tx);
    overlay.record_path = Some(path.to_string_lossy().into_owned());
    Ok(())
}

fn record_mp4(
    path: &std::path::Path,
    rect: RECT,
    quality: RecordQuality,
    stop: &Receiver<()>,
) -> Result<(String, u64, u64), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_core::PCWSTR;
    use windows_startup::Win32::Media::MediaFoundation::{
        MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
        MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
        MF_VERSION, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
        MFCreateSinkWriterFromURL, MFMediaType_Video, MFShutdown, MFStartup, MFVideoFormat_H264,
        MFVideoFormat_RGB32, MFVideoInterlace_Progressive,
    };

    const TICKS_PER_SECOND: i64 = 10_000_000;
    let (max_width, max_height, frame_rate, bit_rate) = quality.profile();
    let screen_width = (rect.right - rect.left).max(2) as u32;
    let screen_height = (rect.bottom - rect.top).max(2) as u32;
    let scale = (f64::from(max_width) / f64::from(screen_width))
        .min(f64::from(max_height) / f64::from(screen_height))
        .min(1.0);
    let width = (((screen_width as f64 * scale).round() as u32).max(2)) & !1;
    let height = (((screen_height as f64 * scale).round() as u32).max(2)) & !1;
    if (width as usize).saturating_mul(height as usize) > MAX_RECORD_PIXELS {
        return Err("The selected recording area exceeds the supported 4K frame size.".into());
    }
    let frame_duration = TICKS_PER_SECOND / i64::from(frame_rate);
    std::fs::create_dir_all(path.parent().ok_or("Recording path is invalid")?)
        .map_err(|e| e.to_string())?;
    let output = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MFStartup(MF_VERSION, 0)
            .map_err(|_| "Media Foundation could not start the screen recorder.".to_owned())?;
    }
    let encoded = (|| -> Result<u64, String> {
        let target = unsafe { MFCreateMediaType() }
            .map_err(|_| "Windows could not configure H.264 recording.".to_owned())?;
        unsafe {
            target
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| e.to_string())?;
            target
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
                .map_err(|e| e.to_string())?;
            target
                .SetUINT32(&MF_MT_AVG_BITRATE, bit_rate)
                .map_err(|e| e.to_string())?;
            target
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(|e| e.to_string())?;
            target
                .SetUINT64(&MF_MT_FRAME_SIZE, pack_ratio(width, height))
                .map_err(|e| e.to_string())?;
            target
                .SetUINT64(&MF_MT_FRAME_RATE, pack_ratio(frame_rate, 1))
                .map_err(|e| e.to_string())?;
            target
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))
                .map_err(|e| e.to_string())?;
        }
        let input = unsafe { MFCreateMediaType() }
            .map_err(|_| "Windows could not configure screen pixels.".to_owned())?;
        unsafe {
            input
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| e.to_string())?;
            input
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|e| e.to_string())?;
            input
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(|e| e.to_string())?;
            // capture_region creates a top-down 32-bit DIB (negative biHeight).
            // Media Foundation needs the matching positive byte stride to
            // interpret rows in that same top-to-bottom order.
            input
                .SetUINT32(&MF_MT_DEFAULT_STRIDE, width.saturating_mul(4))
                .map_err(|e| e.to_string())?;
            input
                .SetUINT64(&MF_MT_FRAME_SIZE, pack_ratio(width, height))
                .map_err(|e| e.to_string())?;
            input
                .SetUINT64(&MF_MT_FRAME_RATE, pack_ratio(frame_rate, 1))
                .map_err(|e| e.to_string())?;
            input
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_ratio(1, 1))
                .map_err(|e| e.to_string())?;
        }
        let writer = unsafe { MFCreateSinkWriterFromURL(PCWSTR(output.as_ptr()), None, None) }
            .map_err(|_| {
                "Windows could not create the MP4 file. Check the Downloads folder.".to_owned()
            })?;
        let stream = unsafe { writer.AddStream(&target) }
            .map_err(|_| "Windows could not add the video stream.".to_owned())?;
        unsafe {
            writer
                .SetInputMediaType(stream, &input, None)
                .map_err(|_| "Windows could not prepare H.264 encoding.".to_owned())?;
            writer
                .BeginWriting()
                .map_err(|_| "Windows could not begin screen recording.".to_owned())?;
        }
        let started = Instant::now();
        let frame_period = Duration::from_nanos(1_000_000_000 / u64::from(frame_rate));
        let mut next_frame = started;
        let mut capture = GdiCaptureSurface::new(width as i32, height as i32)?;
        let mut index = 0_i64;
        let mut last_sample_time = 0_i64;
        loop {
            if index > 0 {
                if started.elapsed() >= RECORD_LIMIT || stop.try_recv().is_ok() {
                    break;
                }
                if let Some(delay) = next_frame.checked_duration_since(Instant::now()) {
                    thread::sleep(delay);
                }
                if started.elapsed() >= RECORD_LIMIT || stop.try_recv().is_ok() {
                    break;
                }
            }
            let buffer_len = capture.byte_len;
            capture.capture(screen_rect_for_scaled(rect, width, height))?;
            let buffer = unsafe { MFCreateMemoryBuffer(buffer_len as u32) }
                .map_err(|_| "The screen recorder ran out of frame memory.".to_owned())?;
            let mut destination = null_mut();
            unsafe {
                buffer
                    .Lock(&mut destination, None, None)
                    .map_err(|_| "Windows could not access the video frame buffer.".to_owned())?;
            }
            if destination.is_null() {
                let _ = unsafe { buffer.Unlock() };
                return Err("Windows returned an empty video frame.".into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(capture.bits.cast::<u8>(), destination, buffer_len);
                buffer
                    .Unlock()
                    .map_err(|_| "Windows could not release the video frame buffer.".to_owned())?;
                buffer
                    .SetCurrentLength(buffer_len as u32)
                    .map_err(|_| "Windows rejected the video frame size.".to_owned())?;
            }
            let sample = unsafe { MFCreateSample() }
                .map_err(|_| "Windows could not create a video sample.".to_owned())?;
            let clock_time = started
                .elapsed()
                .as_nanos()
                .saturating_mul(TICKS_PER_SECOND as u128)
                / 1_000_000_000;
            // Keep timestamps tied to elapsed wall time. If capture or encode
            // is slower than the selected frame rate, the video runs at real
            // speed with dropped frame opportunities instead of drifting.
            let sample_time = (clock_time.min(i64::MAX as u128) as i64).max(if index == 0 {
                0
            } else {
                last_sample_time.saturating_add(1)
            });
            unsafe {
                sample
                    .AddBuffer(&buffer)
                    .map_err(|_| "Windows could not attach the video frame.".to_owned())?;
                sample
                    .SetSampleTime(sample_time)
                    .map_err(|_| "Windows could not timestamp the video frame.".to_owned())?;
                sample
                    .SetSampleDuration(frame_duration)
                    .map_err(|_| "Windows could not time the video frame.".to_owned())?;
                writer
                    .WriteSample(stream, &sample)
                    .map_err(|_| "Windows could not encode a video frame.".to_owned())?;
            }
            last_sample_time = sample_time;
            index += 1;
            next_frame = started
                + Duration::from_nanos(
                    (index as u64).saturating_mul(frame_period.as_nanos() as u64),
                );
            // Do not immediately capture multiple frames to "catch up" after
            // a slow frame. This caps sustained CPU pressure on weaker PCs.
            let now = Instant::now();
            if next_frame <= now {
                next_frame = now + frame_period;
            }
        }
        unsafe {
            writer
                .Finalize()
                .map_err(|_| "Windows could not finish the MP4 file.".to_owned())?;
        }
        if index == 0 {
            return Err("Recording stopped before the first video frame.".into());
        }
        Ok(
            last_sample_time.saturating_add(frame_duration).max(0) as u64 * 1000
                / TICKS_PER_SECOND as u64,
        )
    })();
    let _ = unsafe { MFShutdown() };
    let duration_ms = encoded?;
    let size_bytes = std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    Ok((path.to_string_lossy().into_owned(), duration_ms, size_bytes))
}

fn screen_rect_for_scaled(rect: RECT, _width: u32, _height: u32) -> RECT {
    rect
}

/// Reuses one GDI device context and DIB for the duration of a recording.
/// This avoids creating/deleting a bitmap and allocating a large Vec for each
/// frame, which matters most for 1080p and 4K captures.
struct GdiCaptureSurface {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut c_void,
    width: i32,
    height: i32,
    byte_len: usize,
}

impl GdiCaptureSurface {
    fn new(width: i32, height: i32) -> Result<Self, String> {
        if width <= 0
            || height <= 0
            || (width as usize).saturating_mul(height as usize) > MAX_RECORD_PIXELS
        {
            return Err("The selected recording area exceeds the supported frame size.".into());
        }
        let screen = unsafe { GetDC(null_mut()) };
        if screen.is_null() {
            return Err("Windows could not access the selected screen area.".into());
        }
        let memory = unsafe { CreateCompatibleDC(screen) };
        if memory.is_null() {
            unsafe {
                ReleaseDC(null_mut(), screen);
            }
            return Err("Windows could not prepare the screen recorder.".into());
        }
        let mut info = bitmap_info(width, height);
        let mut bits = null_mut();
        let bitmap = unsafe {
            CreateDIBSection(screen, &mut info, DIB_RGB_COLORS, &mut bits, null_mut(), 0)
        };
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {
                unsafe {
                    DeleteObject(bitmap as _);
                }
            }
            unsafe {
                DeleteDC(memory);
                ReleaseDC(null_mut(), screen);
            }
            return Err("Windows could not allocate the screen recorder frame.".into());
        }
        let previous = unsafe { SelectObject(memory, bitmap as _) };
        if previous.is_null() || previous as isize == -1 {
            unsafe {
                DeleteObject(bitmap as _);
                DeleteDC(memory);
                ReleaseDC(null_mut(), screen);
            }
            return Err("Windows could not initialize the screen recorder frame.".into());
        }
        Ok(Self {
            screen,
            memory,
            bitmap,
            previous,
            bits,
            width,
            height,
            byte_len: (width as usize)
                .saturating_mul(height as usize)
                .saturating_mul(4),
        })
    }

    fn capture(&mut self, rect: RECT) -> Result<(), String> {
        let copied = unsafe {
            StretchBlt(
                self.memory,
                0,
                0,
                self.width,
                self.height,
                self.screen,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SRCCOPY,
            )
        };
        if copied == 0 {
            Err("Windows could not capture the selected screen pixels.".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for GdiCaptureSurface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.previous);
            DeleteObject(self.bitmap as _);
            DeleteDC(self.memory);
            ReleaseDC(null_mut(), self.screen);
        }
    }
}

fn capture_region(rect: RECT, width: i32, height: i32) -> Result<Vec<u8>, String> {
    if width <= 0
        || height <= 0
        || (width as usize).saturating_mul(height as usize) > MAX_CAPTURE_PIXELS
    {
        return Err("The selected screen area is too large to capture.".into());
    }
    let screen = unsafe { GetDC(null_mut()) };
    if screen.is_null() {
        return Err("Windows could not access the selected screen area.".into());
    }
    let memory = unsafe { CreateCompatibleDC(screen) };
    let mut info = bitmap_info(width, height);
    let mut bits: *mut c_void = null_mut();
    let bitmap =
        unsafe { CreateDIBSection(screen, &mut info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
    if memory.is_null() || bitmap.is_null() || bits.is_null() {
        if !bitmap.is_null() {
            unsafe {
                DeleteObject(bitmap as _);
            }
        }
        if !memory.is_null() {
            unsafe {
                DeleteDC(memory);
            }
        }
        unsafe {
            ReleaseDC(null_mut(), screen);
        }
        return Err("Windows could not allocate the selected screen pixels.".into());
    }
    let previous = unsafe { SelectObject(memory, bitmap as _) };
    let copied = unsafe {
        StretchBlt(
            memory,
            0,
            0,
            width,
            height,
            screen,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SRCCOPY,
        )
    } != 0;
    let len = width as usize * height as usize * 4;
    let pixels = if copied {
        unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len).to_vec() }
    } else {
        Vec::new()
    };
    unsafe {
        SelectObject(memory, previous);
        DeleteObject(bitmap as _);
        DeleteDC(memory);
        ReleaseDC(null_mut(), screen);
    }
    if copied {
        Ok(pixels)
    } else {
        Err("Windows could not capture the selected screen pixels.".into())
    }
}

fn encode_png_region(capture: &Capture, rect: RECT, mode: Mode) -> Result<Vec<u8>, String> {
    let left = (rect.left - capture.origin.x).clamp(0, capture.width - 1);
    let top = (rect.top - capture.origin.y).clamp(0, capture.height - 1);
    let right = (rect.right - capture.origin.x).clamp(left + 1, capture.width);
    let bottom = (rect.bottom - capture.origin.y).clamp(top + 1, capture.height);
    let width = (right - left) as usize;
    let height = (bottom - top) as usize;
    if width.saturating_mul(height) > MAX_CAPTURE_PIXELS {
        return Err("The selected area is too large to save.".into());
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in top as usize..bottom as usize {
        let row = y * capture.width as usize * 4;
        for x in left as usize..right as usize {
            let offset = row + x * 4;
            rgba.extend_from_slice(&[
                capture.pixels[offset + 2],
                capture.pixels[offset + 1],
                capture.pixels[offset],
                255,
            ]);
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "Island could not encode the selected image.".to_owned())?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| "Island could not finish the selected image.".to_owned())?;
    }
    let limit = if mode == Mode::Qr {
        MAX_QR_IMAGE_BYTES
    } else {
        MAX_IMAGE_BYTES
    };
    if output.len() > limit {
        return Err("The selected image is too large. Choose a smaller area.".into());
    }
    Ok(output)
}

fn encode_preview_png(
    capture: &Capture,
    rect: RECT,
    max_width: u32,
    max_height: u32,
) -> Result<Vec<u8>, String> {
    let left = (rect.left - capture.origin.x).clamp(0, capture.width - 1) as usize;
    let top = (rect.top - capture.origin.y).clamp(0, capture.height - 1) as usize;
    let right = (rect.right - capture.origin.x).clamp(left as i32 + 1, capture.width) as usize;
    let bottom = (rect.bottom - capture.origin.y).clamp(top as i32 + 1, capture.height) as usize;
    let source_width = right - left;
    let source_height = bottom - top;
    let scale = (max_width as f64 / source_width as f64)
        .min(max_height as f64 / source_height as f64)
        .min(1.0);
    let width = ((source_width as f64 * scale).round() as usize).max(1);
    let height = ((source_height as f64 * scale).round() as usize).max(1);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let source_y = top + ((y as f64 + 0.5) * source_height as f64 / height as f64) as usize;
        let source_y = source_y.min(bottom - 1);
        for x in 0..width {
            let source_x = left + ((x as f64 + 0.5) * source_width as f64 / width as f64) as usize;
            let source_x = source_x.min(right - 1);
            let offset = (source_y * capture.width as usize + source_x) * 4;
            rgba.extend_from_slice(&[
                capture.pixels[offset + 2],
                capture.pixels[offset + 1],
                capture.pixels[offset],
                255,
            ]);
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "Island could not prepare the screenshot preview.".to_owned())?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| "Island could not encode the screenshot preview.".to_owned())?;
    }
    Ok(output)
}

fn sample_bgra(pixels: &[u8], pixel_index: usize) -> (u8, u8, u8) {
    let index = pixel_index.saturating_mul(4);
    if pixels.len() < index + 4 {
        return (0, 0, 0);
    }
    (pixels[index + 2], pixels[index + 1], pixels[index])
}

fn cursor_in_overlay(capture: &Capture) -> Option<POINT> {
    let mut point = POINT::default();
    if unsafe { GetPhysicalCursorPos(&mut point) } == 0 {
        return None;
    }
    Some(POINT {
        x: (point.x - capture.origin.x).clamp(0, capture.width.saturating_sub(1)),
        y: (point.y - capture.origin.y).clamp(0, capture.height.saturating_sub(1)),
    })
}

fn update_color_patch(overlay: &mut Overlay) -> bool {
    if overlay.color_sampled_at.elapsed() < Duration::from_millis(24) {
        return false;
    }
    overlay.color_sampled_at = Instant::now();
    let center_x = overlay.capture.origin.x + overlay.cursor.x;
    let center_y = overlay.capture.origin.y + overlay.cursor.y;
    let left = (center_x - 5).clamp(
        overlay.capture.origin.x,
        overlay.capture.origin.x + overlay.capture.width - 11,
    );
    let top = (center_y - 5).clamp(
        overlay.capture.origin.y,
        overlay.capture.origin.y + overlay.capture.height - 11,
    );
    let rect = RECT {
        left,
        top,
        right: left + 11,
        bottom: top + 11,
    };
    if let Ok(pixels) = capture_region(rect, 11, 11) {
        overlay.color_patch = Some(Capture {
            origin: POINT { x: left, y: top },
            width: 11,
            height: 11,
            pixels,
        });
        true
    } else {
        false
    }
}

fn paint(hwnd: HWND, overlay: &mut Overlay) {
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    if dc.is_null() {
        return;
    }
    // Black is the layered window's transparency key. Only the active guides
    // and floating picker are painted, so the desktop stays live and unchanged.
    let clear = unsafe { CreateSolidBrush(TRANSPARENT_COLOR_KEY) };
    unsafe {
        FillRect(dc, &paint.rcPaint, clear);
        DeleteObject(clear as _);
    }
    if overlay.mode != Mode::Color {
        if let Some(rect) = overlay.selection {
            if rect.right > rect.left && rect.bottom > rect.top {
                draw_selection_border(dc, rect);
                draw_selection_dimensions(dc, rect);
            }
        }
    }
    unsafe {
        EndPaint(hwnd, &paint);
    }
}

fn paint_color_preview(hwnd: HWND, overlay: &Overlay) {
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    if dc.is_null() {
        return;
    }
    let clear = unsafe { CreateSolidBrush(TRANSPARENT_COLOR_KEY) };
    unsafe {
        FillRect(dc, &paint.rcPaint, clear);
        DeleteObject(clear as _);
    }
    draw_color_preview(dc, overlay);
    unsafe {
        EndPaint(hwnd, &paint);
    }
}

fn draw_selection_border(dc: windows_sys::Win32::Graphics::Gdi::HDC, rect: RECT) {
    let shadow = unsafe { CreatePen(0, 4, color_ref(18, 20, 24)) };
    let old_pen = unsafe { SelectObject(dc, shadow as _) };
    let old_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    unsafe {
        Rectangle(dc, rect.left, rect.top, rect.right, rect.bottom);
    }
    let pen = unsafe { CreatePen(0, 2, color_ref(255, 255, 255)) };
    unsafe {
        SelectObject(dc, pen as _);
        Rectangle(dc, rect.left, rect.top, rect.right, rect.bottom);
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(pen as _);
        DeleteObject(shadow as _);
    }
}

fn draw_selection_dimensions(dc: HDC, rect: RECT) {
    let width = (rect.right - rect.left).max(0);
    let height = (rect.bottom - rect.top).max(0);
    let text = format!("{width} × {height} px");
    let panel = selection_dimensions_rect(rect);
    draw_panel(dc, panel, color_ref(20, 21, 25), color_ref(66, 68, 77));
    draw_text(
        dc,
        &text,
        panel.left + 7,
        panel.top + 4,
        color_ref(245, 245, 248),
    );
}

fn selection_dimensions_rect(rect: RECT) -> RECT {
    let left = (rect.left + 8).min(rect.right - 126).max(8);
    let top = if rect.top >= 32 {
        rect.top - 28
    } else {
        rect.bottom + 6
    };
    RECT {
        left,
        top,
        right: left + 118,
        bottom: top + 22,
    }
}

fn invalidate_selection_visuals(hwnd: HWND, rect: RECT) {
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return;
    }
    let pad = 3;
    invalidate(
        hwnd,
        RECT {
            left: rect.left - pad,
            top: rect.top - pad,
            right: rect.right + pad,
            bottom: rect.top + pad,
        },
    );
    invalidate(
        hwnd,
        RECT {
            left: rect.left - pad,
            top: rect.bottom - pad,
            right: rect.right + pad,
            bottom: rect.bottom + pad,
        },
    );
    invalidate(
        hwnd,
        RECT {
            left: rect.left - pad,
            top: rect.top - pad,
            right: rect.left + pad,
            bottom: rect.bottom + pad,
        },
    );
    invalidate(
        hwnd,
        RECT {
            left: rect.right - pad,
            top: rect.top - pad,
            right: rect.right + pad,
            bottom: rect.bottom + pad,
        },
    );
    let panel = selection_dimensions_rect(rect);
    invalidate(
        hwnd,
        RECT {
            left: panel.left - 2,
            top: panel.top - 2,
            right: panel.right + 2,
            bottom: panel.bottom + 2,
        },
    );
}

fn draw_text(dc: windows_sys::Win32::Graphics::Gdi::HDC, text: &str, x: i32, y: i32, color: u32) {
    let wide = wide(text);
    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    let old = unsafe { SelectObject(dc, font) };
    unsafe {
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        TextOutW(dc, x, y, wide.as_ptr(), wide.len().saturating_sub(1) as i32);
        SelectObject(dc, old);
    }
}

fn draw_color_preview(dc: windows_sys::Win32::Graphics::Gdi::HDC, overlay: &Overlay) {
    let color = overlay.color_patch.as_ref();
    let (r, g, b) = color
        .map(|patch| sample_bgra(&patch.pixels, 60))
        .unwrap_or((0, 0, 0));
    let left = 0;
    let top = 0;
    let rect = RECT {
        left: 1,
        top: 1,
        right: COLOR_PREVIEW_WIDTH - 1,
        bottom: COLOR_PREVIEW_HEIGHT - 1,
    };
    draw_panel(dc, rect, color_ref(20, 21, 25), color_ref(66, 68, 77));
    draw_text(
        dc,
        "COLOR PICKER",
        left + 16,
        top + 13,
        color_ref(156, 158, 168),
    );
    let swatch = RECT {
        left: left + 16,
        top: top + 36,
        right: left + 72,
        bottom: top + 92,
    };
    draw_panel(dc, swatch, color_ref(r, g, b), color_ref(255, 255, 255));
    draw_text(dc, "HEX", left + 88, top + 34, color_ref(156, 158, 168));
    draw_text(
        dc,
        &format!("#{r:02X}{g:02X}{b:02X}"),
        left + 88,
        top + 52,
        color_ref(248, 248, 250),
    );
    draw_text(dc, "RGB", left + 88, top + 84, color_ref(156, 158, 168));
    draw_text(
        dc,
        &format!("{r}, {g}, {b}"),
        left + 88,
        top + 102,
        color_ref(210, 212, 220),
    );
    let magnifier = RECT {
        left: left + 16,
        top: top + 102,
        right: left + 72,
        bottom: top + 158,
    };
    if let Some(patch) = color {
        let mut info = bitmap_info(patch.width, patch.height);
        unsafe {
            StretchDIBits(
                dc,
                magnifier.left,
                magnifier.top,
                magnifier.right - magnifier.left,
                magnifier.bottom - magnifier.top,
                0,
                0,
                patch.width,
                patch.height,
                patch.pixels.as_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
    let border = unsafe { CreatePen(0, 1, color_ref(92, 94, 104)) };
    let old_pen = unsafe { SelectObject(dc, border as _) };
    let old_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    unsafe {
        Rectangle(
            dc,
            magnifier.left,
            magnifier.top,
            magnifier.right,
            magnifier.bottom,
        );
        SelectObject(dc, old_pen);
        SelectObject(dc, old_brush);
        DeleteObject(border as _);
    }
    let cell = (magnifier.right - magnifier.left) / 11;
    let center = RECT {
        left: magnifier.left + cell * 5,
        top: magnifier.top + cell * 5,
        right: magnifier.left + cell * 6,
        bottom: magnifier.top + cell * 6,
    };
    let center_pen = unsafe { CreatePen(0, 2, color_ref(255, 255, 255)) };
    let old_center_pen = unsafe { SelectObject(dc, center_pen as _) };
    let old_center_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    unsafe {
        Rectangle(dc, center.left, center.top, center.right, center.bottom);
        SelectObject(dc, old_center_pen);
        SelectObject(dc, old_center_brush);
        DeleteObject(center_pen as _);
    }
    draw_text(
        dc,
        "Click to copy  ·  Esc to cancel",
        left + 88,
        top + 145,
        color_ref(170, 172, 181),
    );
}

fn rect_from_points(a: POINT, b: POINT) -> RECT {
    RECT {
        left: a.x.min(b.x),
        top: a.y.min(b.y),
        right: a.x.max(b.x),
        bottom: a.y.max(b.y),
    }
}
fn color_ref(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}
fn invalidate(hwnd: HWND, rect: RECT) {
    unsafe {
        InvalidateRect(hwnd, &rect, 0);
    }
}
fn color_preview_rect(capture: &Capture, point: POINT) -> RECT {
    let width = COLOR_PREVIEW_WIDTH;
    let height = COLOR_PREVIEW_HEIGHT;
    let left = if point.x + 28 + width < capture.width {
        point.x + 28
    } else {
        point.x - width - 24
    }
    .clamp(8, (capture.width - width - 8).max(8));
    let top = if point.y + 28 + height < capture.height {
        point.y + 28
    } else {
        point.y - height - 24
    }
    .clamp(12, (capture.height - height - 12).max(12));
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn draw_panel(dc: HDC, rect: RECT, fill: u32, border_color: u32) {
    let brush = unsafe { CreateSolidBrush(fill) };
    let pen = unsafe { CreatePen(0, 1, border_color) };
    let old_brush = unsafe { SelectObject(dc, brush as _) };
    let old_pen = unsafe { SelectObject(dc, pen as _) };
    unsafe {
        RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, 12, 12);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);
    }
}
fn point_from_lparam(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam as u32 as u16 as i16) as i32,
        y: ((lparam as u32 >> 16) as u16 as i16) as i32,
    }
}
fn pack_ratio(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn downloads_path(prefix: &str, extension: &str) -> Result<std::path::PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .ok_or("The user profile folder is unavailable")?;
    let downloads = home.join("Downloads");
    std::fs::create_dir_all(&downloads)
        .map_err(|_| "Island could not open the Downloads folder.".to_owned())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(downloads.join(format!("{prefix}-{stamp}.{extension}")))
}

pub fn save_png(png_bytes: &[u8]) -> Result<String, String> {
    let path = downloads_path("Island-capture", "png")?;
    std::fs::write(&path, png_bytes)
        .map_err(|_| "Island could not save the screenshot in Downloads.".to_owned())?;
    Ok(path.to_string_lossy().into_owned())
}
