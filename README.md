# Voxa

**Talk instead of typing, in any app.** Press a shortcut, say what you want to write, and Voxa
turns it into text with your own ChatGPT account and pastes it where your cursor is: chats, email,
documents, code, Claude Code, anywhere.

Native, fast and light: the strip appears in about 30 ms and Voxa uses about 65 MB of memory.

## Download

| Platform | Get it |
|---|---|
| **Windows 10/11** | [Voxa-Setup-0.4.7.exe](https://github.com/blackscythe123/voxa/releases/download/v0.4.7/Voxa-Setup-0.4.7.exe) |
| **Mac** (Apple Silicon and Intel) | [Voxa-0.4.7-mac.dmg](https://github.com/blackscythe123/voxa/releases/download/v0.4.7/Voxa-0.4.7-mac.dmg) |
| **Android** (voice keyboard, Android 10+) | [Voxa-Android-0.2.1.apk](https://github.com/blackscythe123/voxa/releases/download/android-v0.2.1/Voxa-Android-0.2.1.apk) |

Always the newest version: [Windows](https://github.com/blackscythe123/voxa/releases/latest/download/Voxa-Setup.exe)
· [Mac](https://github.com/blackscythe123/voxa/releases/latest/download/Voxa-mac.dmg)
· [latest release page](https://github.com/blackscythe123/voxa/releases/latest). All releases:
[Releases page](https://github.com/blackscythe123/voxa/releases).

- **Windows:** run the installer (no admin needed). If SmartScreen says "Windows protected your PC",
  click *More info* > *Run anyway*. It updates the old Voxa and keeps your history and sign-in.
- **Mac:** drag Voxa to Applications, then the first time **right-click > Open** (not notarized yet).
  Allow the microphone, and Accessibility so Voxa can paste.
- **Android:** if you had an older Voxa for Android, uninstall it first, then install the APK. Send it as a file (Drive, or a WhatsApp/Telegram document); a broken copy shows "problem while parsing the package".

You need a ChatGPT account (free works). Voxa signs in on ChatGPT's own page and never sees your
password.

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

One Rust codebase for Windows and macOS, plus the Android app.

```
core/            shared Rust, used by every app
  voxa-core        the dictation flow (a tested state machine), strip wording, trimming, word list
  voxa-chatgpt     ChatGPT session, account and transcription client
  voxa-vad         Silero voice detection (cuts long silences before upload)
desktop/
  app/             the Windows + macOS app (GPUI): strip, tray, Home, History, Words, Insights, Settings
    src/win.rs       Windows-only code   } the only per-OS files; the build picks one
    src/mac.rs       macOS-only code     } (mod os in main.rs), everything else is shared
  signin/          the ChatGPT sign-in window (system WebView)
  windows/         Windows packaging: installer (Inno Setup), ONNX runtime download
  macos/           macOS packaging: Info.plist, entitlements, .app and .dmg script
android/         Android app and keyboard (Kotlin)
docs/            flow audit, design mockups, reviews, measurements
```

**Build on Windows**
```
cargo build --release -p voxa-desktop -p voxa-signin
desktop\windows\fetch-onnxruntime.ps1     # once, for silence trimming
ISCC desktop\windows\voxa.iss             # installer -> target\dist
```

**Build on macOS:** `bash desktop/macos/package.sh` -> `target/dist/Voxa-<version>-mac.dmg`

**Tests:** `cargo test --workspace`

**Releases:** GitHub Actions (`.github/workflows/desktop.yml`) builds and tests Windows and macOS on
every push to `main`. Push a tag like `v0.4.1` to publish a release with the Windows installer and
the Mac `.dmg`.

The earlier Electron app is in git history (removed in `110ea43`).

## License

GPL-3.0. See [LICENSE](LICENSE).
