#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    StartupFailed,
    TrayUnavailable,
    ShellUnavailable,
    DpiAwarenessUnavailable,
    ForegroundRestoreDenied,
    SettingsUnavailable,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartupFailed => "startup_failed",
            Self::TrayUnavailable => "tray_unavailable",
            Self::ShellUnavailable => "shell_unavailable",
            Self::DpiAwarenessUnavailable => "dpi_awareness_unavailable",
            Self::ForegroundRestoreDenied => "foreground_restore_denied",
            Self::SettingsUnavailable => "settings_unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticCode;

    #[test]
    fn diagnostics_use_only_fixed_codes() {
        let codes = [
            DiagnosticCode::StartupFailed,
            DiagnosticCode::TrayUnavailable,
            DiagnosticCode::ShellUnavailable,
            DiagnosticCode::DpiAwarenessUnavailable,
            DiagnosticCode::ForegroundRestoreDenied,
            DiagnosticCode::SettingsUnavailable,
        ];
        for code in codes {
            let text = code.as_str();
            assert!(!text.contains("prompt"));
            assert!(!text.contains("C:\\"));
            assert!(!text.contains("private marker"));
        }
    }
}
