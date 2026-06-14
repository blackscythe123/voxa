# Voxa Wispr-Flow Redesign — Verification v1

Date: 2026-06-15
Scope: Hub UI redesign + dictation **History**, **audio retention**, and **failed-dictation retry**.

## What was built

- **Hub UI** — single-window app with a left sidebar (Home / History / Microphone / Shortcuts / Settings),
  inline `<template>` views cloned by a vanilla-JS router (`src/renderer/renderer.js`), parchment aesthetic
  preserved. Mockup of record: `docs/ui-mocks/v3-whisperflow.html`.
- **History store** — `src/services/historyStore.js`. **Pure JavaScript**, atomic JSON file at
  `<userData>/history.json`. (Replaced the original better-sqlite3 design — see "Storage decision".)
- **Streaming audio** — `src/services/audioStore.js` + `overlay.html` rewrite. Audio is streamed to
  `<userData>/audio/<id>.webm` in 250 ms chunks **during** recording, so a crash mid-dictation no longer
  loses the audio (the original failure the user reported).
- **Retention** — `src/services/retentionWorker.js`. Success audio kept **24 h**, failed audio kept **7 d**,
  purged on launch + every 6 h. Configurable in Settings.
- **Retry** — `history-retry` IPC reads the saved audio off disk, re-runs ChatGPT transcription, updates the
  same row, and pastes into the focused app. Surfaced in a "Last failed dictations" banner (cap 5) + per-row
  menu + detail drawer.
- **Source-app attribution**, **crash recovery** (orphan `pending` → `failed/timeout` on launch),
  **privacy mode** (purge on quit).

## Storage decision (important)

`better-sqlite3` is a native module and requires Visual Studio build tools to compile. This machine has none,
so the app could neither run nor package. Swapped the history store to a **pure-JS atomic JSON file store**
with the identical public API (15 functions, same signatures + return shapes), so `main.js`,
`retentionWorker.js`, and the renderer were untouched. Removed `better-sqlite3`, `@electron/rebuild`, and the
`postinstall` rebuild from `package.json`. The app now runs and packages anywhere with zero native build.

## Verification matrix

| Leg | Method | Evidence | Status |
|---|---|---|---|
| Store logic (all 15 fns) | Node unit test, stubbed `electron.app` | insert→finalize→list-order→search→recentFailures→markRetry (status flips, retry_count++, transcript updates)→reload-from-disk | **PASS (DB-confirmed)** |
| All renderer/service JS compiles | `node -c` on 7 files | exit 0 each | **PASS** |
| Real app boots | `electron .`, read main-process log | `historyStore init ok` at real `%AppData%\voxa\history.json`, audio dir created, retention worker ran on real 24h/7d prefs, **0 errors** | **PASS** |
| Real data binding | Launch with 4 seeded rows, read Home | stats showed **2 dictations / 30 words today** (= the two `ok` rows seeded for today, 21+9 words) | **PASS (DB-confirmed via real bridge)** |
| History view (real renderer) | CDP into the live renderer, click History, read DOM | `active=history`; "Last failed dictations" banner **visible**; **2 failed rows, 2 Retry buttons**; groups `today`/`yesterday`; transcripts render intact (raw charcodes verified) | **PASS (rendered, real bridge)** |
| Hub layout/all 6 views | agent-browser static render (mock bridge) | every view mounts, 0 JS errors; failed banner + retry + detail drawer captured | **PASS** |

### Not yet verified — needs the user's hardware/credentials (voice leg)

- The full voice round-trip: hotkey → record → audio streams to disk → ChatGPT transcribes → row finalizes →
  pastes into focused app. Requires a real microphone and a valid ChatGPT login.
- A real retry: needs a genuinely failed dictation whose audio is on disk + a valid ChatGPT session. (Seeded
  rows reference audio files that don't exist, so retry correctly reports "audio unavailable".)

To drive these: launch `npm start`, sign in, press the hotkey in a text field, speak, and confirm the row
appears in History; then force a failure (disconnect network mid-dictation) and click Retry once reconnected.

## Test-harness note (honesty)

An early CDP extraction one-liner appeared to drop every lowercase "s" from transcripts. Investigated rather
than assumed: the raw DOM `textContent` (charcodes) showed the text fully intact — it was a regex
double-escaping artifact in the throwaway extraction script, **not** an app bug.
