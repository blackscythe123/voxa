# Birthday wish: emulator verification (2026-11-28)

API 35 emulator (emulator-5554, x86_64 debug build). Never the physical phone.

| Leg | Evidence | Status |
|---|---|---|
| Settings: birthday picked with the wheels, "Wish me on my birthday" row shows | `01-settings-birthday-and-switch.png` | Rendered |
| Alarm set at the coming local midnight, 10 minute window, inexact | `02-alarm-before-midnight.txt` (`RTC_WAKEUP origWhen=2026-11-29 00:00:00 window=+10m`) | Verified |
| Crossing midnight (emulator clock set to 23:58:20 with `adb root` and `date`) | Diagnostics: `birthday: alarm: wishing`, `notify: BIRTHDAY: POST` at 00:00:38 (first run) and 00:03:40 (second run); alarm then moved to next year | Log |
| The notification, channel Birthday, title with the first name | `03-birthday-notification-at-midnight.png`; `04-notification-record.txt` (id 3, channel birthday, vis PRIVATE, public version "Happy birthday") | Rendered |
| Tapping opens Home with the birthday doodle | `05-tap-opens-home-with-birthday-doodle.png` (the notification's own `open=home` intent sent with adb, after a real tap landed on setup because the app was not onboarded) | Partly verified |
| Switch off or no birthday: nothing scheduled; DST and 29 February dates | `BirthdayWishTest` (5 instrumented tests, pass) | Tested |

Notes: the first run's notification was cleared when another agent's instrumented test run
reinstalled the app at 00:03, so the crossing was repeated with the birthday on 30 November.
The name is the debug seam `--es debug_name Alexandria` (no account on the emulator). The clock
was restored afterwards (`auto_time 1`). After a reboot the alarm is set again by
`BirthdayReceiver` (BOOT_COMPLETED): code only, not driven.
