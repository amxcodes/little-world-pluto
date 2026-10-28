//! The invisible Windows host for the Pluto interface. All visible controls live
//! in the shell WebView; this eframe viewport only keeps its event dispatcher
//! alive for the tray, settings persistence, and shell callbacks.

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::diagnostics::DiagnosticCode;
use crate::platform::windows::shell::{ShellController, ShellPreferenceChange, ShellStartup};
use crate::platform::windows::tray::TrayController;
use crate::shortcuts::ShortcutAction;
use crate::storage::{SettingsStore, StoredSettings, StoredTheme};

pub struct PlutoApp {
    shell: Option<ShellController>,
    tray: Option<TrayController>,
    settings_store: Option<SettingsStore>,
    settings: StoredSettings,
    quit_requested: bool,
    first_frame: bool,
    startup_error: Option<String>,
}

impl PlutoApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        let (settings_store, settings) = match SettingsStore::open_default() {
            Ok(store) => {
                let settings = store.load().unwrap_or_else(|_| {
                    crate::platform::windows::report_diagnostic(
                        DiagnosticCode::SettingsUnavailable,
                    );
                    StoredSettings::default()
                });
                (Some(store), settings)
            }
            Err(_) => {
                crate::platform::windows::report_diagnostic(DiagnosticCode::SettingsUnavailable);
                (None, StoredSettings::default())
            }
        };
        context.egui_ctx.set_theme(match settings.theme {
            StoredTheme::System => egui::ThemePreference::System,
            StoredTheme::Dark => egui::ThemePreference::Dark,
            StoredTheme::Light => egui::ThemePreference::Light,
        });

        let tray = match TrayController::new(&context.egui_ctx) {
            Ok(tray) => Some(tray),
            Err(_) => {
                crate::platform::windows::report_diagnostic(DiagnosticCode::TrayUnavailable);
                None
            }
        };
        let shell_startup = ShellStartup {
            shape: settings.shape,
            accent_gradient: settings.accent_gradient,
            media_card_style: settings.media_card_style,
            hover_open_delay: std::time::Duration::from_millis(u64::from(
                settings.hover_open_delay_ms,
            )),
            hover_close_delay: std::time::Duration::from_millis(u64::from(
                settings.hover_close_delay_ms,
            )),
            click_to_open: settings.click_to_open,
            close_on_outside_click: settings.close_on_outside_click,
            reduced_motion: settings.reduced_motion,
            folded_transparency_percent: settings.folded_transparency_percent,
            open_transparency_percent: settings.open_transparency_percent,
            clock_indicator_enabled: settings.clock_indicator_enabled,
            battery_indicator_enabled: settings.battery_indicator_enabled,
            privacy_indicator_enabled: settings.privacy_indicator_enabled,
            hide_in_fullscreen: settings.hide_in_fullscreen,
            exclude_from_screen_capture: settings.exclude_from_screen_capture,
            shortcuts: settings.shortcuts,
        };
        let (shell, startup_error) = match ShellController::new(&context.egui_ctx, shell_startup) {
            Ok(shell) => (Some(shell), None),
            Err(error) => {
                crate::platform::windows::report_diagnostic(DiagnosticCode::ShellUnavailable);
                (None, Some(error))
            }
        };
        Self {
            shell,
            tray,
            settings_store,
            settings,
            quit_requested: false,
            first_frame: true,
            startup_error,
        }
    }

    fn save_settings(&mut self) {
        if let Some(store) = self.settings_store.as_mut()
            && store.save(self.settings).is_err()
        {
            crate::platform::windows::report_diagnostic(DiagnosticCode::SettingsUnavailable);
        }
    }

    fn attach_windows(&self, frame: &eframe::Frame) {
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        if let RawWindowHandle::Win32(handle) = handle.as_raw() {
            let window = handle.hwnd.get() as _;
            if let Some(tray) = &self.tray {
                tray.set_root_window(window);
            }
            if let Some(shell) = &self.shell {
                shell.set_desk_window(window);
            }
        }
    }

    fn handle_shell(&mut self, context: &egui::Context) {
        let Some(shell) = &self.shell else { return };
        let mut settings_changed = false;
        for change in shell.take_preference_changes() {
            match change {
                ShellPreferenceChange::Shape(shape) => self.settings.shape = shape,
                ShellPreferenceChange::AccentGradient(value) => {
                    self.settings.accent_gradient = value
                }
                ShellPreferenceChange::MediaCardStyle(value) => {
                    self.settings.media_card_style = value
                }
                ShellPreferenceChange::Clock(value) => {
                    self.settings.clock_indicator_enabled = value
                }
                ShellPreferenceChange::Battery(value) => {
                    self.settings.battery_indicator_enabled = value
                }
                ShellPreferenceChange::Privacy(value) => {
                    self.settings.privacy_indicator_enabled = value
                }
                ShellPreferenceChange::HideFullscreen(value) => {
                    self.settings.hide_in_fullscreen = value
                }
                ShellPreferenceChange::ScreenCaptureExclusion(value) => {
                    self.settings.exclude_from_screen_capture = value
                }
                ShellPreferenceChange::ReducedMotion(value) => self.settings.reduced_motion = value,
                ShellPreferenceChange::HoverOpen(value) => {
                    self.settings.hover_open_delay_ms = value
                }
                ShellPreferenceChange::HoverClose(value) => {
                    self.settings.hover_close_delay_ms = value
                }
                ShellPreferenceChange::ClickToOpen(value) => self.settings.click_to_open = value,
                ShellPreferenceChange::CloseOnOutsideClick(value) => {
                    self.settings.close_on_outside_click = value
                }
                ShellPreferenceChange::FoldedTransparency(value) => {
                    self.settings.folded_transparency_percent = value
                }
                ShellPreferenceChange::OpenTransparency(value) => {
                    self.settings.open_transparency_percent = value
                }
            }
            settings_changed = true;
        }
        if shell.take_theme_toggle_request() {
            self.settings.theme = if context.theme() == egui::Theme::Dark {
                StoredTheme::Dark
            } else {
                StoredTheme::Light
            };
            settings_changed = true;
        }
        if shell
            .set_dark_theme(context.theme() == egui::Theme::Dark)
            .is_err()
            || shell
                .set_reduced_motion(self.settings.reduced_motion)
                .is_err()
            || shell.take_animation_failure()
        {
            crate::platform::windows::report_diagnostic(DiagnosticCode::ShellUnavailable);
        }
        let _ = shell.take_toggle_request();
        for action in shell.take_shortcut_requests() {
            if action == ShortcutAction::CommandBar {
                let _ = shell.show_command();
            }
        }
        if settings_changed {
            self.save_settings();
        }
    }

    fn handle_tray(&mut self, context: &egui::Context) {
        let (show, focus_target, quit) = self
            .tray
            .as_ref()
            .map(|tray| {
                let (show, focus_target) = tray.take_show_request();
                (show, focus_target, tray.take_quit_request())
            })
            .unwrap_or_default();
        if show && let Some(shell) = &self.shell {
            if let Some(target) = focus_target {
                let _ = shell.set_focus_restore_target(target);
            }
            let _ = shell.show_settings();
        }
        if quit {
            self.quit_requested = true;
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn handle_close(&mut self, context: &egui::Context) {
        if self.quit_requested || !context.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.tray.is_some() {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }
}

impl Drop for PlutoApp {
    fn drop(&mut self) {
        // Stop the shell window before destroying the tray window.
        drop(self.shell.take());
    }
}

impl eframe::App for PlutoApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            self.attach_windows(frame);
            if let Some(error) = self.startup_error.take() {
                crate::platform::windows::report_startup_error(&error);
                self.quit_requested = true;
                context.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.handle_shell(&context);
        self.handle_tray(&context);
        self.handle_close(&context);
        context.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
