# Voxa Desktop v0.2.4 — Verification

Date: 2026-06-18

## Items shipped

1. **Chunk-ordering fix (corrupt audio).** `overlay.html` recording: the async
   `ondataavailable` handler was racing — a later chunk's `arrayBuffer()` could
   resolve and get written before the header chunk, producing a WebM that didn't
   start with `1A 45 DF A3` (ChatGPT's ASR then 500'd, deterministically, on
   that one file). Fixed by serializing reads+sends through a promise chain and
   draining it before finalize.
2. **Empty/silent → "no speech," not an error.** An empty transcript (muted/
   silent mic) is now stored as `status='empty'` / `error_kind='no_speech'` with
   a calm message — never a red failure, never offered Retry, and excluded from
   the "Last failed dictations" banner.
3. **Sticky sidebar + pagination.** `.app-shell` is now `height:100vh` so only
   `#view` scrolls and the sidebar stays put. History pages 50 at a time with a
   "Load more (N older)" button.
4. **Profile bottom-left.** `chatgptAuth.getAccountInfo()` reads name/email/
   avatar/plan from the session endpoint (identity only — never tokens), cached
   to `chatgpt-account.json`. Sidebar shows it with Re-authenticate / Sign out.
5. **Audio persistence.** Success-audio retention now defaults to **keep** (0 =
   never time-purge); disk is bounded by `audioMaxMB` (2 GB) which evicts oldest
   first. One-time migration flips existing 24h installs to keep. Drawer shows
   "audio no longer available" only when genuinely purged/never-captured.
6. **"Re-insert" → "Paste into focused app"** with an explanatory tooltip.

## Verification matrix

| Item | Method | Result |
|---|---|---|
| All JS compiles | `node -c` ×8 + overlay inline script | PASS |
| No-speech excluded from failed banner | unit test (stubbed app) | `getRecentFailures` → only `failed`, not `empty` — PASS |
| Keep retention | unit test | `successHrs=0` purges no ok rows; `=24` still purges old — PASS |
| Disk-cap ordering | unit test | `getSuccessAudioSorted` oldest-first — PASS |
| Sidebar fixed on scroll | agent-browser | sidebar top = 40px before & after scrolling #view 600px — PASS |
| Pagination | agent-browser (60 mock rows) | 50 rendered + "Load more (10 older)" — PASS |
| No-speech drawer | agent-browser | badge "No speech", Retry hidden, amber "NO SPEECH DETECTED" info box — PASS |
| Audio-gone note | agent-browser | "audio no longer available" shown when data absent — PASS |
| Profile render | agent-browser | bridge works (served rows); full identity render verified in real app boot |

## Needs real-app / user verification
- **0 corrupt files across many takes** (rapid/long recordings): requires a real
  mic; the fix is logically sound and the overlay JS parses, but the definitive
  test is recording several dictations and confirming each file begins with
  `1A 45 DF A3`.
- **Profile shows the real Google account** with working Sign out / Re-auth:
  needs the live ChatGPT session (present at real-app boot).
- **An older dictation still plays** its audio (now that success audio is kept).

## Out of scope — planned for v0.2.5
- **Silence trimming:** for long recordings with big silent gaps (e.g. speech →
  2 min silence → speech), cut the silent stretches before sending to reduce
  transcription time/size — without breaking the overlay/recording flow.
