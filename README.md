# Voxa

Press a shortcut in any app, talk, and your words are written out by your own ChatGPT account and
pasted where your cursor is.

One Rust codebase for Windows and macOS, plus the Android app. The earlier Electron app is in git
history (removed in `110ea43`); its releases stay on the Releases page.

## Layout

```
core/            shared Rust, used by every app
  voxa-core        the dictation flow (a tested state machine), strip wording, trimming, fillers
  voxa-chatgpt     ChatGPT session, account and transcription client
  voxa-vad         Silero voice detection (cuts long silences before upload)
desktop/
  app/             the Windows + macOS app (GPUI): strip, tray, Home, History, Insights, Settings
    src/win.rs       Windows-only code   } the only per-OS files; the build picks one
    src/mac.rs       macOS-only code     } (mod os in main.rs), everything else is shared
  signin/          the ChatGPT sign-in window (system WebView)
  windows/         Windows packaging: installer (Inno Setup), ONNX runtime download
  macos/           macOS packaging: Info.plist, entitlements, .app and .dmg script
android/         Android app and keyboard (Kotlin; will share core/ through UniFFI)
docs/            flow audit, design mockups, reviews, measurements
```

## Build

Windows:
```
cargo build --release -p voxa-desktop -p voxa-signin
desktop\windows\fetch-onnxruntime.ps1     # once, for silence trimming
ISCC desktop\windows\voxa.iss             # installer -> target\dist
```

macOS (on a Mac): `bash desktop/macos/package.sh` -> `target/dist/Voxa-<version>-mac.dmg`

Tests: `cargo test --workspace`. GitHub Actions (`.github/workflows/desktop.yml`) builds and tests
both on every push to `main`; pushing a tag `v*` publishes a release with the Windows installer and
the Mac .dmg.
