# Island Desktop

Island Desktop is a Windows app that keeps everyday tools and AI coding agents in a compact, always-available workspace. It is an early alpha: expect rough edges, changing behavior, and incomplete Windows packaging and accessibility checks.

The app combines a small Rust host with an embedded React interface. It includes notes and reminders, media and system controls, capture and OCR tools, dictation, file conversion, and ACP sessions for OpenCode and Codex. Optional features depend on Windows capabilities and separately installed tools.

## Build from source

Use Windows 10 or 11, the Rust toolchain selected by `rust-toolchain.toml`, Node.js 22 or newer, and npm. WebView2 Runtime is required to display the interface. For MSIX packaging, install the Windows 10/11 SDK with MakeAppx and SignTool.

```powershell
cd island-ui
npm ci
npm run build
cd ..
cargo build --locked --release --features product-app --bin island-desktop
```

The executable is `target/release/island-desktop.exe`. To create a local developer MSIX, fetch the locked Rust dependencies first, then run:

```powershell
cargo fetch --locked
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/phase1/package_windows.ps1
```

The packager creates a short-lived self-signed developer package under `target/windows-package/`. It does not install the package and its certificate is not a trusted production signing identity. See [Windows packaging](packaging/windows/README.md).

## Development checks

```powershell
cargo fmt --all -- --check
cargo test --locked --all-targets --features windows-shell-probe,ui-host-probe,windows-media-probe
cargo clippy --locked --all-targets --features windows-shell-probe,ui-host-probe,windows-media-probe -- -D warnings
cd island-ui
node --test tests/agent-navigation.test.mjs tests/voice-capture.test.mjs tests/voice-submit.test.mjs
npm run build
```

CI runs these Rust checks and the frontend tests/build on Windows. See [Contributing](CONTRIBUTING.md) before opening a pull request.

## Privacy and external tools

The app stores local preferences and state under `%LOCALAPPDATA%\Island`. MCP and speech credentials are protected with Windows DPAPI. Speech transcription uses the endpoint configured by the user; agent prompts go through the selected local provider integration. Wake phrase recognition uses Windows speech recognition while enabled. Screen capture, microphone, and file conversion features run only when invoked or enabled. See [Privacy](docs/PRIVACY.md) for data-flow details.

OpenCode and Codex sessions require their respective provider tools. FFmpeg is optional and managed separately from the app package. See [Dependencies](DEPENDENCIES.md) and [third-party notices](island-ui/THIRD_PARTY_NOTICES.md).

## Alpha status

Windows x64 is the current release target. Linux and macOS are not supported. The developer MSIX is not production signed, and a trusted installer/update channel is not provided yet. App integrations can vary with Windows version, provider version, installed codecs, and permissions. Please report reproducible issues with the template and remove private prompts, paths, credentials, and screen/audio data before attaching logs.

## License

Island Desktop source is licensed under the MIT License. Third-party components retain their own licenses; see the included notices.
