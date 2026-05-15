# Upstream

This subdirectory is a vendored fork of [HeliBoard](https://github.com/Helium314/HeliBoard) at tag **v3.9** (released 2026-03-29).

HeliBoard is an open-source Android keyboard, a fork of OpenBoard, which is itself a fork of AOSP LatinIME (Google's pre-Gboard Android keyboard). License is **GPL-3.0** — see `LICENSE` in this directory.

## Why it's here

The Voxa team is forking HeliBoard to graft the Voxa mic button (Whisper transcription via ChatGPT) into its toolbar, retheme it to the Voxa "Quiet Sheet" palette, and ship it as Voxa's full QWERTY keyboard with word suggestions, autocorrect, glide typing, and multi-language support.

The home-grown Compose QWERTY (at `voxa-kotlin/app/src/main/java/com/voxa/android/ui/keyboard/`) will be deleted in Phase D of the migration.

## Project layout (independent of `voxa-kotlin/app/`)

HeliBoard keeps its own Gradle build:
- `keyboard/build.gradle.kts` — root
- `keyboard/settings.gradle` — module list (`:app`, `:tools:make-emoji-keys`)
- `keyboard/gradlew` / `gradlew.bat` — own wrapper
- `keyboard/app/` — the IME app
- `keyboard/tools/make-emoji-keys/` — emoji generation tool

Build with:
```
cd voxa-kotlin/keyboard
./gradlew :app:assembleDebug
```

### Windows note: spaces in path break NDK build

The Android NDK's `ndk-build.cmd` can't parse paths with spaces, so building from the OneDrive location directly (`C:\Users\sam\OneDrive\one drive back up\...`) fails with:

```
APP_BUILD_SCRIPT points to an unknown file: ...\jni\Android.mk
```

Workaround: map the keyboard dir to a virtual drive letter in PowerShell, then build from there.

```powershell
subst V: "C:\Users\sam\OneDrive\one drive back up\OneDrive - SSN-Institute\Documents\projects\voxa\voxa-kotlin\keyboard"
cd V:\
./gradlew :app:assembleDebug
```

The `subst` mapping only persists for the current Windows session. To remove it: `subst V: /D`.

(Long-term fix: move the project to a no-spaces path or use `cmake` instead of `ndk-build`. Not worth the effort for a fork.)

## License interaction

HeliBoard is GPL-3.0. Vendoring it into Voxa means the resulting HeliBoard-derived APK is GPL-3.0. Voxa's other components (the existing `voxa-kotlin/app/` Compose app, and the Electron desktop app) are not affected by inclusion of this directory — they sit in separate APKs / Gradle projects and do not link against HeliBoard code.

If we ever distribute a binary that combines HeliBoard's code with Voxa's `app/` Kotlin code in a single APK, the combined binary becomes GPL-3.0.

## Voxa-specific patches applied to upstream

When pulling a newer HeliBoard tag, re-apply these (the diff is small):

1. **`app/build.gradle.kts`** — converted from `com.android.application` to `com.android.library`. Removed `applicationId`, `versionCode`, `versionName`, custom `buildTypes` other than `release`/`debug`, `signingConfigs`, the `androidComponents.onVariants` block, and the `dependenciesInfo` block (application-only). Added `buildConfigField` entries for `VERSION_NAME` / `VERSION_CODE` / `APPLICATION_ID` / `BUILD_TYPE` since library modules don't auto-generate them. Raised `minSdk` from 21 to 29 to match Voxa.
2. **`app/src/main/AndroidManifest.xml`** — removed the `MAIN`/`LAUNCHER` intent-filter from `helium314.keyboard.settings.SettingsActivity` so the keyboard settings don't show up as a separate launcher icon in the host app drawer. SettingsActivity is still reachable from the keyboard's toolbar settings button.
3. **`app/src/main/java/helium314/keyboard/latin/App.kt`** — extracted the body of `onCreate` into a public static `App.initHeliBoard(application)` so a host application (such as Voxa) can run it from its own `Application.onCreate()`. The original `onCreate` now just delegates to `initHeliBoard(this)` for the standalone-build case.

## Updating

To pull a newer upstream version:
1. Note current pinned tag (currently `v3.9`).
2. Replace this directory's contents with a fresh `git clone --depth 1 --branch <new-tag>` and `rm -rf .git` inside.
3. Re-apply Voxa-specific patches (Voxa mic button, theme overrides). Track those patches in `voxa-kotlin/keyboard/patches/` if accumulated.
4. Update this file with the new tag.
