# Open-source Android keyboard survey

Context: Voxa currently ships a voice-only IME (`VoxaInputMethod`). We want a *second* IME variant that is a normal QWERTY keyboard plus a small mic button in the top bar (Gboard-style). Press the mic = start recording, press again = stop + transcribe + insert. Typing on QWERTY still works. Must support emoji, number row, and standard keyboard features. The voice-only IME stays.

We want to **fork or embed an existing open-source keyboard** rather than rebuild QWERTY from scratch.

## Candidates evaluated

| Project | License | Stack | Last release | Suggestions / autocorrect | Emoji | Glide | Embeddable? |
|---|---|---|---|---|---|---|---|
| [FlorisBoard](https://github.com/florisboard/florisboard) | **Apache 2.0** | **Kotlin + Jetpack Compose** | v0.5.2 (Nov 2025), active | Not yet (v0.6 milestone) | Yes | No | Fork only, modular Gradle |
| [AnySoftKeyboard](https://github.com/AnySoftKeyboard/AnySoftKeyboard) | Apache 2.0 | Java + legacy Views | v1.13-r1 (Feb 2026), active | Yes | Yes | Yes | Fork only |
| [HeliBoard](https://github.com/Helium314/HeliBoard) | **GPL-3.0** | Java + Kotlin + C++, Views | v3.9 (Mar 2026), active | Yes | Yes | Yes (closed-src lib) | Fork only |
| [OpenBoard](https://github.com/openboard-team/openboard) | GPL-3.0 | Java + C++ | Aug 2022, dormant | Yes | Yes | No | Fork only |
| AOSP LatinIME | Apache 2.0 | Java + C++ | Stale | Yes | Yes | No | Painful outside AOSP |
| [Simple Keyboard](https://github.com/rkkr/simple-keyboard) | Apache 2.0 | Java + Views | v6.3 (Jan 2026) | No | No | No | Yes (small) |

## Ranked recommendation

1. **FlorisBoard** — best stack fit for Voxa (already Kotlin + Compose), permissive license, very active. Trade-off: no word suggestions / autocorrect until v0.6. Mic button is a trivial Composable in the smartbar.
2. **AnySoftKeyboard** — best feature fit *today* (suggestions, autocorrect, glide, emoji) under a permissive license, but the codebase is Java + legacy Views, clashing with the rest of Voxa.
3. **HeliBoard** — best UX overall, but GPL-3.0 forces the whole Voxa app to GPL-3.0 (or ship the keyboard as a separately distributed APK). Skip unless we accept that.
4. OpenBoard / AOSP LatinIME / Simple Keyboard — skip (dormant, stale, or too minimal).

## Decision (working assumption — easy to revisit)

**Fork FlorisBoard** under `voxa-kotlin/keyboard-floris/` as a Gradle module, rename the package to `com.voxa.android.keyboard`, drop in a Voxa mic button in its smartbar that talks to `AudioRecorder` and `TranscriptionApi`. Ship as a *second* IME alongside the existing voice-only `VoxaInputMethod`.

Trade-off accepted: no autocorrect/suggestions on day one. If users complain before FlorisBoard's v0.6 lands, the fallback plan is to swap the keyboard layer for AnySoftKeyboard (the Voxa mic button and the recorder/transcribe code stay; only the keyboard chrome changes).
