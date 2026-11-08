#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod agent_acp;
#[cfg(windows)]
mod agent_presentation;
#[cfg(windows)]
mod agent_sessions;
#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod conversion;
#[cfg(windows)]
mod diagnostics;
#[cfg(windows)]
mod meetings;
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
mod update;
mod visual_references;

#[cfg(windows)]
fn main() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            let folder = std::path::PathBuf::from(root).join("Pluto").join("logs");
            let _ = std::fs::create_dir_all(&folder);
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(folder.join("panic.log"))
            {
                let _ = writeln!(
                    file,
                    "{info}\n{}",
                    std::backtrace::Backtrace::force_capture()
                );
            }
        }
        previous(info);
    }));
    if std::env::args().any(|arg| arg == "--check-wake-listener") {
        if let Err(error) = platform::windows::check_windows_wake_listener() {
            eprintln!("{error}");
            std::process::exit(2);
        }
        println!("Windows wake listening and shutdown passed.");
        return;
    }
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
            .with_title("Pluto host")
            .with_inner_size([1.0, 1.0])
            .with_min_inner_size([1.0, 1.0])
            .with_position([-32000.0, -32000.0])
            .with_decorations(false)
            .with_visible(true),
        ..Default::default()
    };

    eframe::run_native(
        "Pluto host",
        options,
        Box::new(|context| Ok(Box::new(product::PlutoApp::new(context)))),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Pluto's current product build targets Windows only.");
}
