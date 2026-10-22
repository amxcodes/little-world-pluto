pub mod agent_library;
pub mod privacy;
pub mod screen_ocr;
pub mod screen_overlay;
pub mod screen_qr;
#[allow(dead_code)] // DPAPI is verified now; credential controls arrive with their owning module.
pub mod secrets;
pub mod shell;
pub mod speech;
pub mod startup;
pub mod tray;
pub mod wake_word;

use crate::diagnostics::DiagnosticCode;
use windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_PER_MONITOR_AWARE,
    GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

pub fn report_startup_error(message: &str) {
    let title = wide("Island failed to start");
    let body = wide(message);
    // SAFETY: both strings are nul-terminated UTF-16 buffers that live through
    // the call; a null owner is valid for a standalone error dialog.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn enable_per_monitor_dpi_awareness() -> bool {
    // This runs before eframe creates any HWNDs; failure is a diagnostic because
    // an embedding host may already have selected the process DPI context.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } != 0 {
        return true;
    }
    let current = unsafe { GetThreadDpiAwarenessContext() };
    unsafe { GetAwarenessFromDpiAwarenessContext(current) == DPI_AWARENESS_PER_MONITOR_AWARE }
}

pub fn report_diagnostic(code: DiagnosticCode) {
    let message: Vec<u16> = format!("Island diagnostic: {}\n", code.as_str())
        .encode_utf16()
        .chain([0])
        .collect();
    // SAFETY: the UTF-16 buffer is nul-terminated and remains alive for the call.
    unsafe { OutputDebugStringW(message.as_ptr()) };
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
