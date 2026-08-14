# Voxa

A cross-platform voice-to-text assistant. The desktop app captures speech on a global hotkey, transcribes it through ChatGPT, and pastes the result into whatever app is focused. An Android keyboard lives in [`voxa-kotlin/`](voxa-kotlin/).

**Current version:** 0.2.4

## Features

### Desktop

- **System-wide hotkey** — Start and stop recording from any app (default: `F9` to start on Windows/Linux, `Alt+Space` on macOS — F9 collides with Mission Control on many Mac keyboards; `Escape` to stop everywhere)
- **Recording overlay** — Small floating widget with a live timer
- **ChatGPT transcription** — Uses ChatGPT's backend Whisper API for speech-to-text
- **Auto-paste** — Transcript is copied to the clipboard and pasted into the focused field (Windows: `Ctrl+V` via SendKeys; macOS: `Cmd+V` via System Events, requires Accessibility permission; Linux: clipboard fallback)
- **Hub UI** — Home, History, Microphone, Shortcuts, and Settings in one window
- **Dictation history** — Browse past transcripts with source app labels, word counts, and pagination
- **Audio retention** — Replay saved recordings; configurable retention and disk limits
- **Retry** — Re-transcribe failed dictations from stored audio
- **Unlimited recording** — No hard cap on session length (configurable max duration in Settings)
- **Background operation** — Runs in the system tray with optional launch at login

### Android keyboard

The [`voxa-kotlin/`](voxa-kotlin/) tree contains a HeliBoard-based IME with an in-keyboard mic for voice input on Android. See [`voxa-kotlin/CLAUDE.md`](voxa-kotlin/CLAUDE.md) for build notes.

## Installation

1. Clone this repository:
   ```bash
   git clone https://github.com/blackscythe123/voxa.git
   cd voxa
   ```
2. Install dependencies:
   ```bash
   npm install
   ```

## First Run

```bash
npm start
```

The app opens the Hub window and adds a system tray icon. You can close the window and keep dictating from the tray.

## Setup

1. **Sign in to ChatGPT**
   - Open **Settings** in the Hub (or right-click the tray icon → **Login to ChatGPT**)
   - A browser window opens for ChatGPT login (2FA, CAPTCHA, etc.)
   - Once signed in, your session is saved locally and the browser closes

2. **Configure hotkeys and audio** (optional)
   - **Shortcuts** — Set start/stop hotkeys and overlay position
   - **Microphone** — Choose input device
   - **Settings** — Retention limits, privacy mode, launch at login, and recording duration

## Usage

1. Click into any text field (VS Code, Google Docs, Word, Slack, etc.)
2. Press your start hotkey (`F9` by default on Windows/Linux, `Alt+Space` on macOS)
3. The recording overlay appears with a timer
4. Speak naturally
5. Press the stop hotkey or click **Stop**
6. Wait a few seconds for transcription
7. The transcript is pasted into the focused field (or left on the clipboard if paste fails)

Open **History** in the Hub to review, replay audio, copy text, or retry failed dictations.

## Building installers

```bash
npm run build:win     # Windows NSIS installer
npm run build:mac     # macOS DMG + zip
npm run build:linux   # Linux AppImage + deb
npm run build:all     # All platforms
```

Output lands in `dist/`.

## Architecture

```
src/
├── main.js                      # Electron main process (hotkeys, tray, IPC, dictation pipeline)
├── preload.js                   # Secure preload bridge
├── services/
│   ├── chatgptAuth.js           # ChatGPT login + transcription API
│   ├── configStore.js           # Local preferences (electron-store)
│   ├── historyStore.js          # Dictation history (JSON on disk)
│   ├── audioStore.js            # Saved recording files
│   ├── retentionWorker.js       # Audio/history cleanup by retention policy
│   ├── sourceApp.js             # Foreground app detection for history labels
│   └── textInserter.js          # Clipboard + paste into focused window
└── renderer/
    ├── index.html               # Hub shell (sidebar + routed views)
    ├── overlay.html             # Recording overlay
    ├── renderer.js              # Hub router, audio capture, history UI
    └── styles/                  # tokens, hub, history, views
```

## Dependencies

- **Electron** — Desktop app framework
- **electron-store** — Local preference persistence

Dictation history and audio are stored under the app's user data directory (`history.json` and an `audio/` folder).

## Notes

### Security

- ChatGPT session cookies are stored in the app's local user data folder
- Audio is sent to ChatGPT's servers for transcription
- **Privacy mode** (Settings) skips writing history and audio to disk

### Limitations

- Automatic paste is implemented on Windows and macOS (macOS requires Accessibility permission — Voxa will prompt on first launch); Linux relies on clipboard + manual paste
- macOS builds are unsigned and not notarized — Gatekeeper will flag the app as from an "unidentified developer" on first launch (see Troubleshooting below)
- Password fields are excluded from paste targets where possible
- ChatGPT's internal API endpoints may change; updates may be required

### Troubleshooting

**Hotkey not working?**
- Check for conflicts with system hotkeys or other apps
- Try a different hotkey in **Shortcuts**

**Recording but no transcription?**
- Sign in again via Settings or the tray menu
- Check your internet connection
- ChatGPT may be rate-limited or temporarily unavailable

**Microphone not working?**
- Grant microphone permission when prompted
- Pick the correct device under **Microphone**
- Confirm the mic works in another app

**Text not inserting?**
- Click the target field before dictating
- On macOS, grant Accessibility permission (System Settings → Privacy & Security → Accessibility) — without it, auto-paste silently falls back to clipboard-only
- On Linux, paste manually from the clipboard (`Ctrl+V`)
- Some secure fields block programmatic paste

**macOS says the app is from an "unidentified developer"?**
- The app isn't notarized (no Apple Developer account). Right-click the app in Finder and choose **Open**, or run `xattr -cr /Applications/Voxa.app` in Terminal, then launch normally.

**History or audio missing?**
- History requires a successful `npm install` with no errors loading history services
- Check retention settings in **Settings** (success audio defaults to kept indefinitely, bounded by disk cap)

## Development

1. Edit files in `src/`
2. Restart the app (`npm start`)
3. Main-process changes require a full restart

Regenerate tray/app icons after asset changes:

```bash
npm run icons
```

## License

MIT — see [LICENSE](LICENSE).

## Support

For issues or feature requests, open an issue on [GitHub](https://github.com/blackscythe123/voxa).
