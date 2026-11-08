use std::borrow::Cow;
use std::ptr::null_mut;
use std::time::Instant;

use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateRoundRectRgn, DeleteObject, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow, SetWindowRgn,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowRect, HWND_TOPMOST, IsWindow, LWA_ALPHA, PostMessageW,
    SW_SHOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetForegroundWindow, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, WM_CLOSE, WM_NCDESTROY, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
};
use wry::WebViewBuilderExtWindows;

use super::shell::{ChildWindowHandle, WM_APP_ANSWER_CLOSE};

const ANSWER_SUBCLASS_ID: usize = 0x504C_5554;
const DWMWA_USE_IMMERSIVE_DARK_MODE_WIN10: u32 = 19;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_TEXT_COLOR: u32 = 36;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: i32 = 2;

const fn colorref(red: u8, green: u8, blue: u8) -> u32 {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

unsafe extern "system" fn answer_window_subclass(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass_id: usize,
    reference_data: usize,
) -> isize {
    if message == WM_CLOSE {
        let owner = reference_data as HWND;
        if !owner.is_null()
            && unsafe { PostMessageW(owner, WM_APP_ANSWER_CLOSE, window as usize, 0) } != 0
        {
            return 0;
        }
    }
    if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(window, Some(answer_window_subclass), subclass_id) };
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

pub(super) struct AnswerWindow {
    view: Option<wry::WebView>,
    bloom: Option<Bloom>,
    window: HWND,
    session_id: String,
    dark: bool,
    closing: bool,
}

impl AnswerWindow {
    pub(super) fn open(
        owner: HWND,
        answer: &serde_json::Value,
        animate: bool,
        exclude_capture: bool,
    ) -> Result<Self, String> {
        let monitor = unsafe { MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err("Could not locate the answer window's monitor".into());
        }
        let RECT {
            left,
            top,
            right,
            bottom,
        } = info.rcWork;
        let width = (right - left).min(1120);
        let height = (bottom - top).min(860);
        let window = unsafe {
            CreateWindowExW(
                0,
                windows_startup::core::w!("STATIC").as_ptr(),
                windows_startup::core::w!("Pluto — Full view").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                left + (right - left - width) / 2,
                top + (bottom - top - height) / 2,
                width,
                height,
                owner,
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if window.is_null() {
            return Err("Could not create the answer window".into());
        }
        let dark = answer["dark"].as_bool().unwrap_or(true);
        let dark_title_bar = i32::from(dark);
        let rounded = DWMWCP_ROUND;
        // DWM's immersive flag alone can leave a light caption on systems
        // where the user's Windows app mode is light. Explicit caption colors
        // keep the detached viewer visually continuous with Pluto's surface.
        let (caption_color, border_color, text_color) = if dark {
            (
                colorref(0x18, 0x18, 0x18),
                colorref(0x39, 0x39, 0x40),
                colorref(0xf5, 0xf5, 0xf7),
            )
        } else {
            (
                colorref(0xf6, 0xf6, 0xf7),
                colorref(0xd9, 0xd9, 0xde),
                colorref(0x17, 0x17, 0x19),
            )
        };
        unsafe {
            // Attribute 19 is used by Windows 10 builds; 20 is the current
            // Windows 11 value. Unsupported attributes simply return failure.
            for attribute in [
                DWMWA_USE_IMMERSIVE_DARK_MODE_WIN10,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
            ] {
                DwmSetWindowAttribute(
                    window,
                    attribute,
                    (&dark_title_bar as *const i32).cast(),
                    std::mem::size_of::<i32>() as u32,
                );
            }
            DwmSetWindowAttribute(
                window,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&rounded as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
            for (attribute, color) in [
                (DWMWA_CAPTION_COLOR, caption_color),
                (DWMWA_BORDER_COLOR, border_color),
                (DWMWA_TEXT_COLOR, text_color),
            ] {
                DwmSetWindowAttribute(
                    window,
                    attribute,
                    (&color as *const u32).cast(),
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
        if unsafe {
            SetWindowSubclass(
                window,
                Some(answer_window_subclass),
                ANSWER_SUBCLASS_ID,
                owner as usize,
            )
        } == 0
        {
            unsafe { DestroyWindow(window) };
            return Err("Could not install the answer window close handler".into());
        }
        if exclude_capture
            && unsafe { SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE) } == 0
        {
            unsafe { DestroyWindow(window) };
            return Err("Could not apply screen capture exclusion to the answer window".into());
        }
        let mut result = Self {
            view: None,
            window,
            session_id: answer["sessionId"].as_str().unwrap_or_default().to_owned(),
            bloom: None,
            dark,
            closing: false,
        };
        let html = include_str!("../../../island-ui/dist/index.html");
        let worker = include_str!("../../../island-ui/dist/figure-worker.js");
        let answer_hwnd = window;
        result.view = Some(
            wry::WebViewBuilder::new()
                .with_initialization_script(format!(
                    "window.islandDetachedAnswer = {answer}; window.islandDetachedViewer = true;"
                ))
                .with_ipc_handler(move |request| {
                    // The detached WebView also hosts the procedural scene
                    // renderer. Only an explicit close command should trigger
                    // the reverse bloom; scene and playback IPC must not close it.
                    let is_close = serde_json::from_str::<serde_json::Value>(request.body())
                        .ok()
                        .and_then(|message| message["kind"].as_str().map(str::to_owned))
                        .is_some_and(|kind| kind == "closeDetachedAnswer");
                    if is_close {
                        unsafe {
                            PostMessageW(owner, WM_APP_ANSWER_CLOSE, answer_hwnd as usize, 0)
                        };
                    }
                })
                .with_custom_protocol("island".to_owned(), move |_, request| {
                    let (body, mime) = if request.uri().path() == "/figure-worker.js" {
                        (worker, "text/javascript; charset=utf-8")
                    } else {
                        (html, "text/html; charset=utf-8")
                    };
                    wry::http::Response::builder()
                        .header(wry::http::header::CONTENT_TYPE, mime)
                        .body(Cow::Borrowed(body.as_bytes()))
                        .expect("valid answer window response")
                })
                .with_navigation_handler(|url| {
                    url == "https://island.localhost/" || url == "island://localhost/"
                })
                .with_https_scheme(true)
                .with_url("island://localhost/")
                .build(&ChildWindowHandle(window))
                .map_err(|error| error.to_string())?,
        );
        if animate {
            result.bloom = Bloom::start(
                owner,
                window,
                answer["dark"].as_bool().unwrap_or(true),
                exclude_capture,
            );
        }
        if result.bloom.is_none() {
            result.reveal();
        }
        Ok(result)
    }

    fn reveal(&self) {
        unsafe {
            ShowWindow(self.window, SW_SHOW);
            SetForegroundWindow(self.window);
        }
    }

    pub(super) fn is_blooming(&self) -> bool {
        self.bloom.is_some()
    }

    pub(super) fn finish_bloom(&mut self) {
        if self.bloom.take().is_some() {
            self.reveal();
        }
    }

    pub(super) fn update(&self, session_id: &str, answer: &serde_json::Value) {
        if self.session_id != session_id || self.closing || !self.is_open() {
            return;
        }
        let Ok(payload) = serde_json::to_string(answer) else {
            return;
        };
        let script = format!(
            "window.islandDetachedAnswer={payload};window.dispatchEvent(new CustomEvent('pluto-answer-update',{{detail:window.islandDetachedAnswer}}));"
        );
        if let Some(view) = &self.view {
            let _ = view.evaluate_script(&script);
        }
    }

    pub(super) fn begin_close(
        &mut self,
        owner: HWND,
        requested_window: HWND,
        animate: bool,
        exclude_capture: bool,
    ) -> bool {
        if !self.is_open() {
            return false;
        }
        if !requested_window.is_null() && requested_window != self.window {
            return false;
        }
        if self.closing {
            return true;
        }
        // If the user closes during the opening bloom, complete that handoff
        // before starting the reverse one so neither surface remains cloaked.
        self.finish_bloom();
        self.closing = true;
        self.bloom = animate
            .then(|| Bloom::start_close(self.window, owner, self.dark, exclude_capture))
            .flatten();
        if self.bloom.is_some() {
            true
        } else {
            self.finish_close();
            false
        }
    }

    pub(super) fn finish_close(&mut self) {
        if let Some(mut bloom) = self.bloom.take() {
            bloom.uncloak();
        }
        if self.is_open() {
            unsafe { DestroyWindow(self.window) };
        }
        self.closing = false;
    }

    pub(super) fn advance_bloom(&mut self) -> bool {
        let Some(mut bloom) = self.bloom.take() else {
            return false;
        };
        if !self.is_open() {
            return false;
        }
        let elapsed = bloom.start.elapsed().as_secs_f64();
        if elapsed >= ARRIVAL_SECONDS && !bloom.revealed {
            if bloom.closing {
                bloom.reveal_destination();
            } else {
                bloom.uncloak();
                self.reveal();
            }
            bloom.revealed = true;
        }
        if elapsed >= ARRIVAL_SECONDS + FADE_SECONDS || !bloom.paint(elapsed) {
            if bloom.closing {
                bloom.uncloak();
                unsafe { DestroyWindow(self.window) };
            } else if !bloom.revealed {
                bloom.uncloak();
                self.reveal();
            }
            self.closing = false;
            return false;
        }
        if !bloom.revealed && elapsed >= COVER_SECONDS && !bloom.covered && !bloom.cover() {
            bloom.uncloak();
            if bloom.closing {
                unsafe { DestroyWindow(self.window) };
            } else {
                self.reveal();
            }
            self.closing = false;
            return false;
        }
        self.bloom = Some(bloom);
        true
    }

    pub(super) fn is_open(&self) -> bool {
        unsafe { IsWindow(self.window) != 0 }
    }
}

impl Drop for AnswerWindow {
    fn drop(&mut self) {
        self.bloom.take();
        self.view.take();
        if self.is_open() {
            unsafe { DestroyWindow(self.window) };
        }
    }
}

const COVER_SECONDS: f64 = 0.09;
const FLIGHT_START: f64 = 0.31;
const FLIGHT_SECONDS: f64 = 0.95;
const ARRIVAL_SECONDS: f64 = FLIGHT_START + FLIGHT_SECONDS;
const FADE_SECONDS: f64 = 0.30;

struct Bloom {
    carrier: HWND,
    source: RECT,
    destination: RECT,
    source_window: HWND,
    destination_window: HWND,
    start: Instant,
    source_cloaked: bool,
    destination_cloaked: bool,
    covered: bool,
    revealed: bool,
    closing: bool,
}

fn cloak(window: HWND, value: bool) -> bool {
    let enabled = i32::from(value);
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_CLOAK as u32,
            (&enabled as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        ) >= 0
    }
}

impl Bloom {
    fn start(owner: HWND, destination: HWND, dark: bool, exclude_capture: bool) -> Option<Self> {
        Self::start_transition(owner, destination, dark, exclude_capture, false)
    }

    fn start_close(
        source: HWND,
        destination: HWND,
        dark: bool,
        exclude_capture: bool,
    ) -> Option<Self> {
        Self::start_transition(source, destination, dark, exclude_capture, true)
    }

    fn start_transition(
        source_window: HWND,
        destination_window: HWND,
        dark: bool,
        exclude_capture: bool,
        closing: bool,
    ) -> Option<Self> {
        let mut source = RECT::default();
        let mut target = RECT::default();
        if unsafe { GetWindowRect(source_window, &mut source) } == 0
            || unsafe { GetWindowRect(destination_window, &mut target) } == 0
            || unsafe { MonitorFromWindow(source_window, MONITOR_DEFAULTTONEAREST) }
                != unsafe { MonitorFromWindow(destination_window, MONITOR_DEFAULTTONEAREST) }
        {
            return None;
        }
        // Standard STATIC control styles SS_BLACKRECT and SS_WHITERECT.
        let fill_style = if dark { 4 } else { 6 };
        let carrier = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
                windows_startup::core::w!("STATIC").as_ptr(),
                std::ptr::null(),
                WS_POPUP | fill_style,
                source.left,
                source.top,
                1,
                1,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if carrier.is_null() {
            return None;
        }
        let bloom = Self {
            carrier,
            source,
            destination: target,
            source_window,
            destination_window,
            start: Instant::now(),
            source_cloaked: false,
            destination_cloaked: false,
            covered: false,
            revealed: false,
            closing,
        };
        if (exclude_capture
            && unsafe { SetWindowDisplayAffinity(carrier, WDA_EXCLUDEFROMCAPTURE) } == 0)
            || !bloom.paint(0.)
        {
            return None;
        }
        Some(bloom)
    }

    fn paint(&self, elapsed: f64) -> bool {
        let (bounds, radius, alpha) = bloom_frame(self.source, self.destination, elapsed);
        let width = (bounds.right - bounds.left).max(1);
        let height = (bounds.bottom - bounds.top).max(1);
        let region = unsafe { CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius) };
        if region.is_null() {
            return false;
        }
        if unsafe { SetWindowRgn(self.carrier, region, 1) } == 0 {
            unsafe { DeleteObject(region) };
            return false;
        }
        unsafe {
            SetLayeredWindowAttributes(self.carrier, 0, alpha, LWA_ALPHA) != 0
                && SetWindowPos(
                    self.carrier,
                    HWND_TOPMOST,
                    bounds.left,
                    bounds.top,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) != 0
        }
    }

    fn cover(&mut self) -> bool {
        self.source_cloaked = cloak(self.source_window, true);
        if !self.source_cloaked {
            return false;
        }
        self.destination_cloaked = cloak(self.destination_window, true);
        if !self.destination_cloaked {
            return false;
        }
        unsafe {
            ShowWindow(self.destination_window, SW_SHOW);
            SetForegroundWindow(self.destination_window);
        }
        self.covered = true;
        true
    }

    fn reveal_destination(&mut self) {
        if self.destination_cloaked && cloak(self.destination_window, false) {
            self.destination_cloaked = false;
        }
        unsafe {
            ShowWindow(self.destination_window, SW_SHOW);
            SetForegroundWindow(self.destination_window);
        }
    }

    fn uncloak(&mut self) {
        if self.destination_cloaked && cloak(self.destination_window, false) {
            self.destination_cloaked = false;
        }
        if self.source_cloaked && cloak(self.source_window, false) {
            self.source_cloaked = false;
        }
    }
}

impl Drop for Bloom {
    fn drop(&mut self) {
        self.uncloak();
        unsafe { DestroyWindow(self.carrier) };
    }
}

fn bloom_frame(source: RECT, destination: RECT, elapsed: f64) -> (RECT, i32, u8) {
    let phase = ((elapsed - FLIGHT_START) / FLIGHT_SECONDS).clamp(0., 1.);
    let omega = 4. * std::f64::consts::PI;
    let damping = 0.74_f64;
    let frequency = omega * (1. - damping * damping).sqrt();
    let spring = if phase >= 1. {
        1.
    } else {
        1. - (-omega * damping * phase).exp()
            * ((frequency * phase).cos() + omega * damping / frequency * (frequency * phase).sin())
    };
    let drop = ((elapsed - 0.06) / 0.25).clamp(0., 1.);
    let drop = 0.08 * drop * drop * (3. - 2. * drop);
    let progress = drop + (1. - drop) * spring;
    let lerp =
        |a: i32, b: i32| (f64::from(a) + (f64::from(b) - f64::from(a)) * progress).round() as i32;
    let cover = (elapsed / COVER_SECONDS).clamp(0., 1.);
    let fade = ((elapsed - ARRIVAL_SECONDS) / FADE_SECONDS).clamp(0., 1.);
    let ease = |p: f64| p * p * (3. - 2. * p);
    let alpha = (255. * ease(cover) * (1. - ease(fade))).round() as u8;
    (
        RECT {
            left: lerp(source.left, destination.left),
            top: lerp(source.top, destination.top),
            right: lerp(source.right, destination.right),
            bottom: lerp(source.bottom, destination.bottom),
        },
        lerp(44, 14),
        alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_bloom_arrives_before_fading() {
        let source = RECT {
            left: -500,
            top: 0,
            right: -300,
            bottom: 36,
        };
        let destination = RECT {
            left: -900,
            top: 100,
            right: -20,
            bottom: 720,
        };
        let (start, _, alpha) = bloom_frame(source, destination, 0.);
        assert_eq!((start.left, start.bottom, alpha), (-500, 36, 0));
        let (arrival, _, alpha) = bloom_frame(source, destination, ARRIVAL_SECONDS);
        assert_eq!((arrival.left, arrival.bottom, alpha), (-900, 720, 255));
        let (end, _, alpha) = bloom_frame(source, destination, ARRIVAL_SECONDS + FADE_SECONDS);
        assert_eq!((end.left, end.bottom, alpha), (-900, 720, 0));
    }
}
