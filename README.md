# budgetWhisper

A cross-platform desktop app that captures voice input, transcribes it using ChatGPT, and automatically inserts the text into any active text field.

## Features

- **System-wide hotkey listening** — Press your configured hotkey from anywhere (VS Code, Word, browser, etc.)
- **Voice recording overlay** — Small floating widget shows recording timer
- **ChatGPT transcription** — Uses ChatGPT's backend API for accurate speech-to-text
- **Auto-insert** — Transcribed text automatically typed into the active text field
- **Configurable settings** — User hotkeys, overlay position, max recording duration
- **Background operation** — Runs in system tray; no window clutter

## Installation

1. Clone or download this repository
2. Navigate to the folder:
   ```bash
   cd budgetWhisper
   ```
3. Install dependencies:
   ```bash
   npm install
   ```

## First Run

```bash
npm start
```

This launches the Electron app in the background. You should see a system tray icon appear.

## Setup

1. **Right-click the tray icon** → Select `Login to ChatGPT`
   - A browser window opens automatically
   - Complete your ChatGPT login (2FA, CAPTCHA, etc.)
   - Once logged in, the app captures your session and closes the browser
   - Session token is stored locally on your machine

2. **Configure hotkeys** (optional):
   - Right-click tray → `Preferences`
   - Set your start hotkey (default: `Ctrl+Shift+R`)
   - Set your stop hotkey (default: `Escape`)
   - Choose overlay position (top-left, top-right, bottom-left, bottom-right)
   - Click **Save**

## Usage

1. Click into any text field (VS Code, Google Docs, Word, Slack, etc.)
2. Press your start hotkey
3. The recording overlay appears with a timer
4. Speak naturally
5. Press stop hotkey or click the **Stop** button
6. Wait 1–3 seconds for transcription
7. Transcript is automatically typed into the text field

## Architecture

```
src/
├── main.js                      # Electron main process (hotkey, tray, IPC)
├── preload.js                   # Secure preload bridge
├── services/
│   ├── chatgptAuth.js           # Playwright login + transcription API
│   ├── configStore.js           # Local preferences storage
│   └── textInserter.js          # Text insertion via keyboard simulation
└── renderer/
    ├── index.html               # Recording overlay UI
    ├── preferences.html         # Settings window
    ├── renderer.js              # Overlay logic + audio capture
    └── styles.css               # Overlay styling
```

## Dependencies

- **Electron** — Desktop app framework
- **Playwright** — Headless browser for ChatGPT authentication
- **@nut-tree-fork/nut-js** — Keyboard text insertion
- **electron-store** — Local preference persistence

## Notes

### Security
- ChatGPT session tokens are stored in your app's local user data folder
- Audio is transmitted to ChatGPT's servers for transcription
- No data is sent to third-party services beyond ChatGPT

### Limitations
- Text insertion uses simulated keyboard typing (not native clipboard paste yet)
- Password fields are intentionally excluded from cursor detection for security
- ChatGPT's internal API endpoints may change; updates may be required

### Troubleshooting

**Hotkey not working?**
- Check for conflicts with system hotkeys or other apps
- Try a different hotkey in Preferences

**Recording but no transcription?**
- Verify ChatGPT login via tray menu
- Check internet connection
- ChatGPT API may be rate-limited or down

**Microphone not working?**
- Grant microphone permission when prompted
- Check system audio settings
- Try another app (voicenotes, Discord) to confirm mic works

**Text not inserting?**
- Ensure cursor is actively in a text field (click the field first)
- Try clicking elsewhere and re-focusing the target field
- Some password fields intentionally block text insertion

## Development

To modify the code:
1. Edit files in `src/`
2. Restart the app (`npm start`)
3. Changes to main process require full app restart

To rebuild and package:
```bash
npm run build   # (when configured)
```

## License

MIT

## Support

For issues or feature requests, please open an issue in the repository.
