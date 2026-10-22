# Privacy and data handling

Island Desktop runs as a local Windows app. Features that use microphone, screen, or files are subject to the Windows permissions and settings for the current user.

## Local storage

App preferences and local workspace state are stored under `%LOCALAPPDATA%\Island`. MCP credentials and the speech API key are encrypted with Windows DPAPI for the current user. Secret values are not included in the interface state sent to the embedded WebView.

## Speech and agents

Wake phrase matching is performed by Windows speech recognition while the wake feature is enabled. When a phrase is matched, the captured question is transcribed through the speech endpoint configured in Settings. The resulting text is sent to the selected OpenCode or Codex ACP provider. Those providers may send prompts to their own configured services; their account, network, and data-retention rules apply.

Ask and dictation capture audio only while the user invokes those controls. Transcription audio is sent to the configured speech endpoint. Review that provider's retention and privacy terms before entering sensitive speech.

## Screen, camera, and files

Capture, OCR, QR, and color tools operate on the screen region selected by the user. Dictation and wake phrase recognition use the microphone only in their active state. File conversion reads selected files and writes output to the chosen destination. Optional FFmpeg downloads use the configured external distributor.

## Network behavior

The app may contact the selected speech service, OpenCode/Codex provider endpoints, update sources, weather/search services, and the FFmpeg distributor when the corresponding feature is used or configured. The repository does not currently publish a complete network-endpoint inventory. Provider binaries and services can have their own telemetry and retention behavior.

Please report any data flow that differs from this description through the private process in [Security](../SECURITY.md).
