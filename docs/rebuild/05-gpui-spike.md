# GPUI strip spike on Windows (2026-11-22)

Code: `spikes/gpui-strip/` (gpui 0.2.2 from crates.io, global-hotkey 0.10, tray-icon 0.26, windows-sys 0.59).
Machine: Windows 11, 2560x1600 at 150% scaling, Rust 1.100.0, VS 2022 Build Tools.
Method: a real desktop run driven by `test-harness.ps1` (real mouse clicks and key presses) against a stand-in target
app (`target.ps1`, a WinForms text box that mirrors its text to a file). Every action logs the foreground window to `spike.log`.

## Results

| Go/no-go check | Result | Evidence |
|---|---|---|
| Builds on Windows (MSVC) | Pass | first build 1m54s, no code errors |
| Renders the v7 strip natively (Segoe UI, mark, translucent surface, buttons, live ink line as a GPU path) | Pass | screenshots `s5`, `s6`, `s9-reshown` |
| Topmost, no Windows 11 frame border | Pass | `WS_EX_TOPMOST` + `DWMWA_BORDER_COLOR = none` |
| **Clicking strip buttons keeps focus in the target app** | **Pass** | `CLICK Start` / `CLICK Stop` logged with foreground = target, before and after, 3 of 3 clicks |
| Global hotkey (Ctrl+Alt+Space) from inside another app | Pass | 4 of 4 presses received, foreground unchanged |
| Tray icon whose dot follows state | Pass | red dot while recording, seen in the tray overflow (`s8`) |
| Paste into the app you started in | Pass, 4 of 5 confirmed | text landed; 1 paste logged as sent but not found in the target (see open issues) |
| Clipboard restored after paste | Pass (plain text) | sentinel text back on the clipboard after every paste |
| Target lost focus before paste: bring it back, then paste | Pass | "target not in front ... bringing it forward" then pasted |
| Strip hidden between dictations, re-shown on hotkey without stealing focus, repaints | Pass | `strip shown (no activate)`, foreground = target, screenshot shows live repaint |
| First show at app launch does not steal focus | Pass with workaround | GPUI activates on first show; we hand focus straight back (same tick) |

**Verdict: GPUI passes the go/no-go.** Focus safety needed no fork, only Win32 code we own (~150 lines in `src/win.rs`):
add `WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`, answer `WM_MOUSEACTIVATE` with `MA_NOACTIVATE` through a window
subclass, and show/hide with `SW_SHOWNOACTIVATE` / `SW_HIDE`. Note: gpui 0.2.2 (crates.io) does not override
`WM_MOUSEACTIVATE`; newer unreleased GPUI does (`MA_ACTIVATE`, `events.rs:95`), so a future upgrade must keep the subclass working.

## Open issues

1. **One paste in five unaccounted for.** The log says Ctrl+V was sent to the target; the target text shows one copy fewer
   than expected after a two-cycle run. Not explained by this data. Needs a stress test: fresh target each run, 50 cycles,
   count pastes, and check whether the target reads the clipboard after our 250 ms restore (would then need a longer or
   adaptive delay, or waiting for the target to release the clipboard).
2. **Faint dark panel behind the strip.** The transparent window area shows a dim rounded rectangle. Likely the window
   background/compositor; size the window to the strip or fix the transparency.
3. **Launch focus:** creating the window hidden and showing it outside GPUI left it unpainted; we instead let GPUI show it
   and hand focus back. Fine for a tray app that creates the strip once at startup.
4. **Clipboard restore covers plain text only** in the spike. The real app must save and restore every clipboard format.
5. **Not covered:** two monitors, a 100% scaling display, macOS, the ChatGPT sign-in helper (`wry` in a separate process),
   real audio capture, and the tray icon being hidden in the overflow by default (onboarding should ask to pin it).

## Harness notes

- The test drives the real desktop: run it only when nobody is using the PC (one run was invalidated by concurrent use).
- The harness overwrites the clipboard with a sentinel to test restore; save the user's clipboard *before* the first run.
