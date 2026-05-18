# Voxa

Android Kotlin/Compose voice-dictation IME. Captures a ChatGPT session in a WebView, transcribes via OpenAI Whisper, inserts text into the active text field.

## Build & install
- `./gradlew :app:assembleRelease` — release uses debug keystore (in `buildTypes.release`) so output is sideload-installable.
- Reliable install when USB drops mid-stream: `adb push app-release.apk /data/local/tmp/voxa.apk && adb shell pm install -r /data/local/tmp/voxa.apk`
- `MSYS_NO_PATHCONV=1` prefix on Git Bash to stop `/data/local/tmp` getting rewritten to `C:/Program Files/Git/...`
- Gradle needs JDK 11+; Android Studio's bundled JBR is JDK 21 and is the safe default.

## Logs
- `adb logcat -s VoxaAudio:*` — audio routing decisions (Builtin / WiredHeadset / BluetoothSco etc.)
- `adb exec-out screencap -p > out.png` — reliable screen capture on OEM Android (avoid `shell screencap -p FILE` + pull)

## Android gotchas
- `AudioManager.setCommunicationDevice()` only accepts devices from `availableCommunicationDevices`, NOT `getDevices(GET_DEVICES_INPUTS)`. Wrong list = silent false.
- BT SCO needs ~400ms settle time after `setCommunicationDevice` before MediaRecorder will capture from the headset mic.
- Adaptive-icon `<monochrome>` element lets Android 13+ launchers theme-tint the icon. Drop it to lock designed colors.
- System splash circular-masks the icon foreground to the 66dp safe zone — wide elements get clipped.

## Design
- Theme: Quiet Sheet light (`#FAFAFA` bg, `#FFFFFF` cards, `#1F4FE0` primary, `#E5484D` destructive, `#16161A` ink).
- Tokens live in `app/src/main/java/com/voxa/android/ui/theme/Theme.kt` as `VoxaColors`.
- Phosphor-style 1.5px-stroke icons in `ui/icons/VoxaIcons.kt`.
- Mockup-first workflow: HTML in `docs/ui-mocks/` → user sign-off → Kotlin.
- Skill caches in `docs/skills-cache/` (taste-skill, minimalist-skill, redesign-skill, ui-redesign).

## Slop linter (from ui-redesign skill)
- No em-dashes (—) or en-dashes (–) in any user-facing string. Use `.` or `,`.
- No Tailwind-500 hex defaults (`#3B82F6`, `#22C55E`, `#8B5CF6`, etc.).
- No AI / model / engine / inference wording in user-facing copy.
