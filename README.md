# Voxa

Press a shortcut in any app, talk, and your words are written out by your own ChatGPT account and
pasted where your cursor is.

This repository holds the native rebuild: one Rust codebase for Windows and macOS, and the Android
app. The earlier Electron app is in git history (last commit before its removal: see `git log --
src/`) and its releases stay on the Releases page.

## Layout

| Path | What |
|---|---|
| `crates/voxa-core` | The dictation flow as a pure state machine, strip wording, silence trimming, filler removal. Shared by every app. |
| `crates/voxa-chatgpt` | ChatGPT session, account and transcription client. |
| `crates/voxa-vad` | Silero voice detection (cuts long silences before upload). |
| `apps/desktop` | The desktop app (GPUI): strip, tray, main window. `src/win.rs` and `src/mac.rs` are the only per-OS code. |
| `apps/signin` | The ChatGPT sign-in window (system WebView). |
| `installer` | Windows installer (Inno Setup). |
| `voxa-kotlin` | Android app and keyboard. |
| `docs` | Flow audit, design mockups, reviews, measurements. |

## Build

```
cargo build --release -p voxa-desktop -p voxa-signin
scripts\fetch-onnxruntime.ps1          # once, for silence trimming on Windows
ISCC installer\voxa.iss                # Windows installer -> target\dist
```

Tests: `cargo test --workspace`. Releases are built by GitHub Actions (`.github/workflows`).
