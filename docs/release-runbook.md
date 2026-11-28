# Release runbook

The ordered steps to cut a Voxa release, from the 1.0.0 final release review (2026-11-28). Do them
in this order; each step says what proves it worked. Keys and signing details are in
[update-keys.md](update-keys.md); how the updater behaves is in [updates.md](updates.md).

1.0.0 matters more than any later release: its updater is the only update path friends will have.
A bug in it (no relaunch, a UAC prompt, a blank strip, a signer mismatch) can only be fixed by each
friend reinstalling by hand. So nobody but the owner gets 1.0.0 until the
**REAL-HARDWARE RELEASE GATE** (section C) has passed.

## A. One-time owner setup (before the first tag)

| # | Step | Proof |
|---|---|---|
| A1 | Generate K1 and K2, commit their public keys in `core/voxa-update/src/keys.rs` (update-keys.md steps 1 and 3) | `cargo test -q --release -p voxa-update` and `python tools/lint/run.py --release` pass `update-key-present` |
| A2 | Create the `release` Environment (required reviewer: you; tags `v*` and `android-v*`), set `VOXA_UPDATE_KEY` (update-keys.md step 2) | `gh secret list --env release` shows it |
| A3 | Create the Android release keystore with keytool, back it up offline, commit its certificate SHA-256 to `android/signing/release-cert.sha256`, add `voxa.release.*` to `android/local.properties` (update-keys.md step 6). Never regenerate it | `python tools/lint/run.py --release` passes `release-android-signing` |
| A4 | Mac: create the stable certificate and its two secrets (update-keys.md step 7), or leave the Mac row marked "not yet tested" in the README | The tag's macos job log says "signing with the stable identity" |
| A5 | Create the `update-feed` prerelease (update-keys.md step 5). **Before the first `v*` tag**, or that tag's feed job fails after the release is already public | `gh release view update-feed` |
| A6 | Tag rulesets for `v*`, `android-v*`, `update-feed` (update-keys.md step 4) | Settings, Rules |
| A7 | Git hooks on in your clone: `git config core.hooksPath .githooks` | A commit runs the guardrails |

## B. Every release, in order

| # | Step | Proof |
|---|---|---|
| B1 | Bump the version in the workspace `Cargo.toml`, `desktop/windows/voxa.iss` (default `AppVersion`) and `android/app/build.gradle.kts` (`versionName`, `versionCode` = major*10000 + minor*100 + patch; 1.0.0 is 10000) | `python tools/lint/run.py --release` passes `release-version-sync` and `release-readme-links` (fix the README links to the new version) |
| B2 | Push to `main`, wait for green Guardrails and Desktop runs | Both workflows green |
| B3 | Tag the desktop release on that commit: `git tag v1.0.0 && git push origin v1.0.0` | Desktop workflow starts on the tag |
| B4 | Approve the `release` environment twice: the `macos` job, then the `feed` job | The release has `Voxa-Setup-1.0.0.exe`, `Voxa-1.0.0-mac.dmg` and the version-free copies; the feed job ran `guard-forward`, `verify` and `fetch-check` |
| B5 | Build Android on your PC: `powershell -File android/release.ps1` (refuses a dirty tree, placeholder keys, a missing or debug release key, or an unpinned signer) | `android/build/release/Voxa-Android-1.0.0-arm64-v8a.apk` and `-universal.apk`; the arm64 APK is within the 6.0 MB budget and the universal one under the 15 MiB feed cap |
| B6 | Publish Android exactly as release.ps1 prints it: `gh release create android-v1.0.0 <both APKs> --target <commit> --latest=false --title "Voxa for Android 1.0.0"` | The Android release is never "Latest"; the desktop release is a prerelease until B10 |
| B7 | Sign the Android feed from the tag ref: `gh workflow run android-feed.yml --ref android-v1.0.0 -f tag=android-v1.0.0`, approve it | The run checks the ref, the versionCode, the pinned signer, `guard-forward`, then `fetch-check` |
| B8 | Click every README download link while logged out of GitHub | Each one downloads (no 404) |
| B9 | First release, or any release that changes the updater: section C before anyone else gets it | The gate's checklist is all ticked |
| B10 | Promote the desktop release only after the gate: `gh release edit v1.0.0 --prerelease=false --latest` (the tag workflow always publishes a prerelease, so "latest" and every `releases/latest/download/...` link keep the previous version until now) | `gh api repos/blackscythe123/voxa/releases/latest --jq .tag_name` prints the new tag; then click the README links again |

## C. REAL-HARDWARE RELEASE GATE

Nobody else gets 1.0.0 (or any build that changes the updater) until this passes on the owner's
own PC and phone:

1. Install the CI-built 1.0.0 (`Voxa-Setup-1.0.0.exe` from the release, `Voxa-Android-1.0.0-*.apk`)
   on your PC and your phone. On the phone, uninstall the test build first (test-build History is
   not kept).
2. Use both normally for a day.
3. Publish a trivial 1.0.1 (a copy or README change only, through section B: bump, tag `v1.0.1`,
   Android `android-v1.0.1`).
4. Watch both update **by themselves**: the PC while locked or idle, the phone with the
   notification and one tap. Then run checklist D below.
5. Only then give friends 1.0.1 (it is the same code, already proven to update).

### Beta feed: evaluated, not needed for this gate

A separate "beta" feed would need a client setting to choose it, and 1.0.0 has none; adding one
is a new feature and its own risk. It is also unnecessary for this gate: until friends have a
build, the only 1.0.0 installs are the owner's, so the normal feed reaches nobody else. For later
updater changes, the gate is the same: the owner's devices take a release first and friends get
the link only after it updated itself. If a beta channel is ever wanted, it needs a signed
`channel` field the client opts into, chosen in Settings, and its own tests.

## D. Manual tests on real hardware (from the 1.0.0 review)

Tick each before friends get the build. Record results in `docs/verification/`.

**Windows**

- [ ] 1. Upgrade over real 0.4.x GPUI data (History with audio, a custom hotkey, Words, ChatGPT sign-in, Start with Windows on) and over the old Electron install: History, playback, settings, Words, hotkey, sign-in and autostart survive; two copies never both hold the hotkey; SmartScreen; the "remove old" UAC task works; the silent update raises no UAC prompt. Check whether an Electron per-user install in `%LOCALAPPDATA%\Programs\voxa` is found.
- [ ] 2. Auto-update 1.0.0 to 1.0.1 through the signed feed: (a) PC locked, (b) idle 15 min, (c) tray Quit (no relaunch), (d) "Restart and update" with the window open (the window closes itself first and comes back after the update), (e) at sign-in via `--background`. Each time: no UAC prompt, no window (except d), no change of focus, no strip flash while typing in another app, the strip not blank at the next hotkey, the hotkey works.
- [ ] 3. Forced rollback: hold `Voxa.exe` or `onnxruntime.dll` open. The relaunch must not retry at once (update.json `attempts` stays 1 and `last_attempt_ms` is set; the next try waits 6 hours); the status text and download link are right after the second failure.
- [ ] 4. Tamper test: flip one byte in the staged installer. It is deleted and never run.
- [ ] 5. Locked settings.json at sign-in with auto-update off: no install. Locked or damaged words.json while adding a word: "Words not saved" shows, the file is untouched, a `words.broken-*.json` copy exists.
- [ ] 6. Secure desktop: sit on a UAC prompt or Ctrl+Alt+Del with an update staged (does the "locked" check start an install?).
- [ ] 7. Toasts (dictation, sign-in expired, update ready, birthday) never take focus from a full-screen app or game. The strip after sleep, a monitor unplug and RDP.
- [ ] 8. Main flow: repeat the paste-loss spike (regressions.md); dictate into apps with no text box ("Pasted" must not show); a long take; offline dictation sent later; a real ChatGPT session expiry; dark mode.

**Android**

- [ ] 9. A test build with data, uninstalled, then 1.0.0 installed: onboarding, sign-in, the keyboard enabled and selected, "Allow Voxa to install updates" asked once.
- [ ] 10. Self-update 1.0.0 to 1.0.1 through the feed: Wi-Fi only; the update notification (the lock screen shows only "Voxa update ready"); the "Install unknown apps" step; Install tapped with Cancel and with Update; Play Protect; API 29.
- [ ] 11. Signing: `apksigner verify --print-certs` on both APKs shows only the pinned release certificate; a release build without the release key is refused; installing 1.0.0 over a test build gives "App not installed" (expected: uninstall first).
- [ ] 12. Install timing: tap Install while a dictation is recording or paused, and while a background send runs. Start a dictation while the system confirmation dialog is open.
- [ ] 13. Keyboard: dictate and insert in several apps, pause and resume, airplane mode then background send, the sign-in expiry notification, dark mode, keyboard look presets, the licences page in the R8 release build.
- [ ] 14. Android 15 with 16 KB pages, and the universal APK on 32-bit ARM: the Rust core loads.
- [ ] 15. Low storage: "Not enough space" shows and no part file is left behind.

**CI and macOS**

- [ ] 16. CI: the first tag run (both approvals, the feed, `fetch-check` through `release-assets.githubusercontent.com`); android-feed from the tag ref (and refused from `main`); a 304 on the conditional GET.
- [ ] 17. Kill-switch drill: `gh workflow run feed-pause.yml --ref v1.0.1 -f channel=desktop -f paused=true` (and android). Both platforms drop the staged file at the next check or at the last look before installing. Unpause with `-f paused=false`.
- [ ] 18. macOS, while it stays in the README: install; Gatekeeper on macOS 15 (Privacy & Security, Open Anyway); microphone and Accessibility; paste; the "is out" bar; whether the Accessibility grant survives an update with the stable certificate.

## E. Emergency stop (a bad release is out)

1. Pause the feed: `gh workflow run feed-pause.yml --ref <newest tag> -f channel=desktop -f paused=true`
   (and `-f channel=android`). Approve it. Clients offered the bad version stop: at their next
   daily check they drop it, and a desktop or phone that already downloaded it re-reads the feed
   right before installing and drops it too.
2. If the workflow can't run, delete the feed asset: `gh release delete-asset update-feed desktop.json`
   (clients then see "Couldn't check" and install nothing).
3. Fix, then publish a higher version through section B. Its feed is unpaused. To publish an older
   version on purpose instead, run android-feed with `-f force=true` (desktop: a new patch version
   is simpler than a forced rollback).
4. An install that already crashes at start: after two early exits in a row, Voxa for Windows
   checks the feed for a fixed version before anything else starts and installs it.

## Which checks guard this runbook

`python tools/lint/run.py` (`release-runbook`, `release-readme-links`, `release-feed-guard`,
`release-android-signing`, `release-placeholder-guard`, `release-version-sync`,
`release-latest`); `--release` on every tag (CI) adds `update-key-present` and the pinned
release certificate.
