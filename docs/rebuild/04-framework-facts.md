# Rune vs GPUI: gathered facts (2026-11-22)

Collected with `gh` and the crates.io API on 2026-11-22. Source clones for review:
- Rune: `C:\Users\Sam\AppData\Local\Temp\rune` (branch `dev`, shallow clone, so local git history is 1 commit; real history below)
- GPUI: `C:\Users\Sam\AppData\Local\Temp\zed-gpui\crates\gpui`, `gpui_platform`, `gpui_windows` (sparse clone of zed-industries/zed)

## Popularity and activity

| | GPUI | Rune |
|---|---|---|
| Home repo | zed-industries/zed (GPUI is a crate inside the Zed editor) | anomalyco/rune (by Anomaly, makers of opencode) |
| Repo stars | 95,505 (whole Zed editor, not GPUI alone) | 7,779 |
| Forks | 11,802 (Zed) | 331 |
| Repo created | 2021-02-20 | 2026-10-22 (about one month old) |
| Commits | (Zed: very large; not counted) | 2,628 on `dev` |
| Contributors | about 2,431 (Zed, incl. anonymous) | about 15 (13 named) |
| Last push | 2026-11-22 | 2026-11-22 |
| Published on crates.io | yes, `gpui` 0.2.2; 541,892 total downloads, 313,332 in the last 90 days; 163 reverse dependencies | no; git dependency only ("pin a rev of dev") |
| Component library | longbridge/gpui-component, 21,182 stars, active | `rune-kit` in-repo |
| Releases | crate versions (0.2.x, pre-1.0) | none ("Rune has no releases yet") |
| Contribution model | normal PRs | "We don't take pull requests"; open an issue, their agents fix it |
| Issues | n/a | latest 200 issues: 194 open, 6 closed |
| Known big adopters | Zed; Discord announced (Nov 2026) its desktop rebuild uses GPUI | opencode (Anomaly's own apps, per README demos) |

## Voxa-relevant capabilities (verified by reading source)

| Need | GPUI | Rune |
|---|---|---|
| Global hotkey | not found in gpui crates (would need e.g. `global-hotkey` crate) | `core/rune/src/shortcuts/mod.rs`: `global_shortcut(accelerator, handler)` |
| Transparent always-on-top overlay | window options exist in platform code (verify) | `transparent(...)` in `runtime/elements.rs`; topmost/always_on_top in platform code; `examples/overlay`, `overlay-lab` |
| Tray icon | not found in gpui crates (would need e.g. `tray-icon` crate) | tray references in `core/rune/src` (verify API) |
| Accessibility | AccessKit (`window/a11y.rs`, `examples/a11y.rs`) | AccessKit (vendored `accesskit_windows`, `accesskit_macos`; `src/a11y/`) |
| Webview (ChatGPT sign-in) | none; add `wry` | none; add `wry` |
| Android | no | no (iOS embed only) |
| Headless testing | not found | README: every app can run headless, render PNG, dump element tree, scripted input |
| Platforms | macOS, Windows, Linux, iOS, web | macOS, Windows, Linux, web |
