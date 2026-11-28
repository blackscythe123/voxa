# Pause / resume verification (2026-11-28)

| Leg | Evidence | Level |
| --- | --- | --- |
| Core flow (flag on Recording, idempotent Pause/Resume, Stop/Hotkey/Cancel/Retry/Readiness/Focus while paused, copy) | `core/voxa-core/tests/flow.rs` (9 new tests), `core/voxa-ffi/tests/ffi.rs` (round trip) | tests pass |
| Desktop strip: Listening `[Cancel] [Pause] [Stop]`, Paused `[Cancel] [Resume] [Stop]`, pause glyph, faded frozen ink, frozen timer | `desktop-1..4-*.png` (PrintWindow of a dev copy, VOXA_TEST_PAUSE=3,5,3) | rendered |
| Desktop Esc released while paused, bound again on resume | `desktop-run-a.log` ("Esc released" after Pause, "Esc bound" after Resume) | logged |
| Desktop file leaves the pause out | run a: 3 s talk, 5 s paused, 3 s talk, file 6.31 s (6.0 talk + 0.3 silence); timer 0:03 during the pause | logged |
| Desktop watchdog does not fire while paused | `desktop-run-b-paused-at-start.log`: paused 4 s right after start (no callbacks yet), no "restarting the stream" | logged |
| Android keyboard: Listening, Paused ("The microphone is off", red dot hidden, bars flat, timer frozen at 0:04), Resumed, Stop inserts | `android/docs/verification/pause/1..5-*.png` (emulator-5554, Contacts field, canned debug transcript) | rendered |
| Android PCM leaves the pause out | `android/docs/verification/pause/diag-run.log`: wall 15.1 s (start 14:43:04.83, stop 14:43:19.98), paused 7.1 s, file 264320 bytes / 32000 = 8.26 s (7.96 s talk + 0.30 s silence), History seconds 8.0 | logged |

Not verified: real voice across a pause (the emulator mic hears silence; desktop takes are cancelled, never sent), the
Windows mic privacy indicator going out while paused, the Android call refusal on Resume (no call on the emulator),
resume after sleep or a device change, and hold to talk released while paused.
