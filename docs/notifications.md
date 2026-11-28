# Notifications

What Voxa tells you outside the app, on Android and the desktop, and how to test it. Built from
the approved notifications spec (EGM design review, November 2026).

Voxa says almost everything in context: the keyboard bar, the desktop strip, Home and the tray
dot. A system notification is only for the few moments when you are not looking and something
needs you: your ChatGPT sign-in ended, or a dictation finished in the background. And one that is
only kind: a wish at midnight on your birthday, if you added it.

## Copy rules

Sentence case, no emojis, no dashes. Never transcript text, cookies or the account email. A
notification may name the app a dictation was for. All wording lives in one place,
`core/voxa-core/src/notices.rs`, used by both the desktop and Android (through
`core/voxa-ffi/src/notices.rs`), with unit tests for every string.

## What exists

| | Kind | Trigger | Title | Body | Button |
|---|---|---|---|---|---|
| A1, D1 | Signed out (must) | A session check says the session ended, it was known good before, and a second check about 2 minutes later agrees | You're signed out of ChatGPT | Voxa can't write anything until you sign in. (+ "N dictations are waiting and will send when you do.") | Sign in |
| A2, D2 | Ready to copy (should) | A dictation that wasn't on screen was written out (background send, or sent when the network came back) | Your dictation for {App} is ready / N dictations are ready | It couldn't paste, so it's waiting in History. / They're waiting in History. | Copy (one only) |
| A3, D3 | Couldn't send (should) | A background send gave up (unreachable after retries, or the dictation waited over 24 h) | Voxa couldn't send N dictation(s) | Your recordings are kept. Your sign-in is fine. | Retry (Android), Send now (desktop) |
| A4, D4 | Birthday wish (owner decision, 2026-11-28) | 00:00 local time on the birthday from Settings, "Wish me on my birthday" on, not yet wished this year | Happy birthday, {first name} (or Happy birthday) | Have a lovely day. | None; tapping opens Home (birthday doodle) |
| A5 | Update ready (should; the update spec's A4) | A verified update has finished downloading on Android | Voxa {version} is ready to install | Tap to install. Your history and settings stay as they are. | Install |

Never a notification (already shown in context): waiting to send, mic permission, keyboard
disabled or not selected, storage, recaps or streaks, Cloudflare blocking.

Updates (amended on purpose by the owner, 2026-11-28, with the auto update spec): Android gets
A5 only (called A4 in the update spec; the birthday wish took A4 first), once a verified update
is downloaded. The Windows "Voxa was updated to X" note and the
macOS "Voxa X is out" notice are in context only: a dismissible Home bar, the Settings status
line and the tray tooltip. There is no desktop update toast (no D4).

### "Signed out" is only said when it is certain

A false "signed out" is worse than a late one (a Cloudflare 403 once caused a sign-in loop). So:

- Only a definite answer counts: ChatGPT's session call returned no token for a session Voxa had
  (`voxa_chatgpt::ChatGpt::account` gives `SessionExpired`). Not "no session saved", not
  Cloudflare (`blocked`), not offline, and never a failed transcription (a 401/403 during upload
  only changes the keyboard bar and the strip).
- The session must have been known good at the last definite check (Android keeps this in
  SharedPreferences `notify/known`, the desktop in memory: `session_known`).
- A second check about 2 minutes later must agree. Then the notification is posted once per
  expiry episode and updated in place. It is removed when a check says the session is good again
  or when you sign out on purpose (a deliberate sign-out never notifies).

### Anti-spam (voxa_core::notices::decide)

1. One notification per kind, updated in place: Android IDs 1 (sign-in) and 2 (dictations),
   desktop tags `signin` and `dictations`.
2. Sign-in at most once per expiry episode.
3. Dictations alert at most once every 6 hours; in between the count is only updated silently.
4. No more than 2 new Voxa notifications in 24 hours across kinds. A5 (update ready) counts toward the cap;
   because it is never urgent, a capped A5 is deferred to the next allowed moment (the next
   update job or app start), never dropped (`voxa_core::notices::decide_update`).
5. Never posted while you can see Voxa: the Android keyboard showing or an app screen in front;
   the desktop strip visible or Voxa's window in front.
6. Opening History or the dictation, copying it, deleting it or resending removes the dictations
   notification.
7. Sign-in at night (21:00 to 08:00) arrives silently on Android. A5 (update ready) is always
   silent (its channel is low importance).
8. **The one exception to "never at night": the birthday wish.** The owner decided (2026-11-28)
   that it arrives at midnight on purpose. It is outside rules 2 to 4 and 7 (its own ID or tag,
   not counted against or held back by the daily cap, not silent), at most once a year (the year
   is kept: Android `notify/birthday_wished`, desktop `notify.json` `birthday_wished`), and still
   held back while you look at Voxa (rule 5). Do Not Disturb and Focus apply as usual. Guarded
   by `voxa_core::notices` tests (`the_birthday_wish_is_the_only_notice_allowed_at_midnight` and
   the next-midnight tests) and the `android-exact-alarm` lint.

9. A5 is posted at most once per version. If you swipe it away it never comes back for that
   version; the Home card and the Settings row still show the update.

### The birthday wish

- Settings: "Your birthday" (day and month, never the year, kept on the device; Android private
  prefs `ui`, desktop `settings.json` `birthday_month`, `birthday_day`) and, once added,
  "Wish me on my birthday" (`birthday_wish`, on by default). Off, or no birthday: nothing is
  scheduled. 29 February is wished on 28 February in other years.
- The dates come from the core as local calendar dates (`next_birthday_wish`,
  `birthday_wish_due`), so a time zone or daylight saving change can't move the day; the shells
  turn the date into 00:00 in the current zone only when scheduling.
- Android (`notify/BirthdayWish.kt`, channel "Birthday", default importance, ID 3): an inexact
  `AlarmManager.setWindow` alarm from 00:00, 10 minutes wide, no exact-alarm permission. Set
  again after it fires (next year), when the birthday or switch changes, at app start, and by
  `BirthdayReceiver` after a reboot, an app update, a clock change or a time zone change. Doze
  may hold the alarm back; a wish delivered any time on the day still shows, never on another
  day. Lock screen shows "Happy birthday" without the name. Needs `POST_NOTIFICATIONS`.
- Desktop (`notify.rs`, tag `birthday`): the tray checks about every 10 s and shows the toast
  only in the first 2 minutes after midnight. A PC that was asleep or off at midnight gets no
  late wish. Clicking opens Home.

## Android

Files: `android/app/src/main/java/com/voxa/android/notify/` (`Notifier.kt`, `VoxaJobs.kt`,
`NotifyReceiver.kt`), `ui/NotifyCards.kt`, hooks in `VoxaApp.kt`, `MainActivity.kt`,
`ime/VoxaKeyboard.kt`, `ui/HomeScreen.kt`, `ui/SettingsScreen.kt`.

- Channels: "Sign-in" ("When you need to sign in to ChatGPT again", default importance) and
  "Dictations" ("Dictations sent in the background, or that couldn't be sent", low importance).
  `Notifier.CHANNELS` is a list so a third channel, "Reminders", can be added later.
- "Updates" channel ("When a new version of Voxa is ready to install", low importance) for A5,
  notification ID 4 (IDs 1 to 3 are taken).
- Lock screen: `VISIBILITY_PRIVATE` with a public version that only says "Voxa needs you". A5 is
  the exception: its content is not private, so it is `VISIBILITY_PUBLIC` with the public title
  "Voxa update ready".
- Taps: A1 opens sign-in (`EXTRA_OPEN=sign-in`); A2 opens the dictation (`entry:<id>`) or
  History; A3 opens History. "Copy" and "Retry" go to `NotifyReceiver` (not exported), which
  reads the text from History by entry id, so the text is never in the notification.
- Permission (`POST_NOTIFICATIONS`, Android 13+). The keyboard can't ask, and the app never asks
  for notifications at first launch or during onboarding. The one onboarding exception is not a
  notification permission: the optional, skippable "Keep Voxa up to date" step asks for "install
  unknown apps" (`REQUEST_INSTALL_PACKAGES`), owner decision 2026-11-28. At most two asks:
  1. Right after signing back in when the session had ended (sign-in opened from the keyboard
     or from A1): a sheet "Want a notification next time you're signed out?" with Allow and
     Not now. Allow brings up Android's prompt.
  2. Otherwise a dismissible Home card "Know when you're signed out" with Allow, once a
     dictation has been pasted.
  Settings has a "Notifications" row that opens Android's notification settings for Voxa.
  Below Android 13 the channels are created and notifications simply post.
- Background work on the framework JobScheduler (`VoxaJobs`, no WorkManager):
  - Session check every 12 h (any network, battery not low, persisted; needs
    `RECEIVE_BOOT_COMPLETED`). A definite expiry is confirmed 2 minutes later in the same job.
    Checks the app makes itself (start, resume, network back) schedule a one-off confirmation
    job instead. `VoxaApp.checkSessionNow()` is the awaitable check.
  - Background send: whenever History has entries "Sends when you're back online" (checked at
    start and after every History change), a one-off job with a network constraint and
    exponential backoff (30 s, doubling) sends them through `VoxaCore.send_waiting`. Entries a
    live keyboard flow still owns are skipped (`VoxaFlow.is_live`, the desktop's `is_live`
    guard). Offline keeps them waiting; unreachable keeps them waiting until the 6th try or 24 h,
    then they become "Couldn't reach ChatGPT" and A3 is posted. Sent ones become ready in
    History and A2 is posted. An entry deleted meanwhile stays deleted.
- The keyboard posts A2 or A3 for a dictation that finished while another (or none) was on screen,
  using `VoxaFlow.entry_for_job`, which returns nothing for incognito takes.

## Desktop

Files: `desktop/app/src/notify.rs` (state and rules), `notify_win.rs`, `notify_mac.rs`, hooks
in `main.rs` (`watch_expiry`, `notify_background`, `on_notification`).

- D1 uses the existing 10 minute session check (`RECHECK`). When it finds a definite expiry after
  a good check, it checks again 2 minutes later and only then shows the toast. "Sign in" runs the
  existing `sign_in`. The tray dot still turns amber as before.
- D2 is shown when a dictation that wasn't on the strip is written out (for example several sent
  after a reconnect); clicking opens History, "Copy" copies it from History. D3 is shown when such
  a send fails (not for sign-in, not "no speech"); "Send now" retries it and calls `resend_all`.
- Windows: toasts through the `windows` crate (features `UI_Notifications`, `Data_Xml_Dom`,
  `Foundation`; no new crate). The app's ID is `Voxa.Desktop`: the installer gives the Start
  menu shortcut that AppUserModelID (`desktop/windows/voxa.iss`), the tray process registers
  the same ID with its name and icon under `HKCU\Software\Classes\AppUserModelId` (removed on
  uninstall) and claims it with `SetCurrentProcessExplicitAppUserModelID`. Clicks arrive through
  the toast's Activated event (the tray process is always running). Every toast has a Tag and the
  Group `voxa`, so a newer one replaces the older and Voxa can remove it. Focus and Do Not Disturb
  are respected (no priority toasts).
- macOS: `UNUserNotificationCenter` through the existing `objc` dependency (plus `block`, already
  in the build through `cocoa`). Needs the app bundle (`desktop/macos/Info.plist`); a bare binary
  shows none. Sign-in is `active`, dictations `passive`, never time-sensitive. Permission is
  asked at the same moments as Android (after signing back in following an expiry, or after the
  first pasted dictation), never at launch.

## How to test

### Desktop (Windows)

A test copy runs beside the installed Voxa with its own data folder. With `VOXA_DATA` set it
uses its own notification ID, `Voxa.Desktop.Test` ("Voxa (test)"), so the real one is untouched.

```powershell
$t = "$env:TEMP\voxa-notify-test"; New-Item -ItemType Directory -Force "$t\data"
'{"cookie_header":"x=1","user_agent":"test"}' | Set-Content "$t\data\session.json"
'{"onboarded":true,"imported_old_voxa":true}' | Set-Content "$t\data\settings.json"
$env:VOXA_DATA = "$t\data"
$env:VOXA_TEST = "1"                              # test hooks: also needed in a release build
$env:VOXA_TEST_SESSION = "valid,expired,expired"   # answers of the next session checks
$env:VOXA_TEST_RECHECK_SECS = "15"                 # instead of 10 minutes
$env:VOXA_TEST_CONFIRM_SECS = "3"                  # instead of 2 minutes
Start-Process target\release\voxa-desktop.exe -ArgumentList "--background"
```

The test switches only work with `VOXA_DATA` set. Expected log (`$t\data\voxa-desktop.log`):
`session ended: checking again in 3 s`, `session check (confirming): Expired, definite true`,
`sign-in notification: Post (0 waiting)`, `toast shown: signin`. Add `,valid` to the sequence to
see `sign-in notification removed`. Without `VOXA_TEST_SESSION` the invalid session gets a real
"expired" answer but no toast, because it was never known good. To clean up afterwards:
`[Windows.UI.Notifications.ToastNotificationManager]::History.Clear('Voxa.Desktop.Test')` in
Windows PowerShell and delete `HKCU\Software\Classes\AppUserModelId\Voxa.Desktop.Test`.

### Android (debug build, emulator)

Debug seams (debug builds only), sent to `MainActivity` with
`adb shell "am start -n com.voxa.android/.MainActivity ..."` (quote the whole command so text
with spaces survives):

- `--es debug_check valid|expired|''`: session checks answer this instead of asking ChatGPT.
- `--el debug_confirm_ms 5000`: confirm an expiry after 5 s instead of 2 minutes.
- `--es debug_transcript 'text'`: the keyboard and the background send use this text instead of
  calling ChatGPT.
- `--ez debug_send_unreachable true`: every background send ends "unreachable" (to see A3). It
  does not change History, so the entry keeps waiting and is tried again; turn it off after.
- `--ez debug_fake_session true --ez debug_signin_done true`: as if the sign-in screen had just
  captured a session (to see the ask after an expiry).
- `--ez debug_ready false`: needed for the checks above to run (with `true` the app skips checks).

Steps (job IDs: 7101 session check, 7102 confirmation, 7103 background send):

1. Channels: `adb shell dumpsys notification --noredact | grep "NotificationChannel{mId='sign-in'\|mId='dictations'"`
   shows `sign-in` (importance 3) and `dictations` (importance 2); `cmd jobscheduler
   get-job-state com.voxa.android 7101` says `waiting`.
2. Home card: on Home with a pasted dictation and notifications not allowed, the card shows.
   Allow brings up Android's prompt.
3. A1: `--es debug_check valid --el debug_confirm_ms 5000 --ez debug_ready false` (the check says
   valid, `notify.xml` has `known=valid`), then `--es debug_check expired` (no check runs: the
   resume check is throttled), press Home, then `cmd jobscheduler run -f com.voxa.android 7101`.
   The log shows `session ended after being good: confirming`, `confirming check: EXPIRED,
   definite true`, `SIGN_IN: POST`, and the notification appears. Its "Sign in" opens sign-in.
4. Suppression: the same as 3, but with the Voxa keyboard open in a text field when the job runs:
   `SIGN_IN not posted: Voxa is on screen`.
5. Ask after sign-in: revoke the permission (`pm revoke com.voxa.android
   android.permission.POST_NOTIFICATIONS`), open the app with the session expired, send
   `--es open sign-in`, then `--ez debug_fake_session true --ez debug_signin_done true`: the sheet
   "Want a notification next time you're signed out?" shows (only once onboarded).
6. Background send: `svc wifi disable` and `svc data disable`, select the Voxa keyboard, dictate
   into a text field: History says "Sends when you're back online" and the log `background send
   scheduled`. Press Home, select another keyboard (the Voxa keyboard and its flow go away),
   `am kill com.voxa.android` (not `force-stop`, which cancels jobs), then `svc wifi enable`.
   Android starts the app for job 7103: `send: <id> -> Sent`, `DICTATIONS: POST`, the entry is
   "Ready" in `history.json`, and A2 shows with "Copy".
7. A3 in the same slot: with A2 showing, record another offline dictation with
   `--ez debug_send_unreachable true`, turn the network on, and run job 7103 until the 6th
   attempt: `DICTATIONS: UPDATE`, and ID 2 now reads "Voxa couldn't send 1 dictation" (still one
   Voxa notification). "Retry" schedules the send again and removes it.

8. Birthday wish: set the birthday to tomorrow in Settings, check `dumpsys alarm` for
   `*walarm*:com.voxa.android.notify.BIRTHDAY` at `origWhen=<tomorrow> 00:00:00 window=+10m`, press
   Home, then (with `adb root`) `settings put global auto_time 0` and `date MMDD2358YYYY.20`. Within
   the window the log shows `birthday: alarm: wishing`, `notify: BIRTHDAY: POST`, and the alarm
   moves to next year. `--es debug_name Alexandria` gives the title a first name when no account
   is known. Restore with `settings put global auto_time 1`.

Diagnostics (`files/diag`, Settings, Share diagnostics) log every decision under `notify`,
`jobs` and `birthday`.

Desktop birthday wish: a test copy as above with `"birthday_month":11,"birthday_day":29` in
`settings.json` and `VOXA_TEST_NOW=2026-11-29T00:00` (00:01 also shows; 00:05 must not): the log
says `toast shown: birthday` and `birthday wish: shown`, and `notify.json` gets
`"birthday_wished":2026`.

## Verification (2026-11-28)

Desktop on this Windows 11 PC (test copy with `VOXA_DATA`); Android on the API 35 emulator
(x86_64 debug build). Screenshots: `docs/verification/notifications/` (desktop) and
`android/docs/verification/notifications/` (Android).

| Leg | Evidence | Status |
|---|---|---|
| Core copy and anti-spam rules | `cargo test -p voxa-core notices` (8 tests) | Unit tested |
| D1 toast after valid, expired, expired | Log `toast shown: signin`; Windows notification history holds tag `signin`, group `voxa` with the approved text and a "Sign in" button; `desktop-d1-signin-toast.png` | Rendered (Windows) |
| D1 removed when the session is good again | Log `sign-in notification removed` after a `valid` answer | Log only |
| D1 not shown without a known good session | Real check of an invalid session: `Expired`, no toast | Log only |
| D1 "Sign in" click, D2, D3 | Not driven (clicking on the owner's desktop was not allowed; no background transcription without a real account) | Not verified |
| macOS (all) | Written against UserNotifications; not compiled or run (no Mac here) | Not verified |
| Android channels, 12 h job | `dumpsys notification`, `get-job-state 7101: waiting` | Verified |
| Home card, then Android's prompt, Allow | `01-home-card.png`, `02-system-prompt.png`, permission `granted=true` | Rendered |
| A1 after valid, expired, expired (job) | Log `SIGN_IN: POST`; `04-a1-signed-out.png`; dumpsys id 1, channel sign-in, `vis=PRIVATE` with a public version | Rendered |
| A1 "Sign in" opens sign-in | `05-a1-tap-opens-sign-in.png` | Rendered |
| Ask after sign-in following an expiry | `06-ask-after-sign-in.png`; Allow led to Android's prompt and `granted=true`, `asks=2` | Rendered (sign-in completion simulated by a debug seam) |
| Settings, Notifications row | `07-settings-row.png` | Rendered |
| Not posted while the keyboard shows | Log `SIGN_IN not posted: Voxa is on screen` | Log only |
| Background send after the process died | Process killed with an offline entry; network on; Android started the app for job 7103; `Sent`; entry `Ready` in history.json; `11-a2-ready-after-background-send.png` | Rendered |
| A2 "Copy" | Log `copy: dictation copied`; Android's clipboard preview shows the entry's text; notification removed (`15-after-copy.png`) | Rendered |
| A3 replaces A2 in ID 2 | Log `DICTATIONS: UPDATE`; one Voxa notification, id 2, "Voxa couldn't send 1 dictation" (`13-a3-replaces-a2-in-id-2.png`) | Rendered (unreachable simulated by a debug seam) |
| A3 "Retry" | `14-a3-expanded-retry.png`; log `retry: sending again`, job ran, notification removed | Rendered button, log for the rest |
| Not posted while the app is in front | Log `DICTATIONS not posted: Voxa is on screen` | Log only |
| A2 tap opens the dictation | `16-open-entry.png` (same extra the notification sends, started with adb) | Partly verified |
| Keyboard path (A2/A3 for a dictation sent when the network came back while the keyboard lived) | Code only | Not verified |
| Real ChatGPT session expiring | Needs a real account; all A1/D1 runs used test answers | Not verified |
| Birthday wish rules (next midnight, 29 February, once a year, asleep at midnight, switch off) | `cargo test -p voxa-core notices`, `-p voxa-ffi --test birthday`, `-p voxa-desktop` (`notify::tests`, `store::load_tests`) | Unit tested |
| A4 birthday wish on Android, alarm crossing midnight | `android/docs/verification/birthday/` (alarm window, POST log, notification screenshot, Home with the cake doodle) | Rendered (emulator clock moved) |
| A4 after a reboot | Code only (`BirthdayReceiver`, BOOT_COMPLETED) | Not verified |
| D4 birthday toast on Windows | Test copy with `VOXA_TEST_NOW` 00:00 and 00:01: `toast shown: birthday`; notification history held "Happy birthday" / "Happy birthday, Zz" with launch `home`; 00:05: nothing | Log and toast history |
| D4 click opens Home; macOS | Not driven (no clicking on the owner's desktop; no Mac) | Not verified |
