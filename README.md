# Voxa

**Talk instead of typing, in any app.** Press a shortcut, say what you want to write, and Voxa
turns it into text with your own ChatGPT account and pastes it where your cursor is: chats, email,
documents, code, Claude Code, anywhere.

Native, fast and light: the strip appears in about 30 ms and Voxa uses about 65 MB of memory.

## Download

| Platform | Get it |
|---|---|
| **Windows 10/11** | [Voxa-Setup-1.0.0.exe](https://github.com/blackscythe123/voxa/releases/download/v1.0.0/Voxa-Setup-1.0.0.exe) |
| **Mac** (Apple Silicon and Intel; not yet tested on a real Mac) | [Voxa-1.0.0-mac.dmg](https://github.com/blackscythe123/voxa/releases/download/v1.0.0/Voxa-1.0.0-mac.dmg) |
| **Android** (voice keyboard, Android 10+), works on any phone | [Voxa-Android-1.0.0-universal.apk](https://github.com/blackscythe123/voxa/releases/download/android-v1.0.0/Voxa-Android-1.0.0-universal.apk) |
| **Android**, smaller, for most phones (64-bit ARM) | [Voxa-Android-1.0.0-arm64-v8a.apk](https://github.com/blackscythe123/voxa/releases/download/android-v1.0.0/Voxa-Android-1.0.0-arm64-v8a.apk) |

Always the newest version: [Windows](https://github.com/blackscythe123/voxa/releases/latest/download/Voxa-Setup.exe)
· [Mac](https://github.com/blackscythe123/voxa/releases/latest/download/Voxa-mac.dmg)
· [latest release page](https://github.com/blackscythe123/voxa/releases/latest). All releases:
[Releases page](https://github.com/blackscythe123/voxa/releases).

- **Windows:** run the installer (no admin needed). If SmartScreen says "Windows protected your PC",
  click *More info* > *Run anyway*. It updates the old Voxa and keeps your history and sign-in.
- **Mac:** drag Voxa to Applications and open it. macOS blocks it the first time (not notarized
  yet): open **System Settings > Privacy & Security**, scroll down and click **Open Anyway**. Allow
  the microphone, and Accessibility so Voxa can paste. After an update macOS may ask for
  Accessibility once more; Voxa tells you when.
- **Android:** not sure which file? Take the universal one, it works on any phone. Open the APK
  on the phone; Android asks you to allow "Install unknown apps" for the app you opened it from
  (Files, Chrome, Drive or WhatsApp), and Play Protect may ask to scan it, which is fine. Send it as
  a file (Drive, or a WhatsApp/Telegram document); a broken copy shows "problem while parsing the
  package". If you installed a Voxa test build before 1.0.0, uninstall it once, then install
  1.0.0; test-build History isn't kept. Every version from 1.0.0 on updates in place and keeps
  your History and sign-in. Voxa asks once for permission to install its own updates.

Updates: 1.0.0 is the first version that updates itself (Windows installs new versions when you're
not using Voxa; Android downloads on Wi-Fi and asks you to tap Install; Mac tells you when one is
out). Older versions (0.4.7 and before on Windows and Mac, the Android test builds) have no
updater, so install 1.0.0 by hand once. Settings has "Check for updates" and "What's new".

You need a ChatGPT account (free works). Voxa signs in on ChatGPT's own page and never sees your
password. It uses your ChatGPT web session (not an official API) and keeps that session's cookies
on your device so you stay signed in.

## What it does

- **Dictate anywhere** with one shortcut, or **hold to talk**. A small strip shows that it is
  listening, writing, and where it pasted.
- **Knows where you are**: real app and website icons (Gmail, ChatGPT, WhatsApp...), and Claude
  Code inside VS Code.
- **Never loses what you said**: long pauses are trimmed so long dictations go through; offline or
  signed-out dictations wait and send themselves; if you switch apps it asks before pasting, and
  going back pastes where you started.
- **History**: search, filter by app or site, play back, copy, edit, send again.
- **Words**: fix names ChatGPT keeps getting wrong, snippets ("my email"), spoken "new line", and
  per-app rules (no full stop at the end in chats).
- **Insights**: words, time saved, speaking speed and streaks for any date range.
- **A guided tour** that has you do each step for real on first launch.

**Privacy:** audio goes to ChatGPT only while you dictate. History and recordings stay on your
computer.

## For developers

One Rust codebase for Windows, macOS and Android. The Android app is a thin Kotlin shell over the same core.

```
core/            shared Rust, used by every app
  voxa-core        the dictation flow (a tested state machine), strip wording, trimming, word list
  voxa-chatgpt     ChatGPT session, account and transcription client
  voxa-vad         Silero voice detection (cuts long silences before upload)
  voxa-store       history, settings, word list, finish-text pipeline, silence rules, upload retries
  voxa-ffi         the core for Kotlin, through UniFFI (Android)
desktop/
  app/             the Windows + macOS app (GPUI): strip, tray, Home, History, Words, Insights, Settings
    src/win.rs       Windows-only code   } the only per-OS files; the build picks one
    src/mac.rs       macOS-only code     } (mod os in main.rs), everything else is shared
  signin/          the ChatGPT sign-in window (system WebView)
  windows/         Windows packaging: installer (Inno Setup), ONNX runtime download
  macos/           macOS packaging: Info.plist, entitlements, .app and .dmg script
android/         Android voice keyboard and companion app (Kotlin, Compose), calling core/ via voxa-ffi
docs/            release runbook, update design and keys, guardrails, regressions, mockups, reviews
```

**Build on Windows**
```
cargo build --release -p voxa-desktop -p voxa-signin
desktop\windows\fetch-onnxruntime.ps1     # once, for silence trimming
ISCC desktop\windows\voxa.iss             # installer -> target\dist (refuses placeholder update keys;
                                          # /DAllowPlaceholderKeys builds a local test copy)
```

**Build on macOS:** `bash desktop/macos/package.sh` -> `target/dist/Voxa-<version>-mac.dmg`

**Build for Android** (Windows, macOS or Linux)
```
rustup target add aarch64-linux-android x86_64-linux-android   # once
cargo install cargo-ndk                                          # once
cd android && ./gradlew assembleDebug                            # -> app/build/outputs/apk/debug
```
Needs the Android SDK with NDK 28.0.13004108 (`android/local.properties`: `sdk.dir=...`) and JDK 17+
(Android Studio's JBR works). Gradle's `preBuild` runs `cargo ndk` for arm64-v8a only by default;
add the x86_64 emulator with `-Pabis=arm64-v8a,x86_64`. Then it generates the Kotlin bindings with
UniFFI (`voxa-ffi`'s `uniffi-bindgen` bin). `-PskipRust` reuses the last Rust build;
`-PrustProfile=debug` builds Rust faster. A release build (`assembleRelease`) refuses placeholder
update keys and the debug key; `-Pvoxa.devRelease=true` builds a local test copy that is never
handed out. Instrumented tests (FFI, flow, ChatGPT reachability) run on an emulator only, because
they uninstall the app and its data: `ANDROID_SERIAL=emulator-5554 ./gradlew connectedDebugAndroidTest`.

**Tests:** `cargo test --workspace` and `python tools/lint/run.py` (the guardrails). Turn on the
git hooks once per clone: `git config core.hooksPath .githooks`.

**Releases:** follow [docs/release-runbook.md](docs/release-runbook.md) step by step (keys, the
update feed, tags, Android and the real-hardware release gate). GitHub Actions
(`.github/workflows/desktop.yml`) builds and tests Windows and macOS on every push to `main`; a tag
like `v1.0.1` publishes the Windows installer and the Mac `.dmg`, then the signed update feed.

The earlier Electron app is in git history (removed in `110ea43`).

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

Copyright 2026 Simiyon Vinscent Samuel L. Releases before 1.0.0 contained HeliBoard and were GPL-3.0.
