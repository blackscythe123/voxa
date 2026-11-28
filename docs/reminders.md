# Reminders: wiring the "come back" engine

The decision logic lives in Rust (`core/voxa-nudge`, explained in its README). The Android app
reaches it through `ReminderEngine` in `core/voxa-ffi/src/nudge.rs`. The app only has to post
the notification, run one one-shot job, report taps and swipes, and show a switch. This page is the
exact wiring for whoever owns the notification plumbing.

## What Rust already does

- Reads History from the app's data folder (the same `filesDir` path given to `VoxaCore`) to know
  when the user dictated and how much time they saved this month.
- Enforces the hard rules first: at most one reminder every 3 days, never between
  21:00 and 09:00 local time, never within 48 hours of a dictation, stop after 2 ignored or
  dismissed reminders in a row until the user dictates again, nothing after the "We'll stay quiet"
  reminder until the user dictates again, and the off switch.
- Chooses the moment and the wording with a simple rule: near the hour the user usually
  dictates (19:00 with no history yet), in the tip wording. The evolved network in the crate is
  an experiment the app does not use. It remembers what it sent in `nudge.json` next to History,
  and a damaged `nudge.json` keeps reminders off.
- Says when to check next (`nextCheckMs`): the next usual hour inside 09:00 to 21:00.
- Settles outcomes it can see by itself: a dictation within 24 hours of a reminder counts as
  opened, and no reaction within 24 hours counts as ignored.

## Android wiring

Reuse what the notifications work already built (docs/notifications.md): the framework
JobScheduler in `notify/VoxaJobs.kt`, the channel list in `notify/Notifier.kt`, and the
`notify/NotifyReceiver.kt` broadcast receiver. No WorkManager: it would add a few hundred KB to the
APK for one job.

### 1. A "Reminders" channel

Add it to `Notifier.CHANNELS`, so `Notifier.createChannels` creates it at app start with the
others:

```kotlin
val REMINDERS = Channel("reminders", "Reminders", "An occasional nudge when you haven't dictated in a while", NotificationManager.IMPORTANCE_DEFAULT)
private val CHANNELS = listOf(SIGN_IN, DICTATIONS, REMINDERS)
```

Default importance (sound allowed, no heads up). Android 13+ needs `POST_NOTIFICATIONS`; the app
already asks for it in context (`Notifier.shouldAsk`), never at first launch.

### 2. One one-shot job, not a periodic one

Add a `REMINDER` id (7104) to `VoxaJobs`. The default policy only needs one check near the
user's usual hour, so schedule one one-shot job for that moment and reschedule it each time it
runs. That is at most one wake-up a day, instead of twelve for a 2 hour periodic job.

```kotlin
// VoxaJobs companion
const val REMINDER = 7104

fun scheduleReminder(ctx: Context) {
    val offset = ZoneId.systemDefault().rules.getOffset(Instant.now()).totalSeconds / 60
    val now = System.currentTimeMillis()
    val at = ReminderEngine(ctx.filesDir.path).nextCheckMs(now.toULong(), offset).toLong()
    scheduler(ctx).schedule(
        JobInfo.Builder(REMINDER, component(ctx))
            .setMinimumLatency(at - now)
            .setOverrideDeadline(at - now + 30 * 60_000L)
            .setPersisted(true)
            .build(),
    )
}
```

Call `scheduleReminder` at app start (next to `schedulePeriodic`) and at the end of every
REMINDER run. Turning the Reminders switch on schedules it too.

In `onStartJob`:

```kotlin
REMINDER -> scope.launch {
    runCatching { remind(app) }.onFailure { Diag.log("jobs", "reminder failed: ${it.javaClass.simpleName}") }
    scheduleReminder(app)
    jobFinished(params, false)
}
```

```kotlin
private suspend fun remind(app: VoxaApp) = withContext(Dispatchers.IO) {
    // Permission first: `check` records a returned reminder as sent, so it must only be asked
    // when the reminder can really be shown.
    if (!Notifier.canPost(app, Notifier.REMINDERS)) return@withContext
    val offset = ZoneId.systemDefault().rules.getOffset(Instant.now()).totalSeconds / 60
    val reminder = ReminderEngine(app.filesDir.path).check(System.currentTimeMillis().toULong(), offset) ?: return@withContext
    Notifier.reminder(app, reminder)
}
```

`Notifier.canPost` is a small helper to add: notifications allowed for Voxa
(`areNotificationsEnabled`), `POST_NOTIFICATIONS` granted on Android 13+, and the Reminders
channel not turned off (`getNotificationChannel(id).importance != IMPORTANCE_NONE`).

`check` returns `null` almost every time. A returned `Reminder` is already recorded as sent, so
post it straight away and do not call `check` again to retry.

### 3. Post it and record taps and swipes

In `Notifier`, with one fixed id (`ID_REMINDER = 3`) so a new reminder replaces an old one:

```kotlin
fun reminder(ctx: Context, r: Reminder) {
    val open = PendingIntent.getActivity(
        ctx, ID_REMINDER,
        Intent(ctx, MainActivity::class.java).putExtra(MainActivity.EXTRA_FROM_REMINDER, true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
    val n = builder(ctx, REMINDERS, r.title, r.body, silent = false)
        .setContentIntent(open)
        .setDeleteIntent(NotifyReceiver.reminderDismissed(ctx))
        .build()
    NotificationManagerCompat.from(ctx).safeNotify(ctx, ID_REMINDER, n)
}
```

`builder` already sets the small icon `R.drawable.ic_notify`, the colour, the private lock screen
version and auto cancel.

The tap opens `MainActivity` directly (Android 12+ blocks starting an activity from a receiver
that a notification tap opened). `MainActivity` sees `EXTRA_FROM_REMINDER` and calls
`ReminderEngine(filesDir.path).recordOutcome(ReminderOutcome.OPENED)` off the main thread.

The swipe goes to `NotifyReceiver` with a new action:

```kotlin
const val ACTION_REMINDER_DISMISSED = "com.voxa.android.notify.REMINDER_DISMISSED"

ACTION_REMINDER_DISMISSED -> {
    val done = goAsync()
    Thread {
        ReminderEngine(context.filesDir.path).recordOutcome(ReminderOutcome.DISMISSED)
        done.finish()
    }.start()
}
```

Ignored reminders need no code: after 24 hours without a tap the engine counts it as ignored on
its next check.

### 4. A Settings switch "Reminders"

A row in Settings: label "Reminders", supporting text "An occasional nudge when you haven't
dictated in a while", default on.

```kotlin
var on by remember { mutableStateOf(ReminderEngine(filesDir).enabled()) }
Switch(checked = on, onCheckedChange = {
    on = it
    ReminderEngine(filesDir).setEnabled(it)   // takes effect at the next check
    if (it) VoxaJobs.scheduleReminder(context)
})
```

The switch lives in `nudge.json`, not in `Settings`, so no change to the shared settings file is
needed. Turning it off is immediate and complete: the engine refuses before anything else runs.
A `nudge.json` that can't be read keeps reminders off.

### Checklist for the owner of the notification plumbing

- [ ] `Notifier.REMINDERS` added to `Notifier.CHANNELS`.
- [ ] `VoxaJobs.REMINDER` one-shot job, scheduled at start and after each run, persisted.
- [ ] Permission and channel checked before `check` is called, so no reminder is recorded as sent
      when it can't be shown.
- [ ] Tap reports `OPENED` from `MainActivity`, swipe reports `DISMISSED` through
      `NotifyReceiver`, one fixed notification id.
- [ ] Settings switch "Reminders", default on.
- [ ] Copy is shown as returned: it is plain sentence case with no emojis and no dashes already.

## Desktop (later)

The same crate plugs into the desktop without changes. The desktop has History in its own store
(`desktop/app/src/store.rs`), so:

1. Add `voxa-nudge` as a dependency of `desktop/app`.
2. Once an hour while Voxa is running in the tray, build `voxa_nudge::Usage` from the History
   entry ids (start times) of written dictations and this month's saved seconds, then call
   `Engine::open(data_dir).decide(now_ms, utc_offset_minutes, &usage, &mut rng)`.
3. Show the returned title and body as a Windows toast (or macOS notification); report clicks with
   `record_outcome(Outcome::Opened)` and toast dismissals with `Outcome::Dismissed`.
4. Add a "Reminders" switch in Settings bound to `Engine::set_enabled`.

The desktop only runs while the user is signed in and Voxa is in the tray, so checks will be
uneven; the rules still hold, and nothing is ever sent outside the allowed window. A desktop
version would deserve its own simulation run with an hourly check grid before shipping.
