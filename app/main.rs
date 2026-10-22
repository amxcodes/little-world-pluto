#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod agent_acp;
#[cfg(windows)]
mod agent_presentation;
#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod conversion;
#[cfg(windows)]
mod diagnostics;
mod opencode_control;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod product;
#[cfg(windows)]
mod shortcuts;
#[cfg(windows)]
mod storage;

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        platform::windows::report_diagnostic(diagnostics::DiagnosticCode::StartupFailed);
        platform::windows::report_startup_error(&error.to_string());
    }
}

#[cfg(windows)]
fn run() -> eframe::Result {
    use eframe::egui;

    if !platform::windows::enable_per_monitor_dpi_awareness() {
        platform::windows::report_diagnostic(diagnostics::DiagnosticCode::DpiAwarenessUnavailable);
    }

    // eframe owns the Windows event dispatcher. Keep its root HWND alive but
    // out of view; the React WebView in the shell is the only product UI.
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("Island host")
            .with_inner_size([1.0, 1.0])
            .with_min_inner_size([1.0, 1.0])
            .with_position([-32000.0, -32000.0])
            .with_decorations(false)
            .with_visible(true),
        ..Default::default()
    };

    eframe::run_native(
        "Island host",
        options,
        Box::new(|context| Ok(Box::new(product::IslandApp::new(context)))),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Island Desktop's current product build targets Windows only.");
}
