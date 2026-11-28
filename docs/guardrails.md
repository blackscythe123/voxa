# Guardrails

Voxa's product decisions, written down as checks that fail the build. Each rule says what the owner
decided, why, and how it is checked. The checks read the source the apps compile (colours, strings,
calls), so they catch a broken decision the moment it is committed, not in review.

```
python tools/lint/run.py                 # every rule on the repo; exit 1 on any violation
python tools/lint/run.py --self-test     # prove every rule still catches its fixture violations
python tools/lint/run.py --only copy     # one group: contrast copy privacy focus android isolation release robust
python tools/lint/run.py --apk PATH      # also check an APK's size (a locally built one is found by itself)
python tools/lint/run.py --keys          # show each violation's allow-list key
cargo test --workspace                   # the Rust tests (desktop on Windows/macOS; core everywhere)
```

Python 3.10 or newer, standard library only. CI runs both lint commands on every push and pull
request (`.github/workflows/guardrails.yml`, Linux, under a minute), plus `cargo test` for the
shared core on Linux; `desktop.yml` runs `cargo test --workspace` on Windows and macOS and will not
publish a tagged release unless the guardrails pass.

Known exceptions are in `tools/lint/allowlist.py`, each with its reason. The runner warns when an
entry no longer matches anything, so fixed code takes its entry with it.

## Every new bug gets a test, every new decision a lint

- **A bug fix comes with a regression test** that fails before the fix and passes after: a Rust
  `#[test]` next to the code (or in the crate's `tests/`), or an Android instrumented test. Name it
  after the behaviour ("a test copy never reads the old app's folder"), not the ticket.
- **A product decision comes with a guardrail.** When the owner decides how Voxa must always (or
  must never) behave, add the check to `tools/lint/voxa_lint/`, a Bad and a Good case to
  `tools/lint/fixtures/bad/`, the count to `tools/lint/fixtures/expected.txt`, and a section here.
  The self-test fails if a rule has no fixture.
- An allow-list entry is a debt with a reason, not a way around a rule. Prefer fixing the code.

## 1. Contrast

**Decision.** WCAG AA at least everywhere, in light and dark: 4.5:1 for text, 3:1 for large text,
icons and component edges. AAA (7:1) for body and secondary text. The brand sepia is `#7A3F22` in
light mode; dark mode uses a lighter sepia that still passes on the dark surfaces.

**Why.** Voxa is used at a glance while dictating; muted grey that only "looks fine" on one screen
fails in sunlight and for low vision (feedback_wcag_contrast).

**Check** (`contrast`, `tools/lint/voxa_lint/contrast.py`). Reads the colours from source:

- Desktop: every `const X: Palette = Palette { ... }` under `desktop/app/src` (today `theme.rs`
  LIGHT and DARK; a palette moved to another file is found the same way). Text, nav, card, hover,
  strip, caption and InfoBar pairs at AAA; accent, status and button text at AA; icons, toggles,
  strip glyphs at 3:1. A light palette whose accent is not `#7A3F22` fails. Colour literals drawn
  straight into an element (`.bg(rgb(..))` with `.text_color(rgb(..))`) are checked too, and a
  computed background (an app's brand colour) is checked against every colour literal in its function.
- Android app: `LightVx` / `DarkVx` in `ui/Theme.kt`: text, `text72` body and `muted` at AAA on the
  page, cards and surface; status text and accent at AA; status icons, `text40` icons and the off
  switch at 3:1; `onAccent` on the accent at AA.
- Keyboard: `KbColors.voxa()` light and dark (text and muted at AAA, key labels at AA, the mic and
  its icon), every solid preset in `KeyboardLook.kt` (computed the way `KbColors.solid()` does), and
  every mic swatch with its icon.

The desktop's own `theme.rs` test checks the same palette in Rust; this lint adds AAA for secondary
text, the Android app and keyboard, and literals outside the palette.

## 2. Copy

**Decision.** No emojis in the in-app UI (Voxa's own icons instead; the Android emoji panel is
exempt because it is user input). No dashes as punctuation (` - `, en dash, em dash). Sentence
case for titles and buttons. Culture-neutral: no festival, religious or national occasion names
(Diwali, Pongal, Eid, Christmas, Holi, Republic Day, Independence Day, ...).

**Why.** One calm voice everywhere, readable by screen readers, and a product for everyone
(feedback_no_emojis; the notices module's copy rules).

**Check** (`copy-emoji`, `copy-dash`, `copy-case`, `copy-occasion`, `copy_rules.py`). Every string
literal under `desktop/`, `core/*/src` and `android/app/src/main` plus `strings.xml`, except tests,
log lines, assertions, environment names, font names and diagnostics. Placeholders like `{ts}-{n}`
and Kotlin `${a - b}` are not dashes. Sentence case: a 2 to 8 word label with every word
capitalised fails unless the capitals are names on the `PROPER` list (Voxa, ChatGPT, Windows,
Ctrl, the page names Home, History, Insights, Words, Settings, ...). Files with "emoji" in the
name are exempt from the emoji rule only.

## 3. Privacy

**Decision.** What the user said, their cookies and tokens never appear in a log, in any build.
Notifications never contain transcript text: they name the app or a count.

**Why.** Logs are shared for support and live on disk; a lock screen is public.

**Check** (`privacy-log`, `privacy-notify`, `privacy.py`).

- Log calls (`log`/`os::log`, `println!`/`eprintln!`, `log` macros, the core's `diag::log`,
  Android `Diag.log` and `Log.*`) that interpolate `text`, `transcript`, `said`, `cookie`,
  `cookie_header`, `token` and similar, or Debug-format a whole `event`, `effect`, `entry`. Lengths
  are fine (`.len()`, `.length`, `.chars().count()`), and so is anything through the desktop's
  `redact` module (`desktop/app/src/redact.rs`) or the core's `diag::text` / `diag::event`.
- Notification code (`desktop/app/src/notify*.rs`, `core/voxa-core/src/notices.rs`,
  `core/voxa-ffi/src/notices.rs`, Android `Notifier.kt`): no function takes a `text` or
  `transcript` parameter, and Android's `setContentTitle` / `setContentText` / `bigText` /
  `setTicker` never get such a value.

## 4. Focus

**Decision.** The desktop app never steals focus. The strip appears without activating; the tray,
toasts and strip never pull the keyboard away from the app being dictated into. macOS toast
buttons run in the background, except Open and Sign in.

**Why.** Dictation pastes into the app that has focus; taking focus loses the paste and the
user's place.

**Check** (`focus-steal`, `focus-notify`, `focus.py`). Under `desktop/app/src`:
`SetForegroundWindow`, `SetActiveWindow`, `BringWindowToTop`, `SwitchToThisWindow`,
`activateIgnoringOtherApps`, `makeKeyAndOrderFront`, `activateWithOptions`, gpui
`activate_window()` / `activate(true)`, and `ShowWindow` with `SW_SHOW`, `SW_SHOWNORMAL`,
`SW_RESTORE` and friends fail, except the (file, function, call) entries in `ALLOWED`, each with
its reason: handing focus back to the app that had it, switching to the paste target after
dictating from Voxa's own window, and bringing the window forward when the user asked for it
(tray Open Voxa, a second launch). `category(tag, label, true)` (UNNotificationActionOptionForeground)
fails unless the label is Open or Sign in.

## 5. Android

**Decision.** Size and lean. Android is a small, low-RAM voice keyboard on the Rust core: the release arm64 APK
stays under 6.0 MB, no new dependencies without the owner's OK, and no WorkManager (JobScheduler only).

**Why.** project_android_small_lean: a keyboard is always resident; every library costs size and memory.

**Check** (`android-size`, `android-deps`, `android-workmanager`, `android.py`).

- Size: the APK at `android/app/build/outputs/apk/release/*arm64*.apk`, or `--apk PATH`; the size
  is printed, and over 6,000,000 bytes fails. CI does not build the APK, so there the check prints
  that it was skipped; run it after `./gradlew assembleRelease`, or with `--require-apk` in a job
  that builds one.
- Dependencies: every line in `android/app/build.gradle.kts`'s `dependencies {}` and `plugins {}` is
  on `ALLOWED_DEPS` / `ALLOWED_PLUGINS`. A new library means adding it there with its reason.
- WorkManager: `androidx.work`, `work-runtime`, `WorkManager`, `CoroutineWorker`, work requests,
  anywhere in the Android build files or sources.
- Signing (`android-signing`): `build.gradle.kts` enables the v1, v2 and v3 signature schemes, so
  every installer takes a sideloaded APK (the universal APK once gave "There was a problem parsing
  the package").

## 6. Isolation

**Decision.** Test isolation. A test copy of the desktop app (`VOXA_DATA` set) never reads or writes the real
user's data or the old app's folder, and test hooks (`VOXA_TEST_*`) only work in a test copy.

**Why.** Tests and verification runs happen on the owner's own machine, next to real History and
a real ChatGPT sign-in.

**Check** (`isolation-env`, `isolation-test-hook`, `isolation.py`). Under `desktop/`: a function
reading `HOME`, `APPDATA`, `LOCALAPPDATA` (and similar), a `dirs::*_dir()`, or calling
`app_data_root()` must also check `VOXA_DATA` (the old app's folder is reached only through
`app_data_root()`). Every `VOXA_TEST_*` read sits in a function that checks `VOXA_DATA`, directly or through
`test_hooks()`, which also needs a debug build or `VOXA_TEST=1`.
`ALLOWED` lists `app_data_root` itself (only called from the guarded `data_dir` and
`electron_dir`) and macOS `launch_agent` (the OS login item, not app data).

## 7. Release

**Decision.** Release hygiene. Android GitHub releases never become the repo's "latest" release.

**Why.** The desktop download links use `.../releases/latest/download/Voxa-Setup.exe` (see
`desktop.yml`); an Android release marked latest breaks them.

**Check** (`release-latest`, `release.py`). Every `gh release create` / `gh release edit` in the
docs, scripts and workflows that mentions Android or an `.apk` passes `--latest=false`. (Commit
messages are already checked by a hook.)

## 8. Robustness

**Decision.** Rules from the bug hunt, so the same bugs do not come back.

**Check** (`robust.py`):

- `robust-case-slice`: no byte-index slicing of user text after `to_lowercase()` /
  `to_uppercase()` in the same function (Kotlin: `.lowercase()` then `.substring(`). Case changes
  can change byte lengths ("İ" is 2 bytes, its lowercase 3), so an index from the lowered copy can
  split a character in the original. ASCII case changes are fine. Best effort.
- `robust-unwrap`: no `.unwrap()` / `.expect()` in `core/*/src` library code outside tests (the
  nudge trainer in `src/bin` is exempt). Network, file and user input must never panic the
  keyboard or the app; lock poisoning uses `unwrap_or_else(|e| e.into_inner())`.
- `robust-atomic-write`: history, settings, session, account and other store files are written
  through the atomic writers (`voxa_store::write_atomic`, `desktop/app/src/store.rs`,
  `voxa_chatgpt::write_unique_then_rename`), never with a bare `fs::write` / `File::create` on the
  final path; and every write-temp-then-rename function uses a unique temp name (process id plus a
  counter or time), so two writers never clobber one temp file.

## Open items found when the guardrails were added

These are on the allow-list with their reasons and should be fixed, then deleted from it:

- Desktop app badges: white letters on several brand colours (WhatsApp, Gmail, YouTube, ...) are
  under 4.5:1; the Settings avatar fallback (white on `#10A37F`) is 3.2:1.
- `account.json` (desktop `save_account`) and `notify.json` (desktop `Notifier::save`) are written
  with a bare `fs::write`.
- Desktop `store.rs` `write_atomic` uses a fixed `.tmp` name.
