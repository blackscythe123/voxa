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

## License interaction

HeliBoard is GPL-3.0. Vendoring it into Voxa means the resulting HeliBoard-derived APK is GPL-3.0. Voxa's other components (the existing `voxa-kotlin/app/` Compose app, and the Electron desktop app) are not affected by inclusion of this directory — they sit in separate APKs / Gradle projects and do not link against HeliBoard code.

If we ever distribute a binary that combines HeliBoard's code with Voxa's `app/` Kotlin code in a single APK, the combined binary becomes GPL-3.0.

## Updating

To pull a newer upstream version:
1. Note current pinned tag (currently `v3.9`).
2. Replace this directory's contents with a fresh `git clone --depth 1 --branch <new-tag>` and `rm -rf .git` inside.
3. Re-apply Voxa-specific patches (Voxa mic button, theme overrides). Track those patches in `voxa-kotlin/keyboard/patches/` if accumulated.
4. Update this file with the new tag.
