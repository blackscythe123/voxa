# Voxa for Android 0.3.0 (v6 redesign): verification

Engine for every screenshot: the `voxa_api35` emulator (Android 15, x86_64), driven with adb.
Design reference: `android/docs/ui-mocks/v6-app-redesign.html`.

## How the emulator was set up

- No ChatGPT account and no real microphone are available, so the debug build has test seams
  (debug builds only, never in release):
  - `--ez debug_ready true`: treats the session as good and the network as up.
  - `--es debug_transcript "..."`: the keyboard pastes this text instead of calling ChatGPT. Everything
    after transcription runs for real (paste into the field, History, the success state).
  - `--ez debug_levels true`: the level bars move to a made-up voice (the emulator's mic hears silence).
  - `--ef debug_launch_at 0.25`: holds the launch animation at one point, for screenshots.
  - `--es debug_route home|history|words|settings|setup|detail:<id>`: opens a screen directly.
- History, words and the account name come from throwaway fixtures (names "Alex Rivera" etc.).

## Results per screen and leg

| Screen / leg | Evidence | Status |
|---|---|---|
| Launch animation, cold start | `01-launch-at-*.png` (frozen frames), `01-launch-real-cold-start-light.png` (real run) | Rendered |
| Launch skipped with animations off | `01-launch-skipped-animations-off-light.png` (content shows directly) | Rendered |
| Setup, 4 steps, live re-check | `02-setup-*`: keyboard step turned todo when the IME was disabled and done when re-enabled | Rendered |
| Setup, try once | `02-setup-4-tried-*`: real keyboard pasted into the setup field, checklist turned green from History | Rendered (canned transcript) |
| Sign-in screen | `02-signin-webview-light.png`: header restyled, WebView loads chatgpt.com | Reachable; sign-in itself not done |
| Home, readiness | `03-home-light.png` (ready), `03-home-problem-light.png` (mic not allowed) | Rendered |
| Home, stats | 3m saved, 204 words, 161 words a minute from fixture history via `week_stats` | Rendered |
| Try Voxa sheet | `03-home-try-sheet-*` | Rendered |
| History list, search, recent searches, Your apps | `04-history-*`; recent searches created by typing in the field | Rendered |
| Real app icons | Gmail, Messages and Voxa icons come from PackageManager; others use brand tiles | Rendered |
| Dictation detail, player | `05-detail-playing-*`: AudioTrack playback head advanced and the bars filled | Rendered (sound not heard: headless) |
| Edit a dictation | `05-detail-edit-light.png`; history.json read back with the new text | File confirmed |
| Delete a dictation | `05-detail-delete-confirm-light.png`; history.json read back without it | File confirmed |
| Words: add, edit, delete a fix | `06-words-*`; words.json read back after add and after delete | File confirmed |
| Settings switches | remove_fillers flipped and back in settings.json | File confirmed |
| Keyboard idle / listening / success | `08-keyboard-*` over Google Messages; "Pasted in Messages", 12 words, "That's 14 seconds of typing saved", back to idle after about 1.5 s | Rendered (canned transcript, synthetic levels) |
| Release (R8) build | `09-release-*`: app and keyboard render, real signed-out state | Rendered |

## Size and memory

| | 0.2.1 | 0.3.0 |
|---|---|---|
| arm64 release APK | 5,236,730 B | 5,267,838 B |
| Universal APK (re-signed) | 6,104,888 B | 6,130,512 B |
| Keyboard process PSS, shown idle over Contacts (release universal, x86_64 emulator) | 39.1 to 39.3 MB settled | 37.9 to 38.4 MB settled |

PSS details: `meminfo-ime-0.2.1-release.txt`, `meminfo-ime-0.3.0-release.txt`.

## Tests

`instrumented-tests-api35.xml`: 6 of 6 pass on the emulator (including the new check for the pasted
summary, app counts, week stats, waveform and edit). Core: `cargo test -p voxa-store -p voxa-ffi` pass.

## Not verified

- Real dictation: no ChatGPT account was signed in and the emulator has no real microphone. The
  transcription call, and real voice moving the level bars, are untested here.
- Sign-in through the WebView, and "Write again" / "Send now" (they need a real session).
- Hearing the playback (the emulator runs headless).
- Hold to talk versus tap to stop, the call refusal and the silenced-mic handling: the code paths are
  unchanged apart from the hold-to-talk switch, but none of them was exercised on the emulator.
- Anything on a physical phone.
