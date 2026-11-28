# Auto update (1.0.0)

How Voxa updates itself, built from the approved auto update spec (EGM design review, 2026-11-28)
plus the owner's decisions on its open questions. Keys and signing: `docs/update-keys.md`. Rules
that guard it: `docs/guardrails.md` sections 6, 7 and 10. Notifications: `docs/notifications.md`
(the update spec's A4 is called A5 there, because the birthday wish took A4 first).

## Pieces

| Piece | Where |
|---|---|
| Verify, parse, versions, gate, schedule, capped download | `core/voxa-update` (ring Ed25519 and SHA-256, already linked through rustls; reqwest, already linked) |
| CI and test tool (`verify`, `sign`, `fetch-check`) | `core/voxa-update/src/bin/voxa-update-tool.rs` (feature `sign`, never in an app) |
| Desktop updater (tray process only) | `desktop/app/src/update.rs`, Settings and Home bars in `home.rs`, tray item in `main.rs` |
| Installer | `desktop/windows/voxa.iss` (`/VOXAUPDATE`, mutex wait, no UAC, relaunch with `--background --updated`) |
| Android | `core/voxa-ffi/src/update.rs`, `notify/Updater.kt`, `notify/UpdateReceiver.kt`, jobs 7104 and 7105 in `VoxaJobs.kt`, `ui/UpdateUi.kt`, onboarding step in `SetupScreen.kt` |
| Feeds | `.github/workflows/desktop.yml` job `feed`, `.github/workflows/android-feed.yml`, both on the `update-feed` prerelease |
| Local TEST feeds | `tools/update/make_test_feed.py` (signs with the TEST key in `core/voxa-update/tests/fixtures/`) |

## Cheap checks (owner rule, 2026-11-28)

- At most one automatic check per 24 hours per device, at this install's own time of day (a random
  slot stored once), never on every start. Android's job 7104 runs every 12 hours but the core
  only goes to the network when the check is due.
- A failed check backs off 1 hour, then 4 hours, then the next day's slot. Never a tight retry.
- "Check for updates" by hand goes to the network at most once a minute.
- The feed's ETag and Last-Modified are kept and sent back (`If-None-Match`,
  `If-Modified-Since`), so an unchanged feed is a 304 with no body. Checked on 2026-11-28 with
  curl against a real release asset: `github.com/.../releases/download/...` redirects to
  `release-assets.githubusercontent.com`, which sends `ETag` and `Last-Modified` and answers
  both conditional headers with 304 and 0 bytes. The feed assets live on the same host
  (inferred from that, not yet seen for `update-feed` itself, which doesn't exist yet).
- A version downloads once: a staged file that still verifies is never fetched again. Android
  downloads on unmetered networks only.
- Tests: `voxa_update::schedule` (`check_due_respects_24_hours_and_the_per_install_offset`,
  `the_manual_check_rate_limit_is_once_a_minute`, `the_backoff_sequence_is_one_hour_four_hours_then_the_next_day`),
  `voxa_update::fetch` (`a_304_keeps_the_cached_feed`, `a_verified_download_is_never_fetched_again`,
  `a_check_with_state_records_backoff_and_serial`), desktop `a_start_does_not_check_when_not_due`,
  `the_manual_button_within_a_minute_does_no_network`, Android
  `UpdateFfiTest.a_second_check_right_after_one_does_no_network`.

## Testing with a local feed

Never against the installed Voxa, never running a real installer silently on a PC in use.

```bash
python tools/update/make_test_feed.py desktop --version 9.9.9 --base http://127.0.0.1:8765/feed \
  --asset windows-x64=some-file.bin --out /tmp/feed/feed/desktop.json
python -m http.server 8765 --bind 127.0.0.1 --directory /tmp/feed
```

Desktop test copy: a new temporary `VOXA_DATA`, `VOXA_TEST=1`,
`VOXA_TEST_UPDATE_FEED=http://127.0.0.1:8765/feed/desktop.json` and
`VOXA_TEST_UPDATE_KEY=<core/voxa-update/tests/fixtures/test-k1.pub.hex>`. The gate always refuses
a test copy (`test_copy`), so it stages but never installs.

Android (debug builds only; release builds ignore it):
`adb -s emulator-5554 shell "am start -n com.voxa.android/.MainActivity --es debug_update_feed http://10.0.2.2:8765/feed/android.json --es debug_update_key <hex> --ez debug_update_check true"`.

## Verification (2026-11-28)

Engines: Rust unit tests on Windows 11; the desktop release exe as a test copy; Android on the API
35 emulator (`emulator-5554`, x86_64). macOS was not run (no Mac here).

| Leg | Evidence | Status |
|---|---|---|
| Signature, envelope, channel, serial, schema, versions, URLs, caps, staging, gate, attempts | `cargo test -p voxa-update` (50 tests, spec 13.1 names) | Unit tested |
| OpenSSL 3 signatures (what CI makes) verify in the shipped verifier | `a_known_answer_vector_verifies` (fixture signed with `openssl pkeyutl -sign -rawin`) | Unit tested |
| A5 copy, once per version, deferred under the cap | `voxa-core` notices tests (spec 13.2 names) | Unit tested |
| Desktop: settings, links, installer arguments, re-verify before launch, no window on `--background --updated`, logs kept | desktop tests (spec 13.3 names, plus log rotation) | Unit tested |
| Installer script compiles | ISCC with stub binaries into a temp folder, exit 0 (never run) | Compiled only |
| Desktop check and download from a local signed feed | Test copy log `checked: feed 9.9.9 ... offer`, `downloaded 9.9.9 (300000 bytes), verified`; status "Voxa 9.9.9 is ready. It installs by itself the next time you're not using Voxa." | Seen in the log and live.json |
| Desktop gate refuses a test copy | Log `install waits: test_copy`, `not installing at sign-in: waiting: test_copy`; file never launched | Seen in the log |
| Real silent install while locked, relaunch with no window or focus change, rollback on a locked file, attempt cap, no UAC with the old Electron Voxa present | Needs a real install in `{localappdata}\Programs\Voxa` | Not verified (manual) |
| GPUI bars and status line read by Narrator or VoiceOver | Not checked | Not verified |
| Android onboarding step: Allow, Not now, resume after the process restart | Emulator: system "Install unknown apps" screen, then "Updates allowed" and Continue (`onboarding_ask=granted`); Not now moved to "Switch to Voxa and try it" (`skipped`); revoking killed the process and setup resumed on the step | Rendered |
| Android full flow from a local signed feed | Server log: feed then APK; `files/updates/...apk` staged; Home card "Voxa 0.3.1 is ready to install"; A5 held while on screen and posted after Home (id 4, channel updates, importance 2, public title "Voxa update ready"); tap showed the system "Do you want to update this app?" dialog; Cancel gave "Voxa wasn't updated. You're still on 0.3.0." with Try again; Update installed 0.3.1 in place; jobs 7101 and 7104 rescheduled; Home "Voxa was updated to 0.3.1." | Rendered |
| Android instrumented tests (UpdateFfiTest, UpdateNotificationTest, UpdateJobTest, UpdateInstallTest, SetupUpdateStepTest) | `connectedDebugAndroidTest` on the emulator, after the merge with main: 60 of 61 pass; the one failure is main's `RegressionsTest.theBirthdayIsDayAndMonthOnlyAndStaysOnThePhone` (a `birthday_wish` pref left on the emulator, not update code) | Tested |
| Android API 29, Play Protect, the v3 lineage update from a 0.3.0 debug-signed install | Only an API 35 emulator here; no release key yet | Not verified (manual) |
| Android cleanup of `filesDir/updates` after `MY_PACKAGE_REPLACED` | The "updated" note is written after the cleanup; the folder couldn't be listed on the release build | Inferred |
| macOS feed check, "is out" bar, Accessibility bar, CI signing and the designated requirement guard | Code only, not compiled | Not verified |
