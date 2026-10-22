# Contributing

Thanks for helping improve Island Desktop. The project is an early Windows alpha, so small, focused changes with clear reproduction steps are easiest to review.

## Before opening a change

1. Open an issue for substantial behavior changes and agree on the expected behavior first.
2. Keep OS integration in Rust and interface rendering in `island-ui`; preserve the existing IPC boundary and keep secrets out of WebView state.
3. Update tests and user-facing docs when behavior changes. Do not commit build outputs other than the checked-in embedded UI bundle.
4. Run the checks below on Windows and describe any check that you could not run.

```powershell
cargo fmt --all -- --check
cargo test --locked --all-targets --features windows-shell-probe,ui-host-probe,windows-media-probe
cargo clippy --locked --all-targets --features windows-shell-probe,ui-host-probe,windows-media-probe -- -D warnings
cd island-ui
npm ci
node --test tests/agent-navigation.test.mjs tests/voice-capture.test.mjs tests/voice-submit.test.mjs
npm run build
```

The build updates `island-ui/dist/index.html`; include it when the UI source changes.

## Issues and pull requests

Use the issue templates. Include app version or commit, Windows version, concise reproduction steps, and expected versus actual behavior. Redact private content from logs and screenshots. Pull requests should explain user-visible behavior, implementation boundaries, and test results.

By submitting a contribution, you agree that it may be distributed under the repository's MIT License. Contributions remain subject to the licenses of any third-party material they include.
