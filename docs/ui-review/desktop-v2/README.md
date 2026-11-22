# Desktop UI, round 2 (2026-11-22)

Screens from the real app (GPUI, release build, stand-in transcription, throwaway data folder with
synthetic tone recordings). Captured and driven by `desktop/app/tests-e2e/window.ps1`.

## What changed after the round-1 reviews (`../desktop-v1`)

Two EGM reviews (design critic, first-time user) read every round-1 screen. Acted on:

| Finding | Now |
|---|---|
| No welcome, no sign-in explanation, no demo | 6-step tour: welcome, sign in (with what is sent and what stays), mic, shortcut + practice box, what the strip shows, done. Progress bar and "Skip tour" on every step. |
| Mic step had no visible meter | Live ink meter; "Is the line flat?" offers privacy settings and the sound panel |
| Keycaps broke the "Try it" sentence | Big keycaps on their own row, with Change |
| Start with Windows defaulted Off | On by default at the end of the tour (can be switched off there) |
| History was a flat list, nothing like the old app | Search, app filters with counts, Everything / Not pasted / Not sent, sort, totals; per entry Copy, Play, Retry or Send, Show in folder, Delete (with confirm) |
| "Not pasted" had no reason | Explains why and what to do |
| App badges all purple | Brand colours for known apps, distinct hashed colours otherwise |
| Dark title bar and default icon | Light caption in the nav colour; new app icon in window, taskbar, exe and tray |
| Wordmark font inconsistent | Fraunces embedded |
| Insights: zero day looked broken, today unlabelled | Values on bars, dot for empty days, "Today" label |
| Re-run setup buried at the bottom | Settings > Help: "Take the tour again", version, Open data folder |
| Delete confirm said "Keep" | "Cancel" / "Delete 7 dictations" |
| Microphone row was read-only | Change opens Windows sound settings |

Misreadings from round 1 (not app bugs): the strip crops left out its timer and Stop/Cancel; "signed
in" on step 1 and "0.0 MB" came from stand-in mode and seeded entries without recordings.

## Verified (real app, real clicks)

Tour steps 1-6 render; Finish saves `onboarded` and writes the Run key (removed after the test).
History filters (Not sent, app + status = empty message), Play switches to Stop, Delete removes the
entry and its WAV (checked on disk). Settings controls round-trip to `settings.json` / registry
(round 1). Not verified: audio actually audible (needs ears), Retry against ChatGPT from History
(stand-in mode has no session), the practice box with a real dictation, the tray menu by mouse.

## Still open

Device picker inside Voxa, tray icon states beyond the status dot, hold-to-talk, a detail pane per
dictation, interactive animated demo (scripted strip run), real app icons in History.
