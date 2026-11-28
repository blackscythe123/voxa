# Guardrails

Voxa's product decisions, written down as checks that fail the build. Each rule says what the owner
decided, why, and how it is checked. The checks read the source the apps compile (colours, strings,
calls), so they catch a broken decision the moment it is committed, not in review.

```
python tools/lint/run.py                 # every rule on the repo; exit 1 on any violation
python tools/lint/run.py --self-test     # prove every rule still catches its fixture violations
python tools/lint/run.py --only copy     # one group: contrast copy privacy focus android isolation release robust licence licences update
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
- Window descriptions (`describe()` in `desktop/app/src/win.rs` and `mac.rs`, logged on every
  take) never read the window title (`GetWindowTextW`, `title`): they name the program file and
  the window class only. Titles carry email subjects, chat and document names (1.0.0 review S15).

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
stays under 6.0 MB, no new dependencies without the owner's OK, no WorkManager (JobScheduler only),
and no exact alarms (the birthday wish uses an inexact window alarm, owner decision 2026-11-28).

**Why.** project_android_small_lean: a keyboard is always resident; every library costs size and memory.

**Check** (`android-size`, `android-deps`, `android-workmanager`, `android-exact-alarm`, `android.py`).

- Size: the APK at `android/app/build/outputs/apk/release/*arm64*.apk`, or `--apk PATH`; the size
  is printed, and over 6,000,000 bytes fails. CI does not build the APK, so there the check prints
  that it was skipped; run it after `./gradlew assembleRelease`, or with `--require-apk` in a job
  that builds one.
- Dependencies: every line in `android/app/build.gradle.kts`'s `dependencies {}` and `plugins {}` is
  on `ALLOWED_DEPS` / `ALLOWED_PLUGINS`. A new library means adding it there with its reason.
- WorkManager: `androidx.work`, `work-runtime`, `WorkManager`, `CoroutineWorker`, work requests,
  anywhere in the Android build files or sources.
- Exact alarms: `setExact*(` and `setAlarmClock(` calls, and the `SCHEDULE_EXACT_ALARM` /
  `USE_EXACT_ALARM` permissions, anywhere in the Android sources and manifest. Use
  `AlarmManager.setWindow` (as `notify/BirthdayWish.kt` does) or JobScheduler.
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

**Check** (`update-isolation`, `isolation.py`). In `desktop/app/src/update.rs`, every function
that builds the update client (`voxa_update::fetch::Client::new`) checks for a test copy
(`VOXA_DATA` or `test_hooks()`, in its body or in a same-file function it calls), so a test copy
uses its local test feed and never the real updater state. Starting the installer is covered by
`update-gate`: the install gate refuses any test copy.

## 7. Release

**Decision.** Release hygiene. Android GitHub releases never become the repo's "latest" release.

**Why.** The desktop download links use `.../releases/latest/download/Voxa-Setup.exe` (see
`desktop.yml`); an Android release marked latest breaks them.

**Check** (`release-latest`, `release.py`). Every `gh release create` / `gh release edit` in the
docs, scripts and workflows that mentions Android or an `.apk` passes `--latest=false`. (Commit
messages are already checked by a hook.)

The `update-feed` release that holds both signed feeds passes `--latest=false` or `--prerelease`
too (same rule, same file).

**Decision.** One version everywhere (auto update spec B6, 2026-11-28). The Cargo workspace version
is the single source for the desktop; Android's `versionName` matches it and its `versionCode`
follows `major*10000 + minor*100 + patch` (1.0.0 is 10000).

**Why.** The updater compares the running version with the feed. A mismatch between the binary,
the installer and the release tag makes it reinstall the same version forever, or downgrade.

**Check** (`release-version-sync`, `release.py`). The workspace version equals the default
`#define AppVersion` in `desktop/windows/voxa.iss` (and nothing hard-codes `AppVersion=x.y.z`),
`desktop.yml` passes `/DAppVersion` to ISCC and compares `GITHUB_REF_NAME` with the Cargo
version before publishing, and `android/app/build.gradle.kts` has the matching `versionName` and
`versionCode`.

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
- `robust-rename`: no bare `fs::rename`; every file replacement goes through `voxa_core::fsx::replace_file`, which retries for about 2 s while Windows reports the target busy (an antivirus scan, another save). A single try silently lost saves under load.
- `robust-atomic-write`: history, settings, session, account and other store files are written
  through the atomic writers (`voxa_store::write_atomic`, `desktop/app/src/store.rs`,
  `voxa_chatgpt::write_unique_then_rename`), never with a bare `fs::write` / `File::create` on the
  final path; and every write-temp-then-rename function uses a unique temp name (process id plus a
  counter or time), so two writers never clobber one temp file.

## 9. Licence

**Decision (owner, 2026-11-28):** Voxa is licensed Apache-2.0, with a `NOTICE` crediting
Simiyon Vinscent Samuel L that anyone redistributing Voxa must carry. Releases before 1.0.0
contained HeliBoard and were GPL-3.0; the code that remains is the owner's own.

**Check:** `licence-consistent` (tools/lint/voxa_lint/licence.py) fails if `LICENSE` is not the
Apache License 2.0, `NOTICE` is missing or doesn't name the author, `Cargo.toml`'s workspace
licence isn't `Apache-2.0`, or the README's License section doesn't start with Apache-2.0.

## 10. Update

**Decision.** Voxa updates itself only from signed feeds on GitHub, never during a take, and the
signing keys never enter the repo (auto update spec, owner approved 2026-11-28; `docs/updates.md`,
`docs/update-keys.md`).

**Why.** An updater is the most powerful code in the app: a bad host, a leaked key or an install in
the middle of a dictation would hurt every user at once.

**Checks** (`update.py`):

- `update-hosts`: the updater's code (`core/voxa-update/src`, `desktop/app/src/update.rs`,
  `notify/Updater.kt`, outside tests) has no plain `http://` URL, never calls
  `danger_accept_invalid*`, and every URL literal is under `https://github.com/blackscythe123/voxa/`
  or `release-assets.githubusercontent.com` / `objects.githubusercontent.com`.
- `update-key-present`: `core/voxa-update/src/keys.rs` holds at least 2 keys of 64 hex characters
  with unique ids. All-zero placeholders are a note in everyday runs and a violation with
  `--release` (`python tools/lint/run.py --release`; CI passes it on every `v*` and `android-v*`
  tag in `desktop.yml` and `guardrails.yml`). The crate's `build.rs` also fails a
  `VOXA_RELEASE_BUILD=1` build with placeholders (section 12 has every release guard).
- `update-secret`: no PEM private key block, no Ed25519 PKCS#8 prefix (base64 of
  `302e020100300506032b6570`) and no `.jks`, `.keystore`, `.p12` or `.pfx` file in the tree, outside
  the labelled TEST fixtures (`core/voxa-update/tests/fixtures/`, `tools/lint/fixtures/`).
- `update-gate`: the only desktop function that starts the installer (a `.spawn(` with
  `/VERYSILENT` or `installer_args(`) is `update.rs` `launch_installer()`, and it calls
  `can_install_now` (or `can_install_on_request`) and `verify_file` before it spawns. On Android,
  every PackageInstaller `commit(` in `Updater.kt` comes after `dictationLive()` in the same
  function.
- `android-release-key`: `buildTypes.release` never names `signingConfigs.getByName("debug")`; the debug
  fallback lives in a helper that only a dev release (`-Pvoxa.devRelease=true`) reaches. The gradle
  build itself refuses every other release without the release key (section 12).
- `android-onboarding-permissions`: `SetupScreen.kt` only routes to the microphone permission, the
  keyboard settings and `ACTION_MANAGE_UNKNOWN_APP_SOURCES`; never `POST_NOTIFICATIONS`.

The existing copy, privacy, focus, robustness, contrast and APK size rules apply to the update
code unchanged.

## 11. Licences

**Decision (owner, 2026-11-28):** both apps carry the licences of everything they ship, under
Settings, Open source licences: Voxa's own NOTICE first, then every library (name, version,
licence) with its licence text. Every shipped licence must allow distributing Voxa under
Apache-2.0.

**How it is built.** `python tools/licences/generate.py` reads the real dependency graph and writes,
all committed:

- `THIRD_PARTY_NOTICES.txt` (repo root): every component of every app, a per-app summary by licence,
  and each licence or NOTICE text once, numbered (T1, T2, ...).
- `tools/licences/inventory.tsv`: kind, name, version, licence (SPDX), apps. The lint reads this.
- `desktop/app/assets/licences-windows.json.gz` and `licences-macos.json.gz` (the desktop embeds its
  platform's copy and decompresses it with flate2, already in GPUI's tree), and
  `android/app/src/main/assets/licences.json.gzip` (read with `GZIPInputStream`; not `.gz`, which the Android build unpacks and stores twice as large).

Sources: Rust crates linked into each artefact (`cargo tree -e normal,no-proc-macro` per target:
voxa-desktop and voxa-signin for Windows and macOS, voxa-ffi for the three Android ABIs; build
scripts and proc macros never ship), licences from `cargo metadata`, texts from each crate's
LICENSE, COPYING and NOTICE files in the cargo registry (every NOTICE verbatim, Apache-2.0 section
4(d)), plus the Rust standard library. Android: the release runtime classpath, stored in
`tools/licences/android-deps.txt` (`--gradle` re-resolves it; needs the Android SDK), licences
from each POM. Assets: the `ASSETS` table in `tools/lint/voxa_lint/licences.py` (fonts, the Silero
model, ONNX Runtime with its own third-party notices). Licence texts that do not come with a package
live in `tools/licences/texts/`. JNA is dual licensed; Voxa uses it under Apache-2.0, and the page
says so.

After any dependency change: `python tools/licences/generate.py` (add `--gradle` when the Android
dependencies changed), commit the outputs.

**Checks** (`licences.py`, no network and no Android SDK, so CI runs them):

- `licences-stale`: the notices, the inventory and the three app copies carry the dependency
  fingerprint of the checkout: Cargo.lock (Voxa's own version numbers blanked), the dependency
  sections of the workspace's Cargo.toml files, `android-deps.txt`, NOTICE, LICENSE, the assets,
  the onnxruntime pin and the stored texts. `android-deps.txt` itself records the digest of
  `libs.versions.toml` and the app's `dependencies { }` block it was resolved from.
- `licences-allowed`: every inventory licence passes the allow-list: MIT, MIT-0, Apache-2.0,
  BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, Unicode-DFS-2016, MPL-2.0 (file level, fine
  unmodified), CC0-1.0, OFL-1.1, BSL-1.0, 0BSD, Unlicense, CDLA-Permissive-2.0 (webpki-roots' root
  certificate data, approved by the owner on 2026-11-28), with `WITH LLVM-exception`. An `OR`
  passes when one side does, an `AND` when both do. GPL, AGPL, LGPL-only, SSPL and anything unknown
  fail.
- `licences-asset`: every font, sound, model and native library in the app trees (`res/font`,
  `res/raw`, Android `assets`, desktop `assets/*.ttf`, `core/*/model/*.onnx`, `vendor/`) is listed in
  `ASSETS`.

Tests: the desktop's `licences::tests` (`the_embedded_licences_decompress_and_start_with_voxas_notice`:
the embedded copy decompresses and its NOTICE is today's NOTICE; every library has a licence and a
text) and the Android instrumented `LicencesTest` (the asset loads, NOTICE first, androidx.core,
kotlin-stdlib, JNA under Apache-2.0, uniffi and Manrope are listed; Settings opens the page and a
library's licence text shows; passes on the debug build and the R8 release build). Screenshots:
`docs/verification/licences/`.

## 12. Release pipeline

**Decision.** The Voxa 1.0.0 final release review (2026-11-28): no build that could strand its
users ever ships, Android 1.0.0 on is signed with one permanent release key (no lineage from the
debug key; test builds are uninstalled once), the update feeds only move forward and can be
paused, and every release follows `docs/release-runbook.md` with its real-hardware gate.

**Why.** 1.0.0's updater is the only update path users will have; a stranded build, a wrong signer
or an older feed can only be repaired by each friend reinstalling by hand.

**Checks** (`pipeline.py`):

- `release-placeholder-guard`: `core/voxa-update/build.rs` calls `release_refused()`,
  `android/app/build.gradle.kts` refuses a release with placeholder keys (`releaseRefusal`), and
  `desktop/windows/voxa.iss` reads `keys.rs` and stops ISCC with `#error` unless
  `/DAllowPlaceholderKeys`.
- `release-android-signing`: no `--lineage` / `--next-signer` / `apksigner rotate` in the build;
  `releaseKeyIsDebug()` and `verifyReleaseSigner` exist; `android/signing/forbidden-signers.txt`
  lists the test-build signer `e648aa43...`; `android-feed.yml` checks the pinned and forbidden
  certificates. With `--release`, `android/signing/release-cert.sha256` must hold a digest that is
  not forbidden.
- `release-feed-guard`: `android-feed.yml` refuses a run that is not from `refs/tags/$TAG`; both
  feed jobs run `voxa-update-tool guard-forward`; every job that writes `k.pem` has
  `trap 'rm -f k.pem' EXIT` and an empty-secret check; `feed-pause.yml` (the kill switch) exists.
- `release-readme-links`: every README download link names a file the release uploads for the
  workspace version, under the right tag (or a version-free `latest` copy).
- `release-runbook`: `docs/release-runbook.md` has the REAL-HARDWARE RELEASE GATE and the README
  links it.

Tests: `voxa-update` `keys::tests::guard::a_release_build_with_placeholder_keys_fails_unless_the_dev_flag_is_set`,
`policy::tests::the_feed_version_only_moves_forward_when_publishing`, the desktop
`update::tests::a_build_with_placeholder_keys_says_updates_are_off`, the Android instrumented
`UpdateSafetyTest`. The ISCC and Gradle refusals were run by hand on 2026-11-28 (both fail with
placeholder keys; both build with the dev flag).

## Open items found when the guardrails were added

These are on the allow-list with their reasons and should be fixed, then deleted from it:

- Desktop app badges: white letters on several brand colours (WhatsApp, Gmail, YouTube, ...) are
  under 4.5:1; the Settings avatar fallback (white on `#10A37F`) is 3.2:1.
- (Fixed since, see regressions.md: `notify.json` and the fixed `.tmp` name in `write_atomic`.)
