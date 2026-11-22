# Voxa flow audit (2026-11-22)

Input for the rebuild (Tauri desktop shell + Kotlin/Compose Android shell + shared Rust core).
Sources: three EGM flow reviews plus two UX/a11y reviews, read-only, against the code as of `66f1539`.
Key claims spot-checked by hand are marked (verified); the rest are reviewer findings, and anything the
reviewers did not trace end to end is marked (inferred).

## The owner's example, traced

Session expires. User presses the hotkey in WhatsApp and talks. Nothing warns them.

| Step | Desktop | Android |
|---|---|---|
| Session checked before recording? | No. Once, 4 s after launch (`main.js:1065`), then only after speaking (`chatgptAuth.js:247`) | Keyboard only checks a cookie *exists* (`VoxaInputMethod.kt:138`); HeliBoard mic checks nothing |
| What the user sees on failure | Strip hides (`main.js:754`). OS notification only if the Voxa window is not visible (`:756`); if it is merely behind WhatsApp, nothing | Raw `SESSION_EXPIRED` hint or toast |
| Recording kept? | Yes, as a History row | File left in cacheDir, never referenced. Effectively lost |
| Steps to get the text into WhatsApp | 8+: notification, sign in (reported as failed even on success, verified), History, Retry, which pastes into Voxa itself, switch app, Ctrl+V | About 8, and the user must speak again |

## Systemic flaws (both platforms)

1. **Readiness is checked after the user speaks.** Session, network, mic and paste permission are discovered at transcription time.
2. **No single state model.** Status is spread across flags, fire-and-forget status strings, CSS classes and toasts. Offline, Cloudflare-blocked and signed-out collapse into one answer. A Cloudflare 403 is read as "session expired" on both platforms, which explains the Android sign-in loop (fixed in the working tree on Android, see below).
3. **The surface the user is looking at disappears on failure.** Desktop strip hides on every outcome; errors go to a window or notification the user is not watching.
4. **Recordings are not durable jobs.** Android drops them; desktop keeps them but only History can retry them, and retry pastes into the wrong app because the delivery target is never recorded.
5. **No escape hatches.** No timeouts on network calls (desktop can sit on "transcribing…" forever), no cancel (Esc means stop-and-paste), optimistic state set before the mic actually opens.
6. **Success is self-reported.** "Pasted" means a keystroke was sent; failed copy, paste and delete still toast success.
7. **Global side effects.** Esc is grabbed system-wide while Voxa runs (verified); the clipboard is overwritten and never restored.
8. **Duplicated and dead surface area.** Two settings screens on desktop, two keyboards plus an unregistered service on Android, several settings that do nothing (Hold to talk, Trim silence, history cap), three finalize code paths on desktop.
9. **Onboarding is a snapshot.** Permission results ignored, keyboard-enabled checked once, tutorial describes hold-to-talk while the keyboard is tap-to-talk.

## Target loop for the rebuild

One state machine in the Rust core, rendered by each shell on the one surface the user is already looking at
(desktop recording strip, Android keyboard strip).

```
Idle(readiness) --press--> Preflight --ok--> Recording --stop--> Transcribing --text--> Delivering --ok--> Delivered --> Idle
                              |                                      |                        |
                              v                                      v                        v
                     Blocked(reason, action)               Failed(reason, action, jobId) -----+
```

- **Readiness** = `{session: Valid|Expired|Unknown, network: Up|Down|Blocked, mic: Granted|Denied, deliver: Ok|NeedsPermission}`.
  Refreshed on launch, on network change, periodically, and whenever the hotkey or keyboard surface appears. Shown on the idle mic as a dot.
- **Every recording is a durable job** (audio saved from the first byte, delivery target captured at start). Nothing is lost.
- **The strip never auto-hides on failure.** It shows the outcome and exactly one action.

| Situation | What the user gets, in place |
|---|---|
| Session expired before speaking | Recording does not start; strip shows "Sign in". One tap opens sign-in; afterwards back to Idle in the same field |
| Session expires mid-dictation | Audio kept; strip shows "Sign in to send this". After sign-in it re-sends and delivers into the original target automatically |
| Network down at start | Record anyway, labelled "Offline, will send when connected"; auto-send on reconnect if the target is still focused, else offer Copy |
| Cloudflare block | "Couldn't reach ChatGPT, retry" (never "sign in again") |
| Server or network error after recording | "Retry" re-sends the saved audio |
| No speech | "Didn't catch that"; no retry |
| Delivery failed or target changed | "Paste into <original app>" or "Copy" |
| Mic denied | "Allow microphone", deep link to the OS page |
| Accidental recording | Cancel discards; Esc is only bound while recording |

History becomes an archive and a place to re-use past text, not the recovery path.

## Core vs shell ownership

**Rust core:** state machine and readiness; session (cookie store, access-token cache, typed Valid/Expired/Unreachable/Blocked);
transcription client with timeouts and typed errors; durable job store keyed by job id; retry and send-when-online queue;
history and retention; error to copy-and-action mapping; one settings schema with no dead keys.

**Each shell:** mic capture streamed into the core; hosting the sign-in web view and handing cookies plus the exact User-Agent to the core;
delivering text (desktop paste to the captured target with clipboard restore; Android `commitText`) and reporting the real result;
rendering the state on its one surface; OS permission prompts and deep links; launch triggers (hotkey, keyboard mic, login item).

**Cut:** desktop `preferences.html`; the Android voice-only keyboard and `OverlayService` (keep one keyboard with the mic built in, always visible);
pause/resume; settings that are not wired.

## Interim fixes already in the Android working tree (not released)

- Launch check distinguishes Valid / Invalid / Unreachable; only Invalid goes to sign-in (`TranscriptionApi.checkSession`).
- Cloudflare HTML/403 is classified before the 401/403 auth check.
- Sign-in WebView uses the device's real engine User-Agent (minus `; wv`), stored and reused for API calls so `cf_clearance` stays valid. (inferred cause of the blank sign-in; needs on-device confirmation)
- Chunked `__Secure-next-auth.session-token.N` cookies are reassembled.
- Known gap: offline Home/Account still show a green "Connected" pill.
