# Old (Electron) vs new (GPUI) Voxa: startup and memory

Measured 2026-11-22 on the owner's Windows 11 laptop by `rebuild/apps/desktop/tests-e2e/compare.ps1`
(real hotkeys via SendInput; "visible" = a strip-sized window of the app becomes visible, polled every 5 ms).
Both apps were already running in the tray, as in normal use. One run each, nobody speaking.

| | Old Voxa (Electron, Ctrl+Space) | New Voxa (GPUI, Ctrl+Alt+Space) |
|---|---|---|
| Hotkey to strip visible | **3,113 ms** | **33 ms** (~94x faster) |
| Processes | 6 | 1 |
| Private memory, idle | 447 MB | 65 MB |
| Private memory, recording | 458 MB | 67 MB |
| Working set, recording | 156 MB | 80 MB |

Private memory is the honest comparison: Electron's idle working set (72 MB) was low only because
Windows had paged it out; it jumped back to 156 MB as soon as the strip opened.

Evidence: screen recording `old-vs-new.mp4` (32 s, local, not committed): the old strip appears at
the bottom-left about 3 s after its hotkey; the new strip appears bottom-centre on the keypress, then
shows "Didn't hear any speech".

Caveats: single run; the new app is not feature-complete yet (no main window), so its memory will
grow somewhat as History and Settings land.
