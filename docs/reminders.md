# Reminders: wiring the "come back" engine

The decision logic lives in Rust (`core/voxa-nudge`, explained in its README). The Android app
reaches it through `ReminderEngine` in `core/voxa-ffi/src/nudge.rs`. The app only has to post
the notification, run a periodic job, report taps and swipes, and show a switch. This page is the
exact wiring for whoever owns the notification plumbing.

## What Rust already does

- Reads History from the app's data folder (the same `filesDir` path given to `VoxaCore`) to know
  when the user dictated and how much time they saved this month.
- Enforces the hard rules before the model runs: at most one reminder every 3 days, never between
  21:00 and 09:00 local time, never within 48 hours of a dictation, stop after 2 ignored or
  dismissed reminders in a row until the user dictates again, nothing after the "We'll stay quiet"
  reminder until the user dictates again, and the off switch.
- Chooses the moment and the wording, and remembers what it sent, in `nudge.json` next to
  History.
- Settles outcomes it can see by itself: a dictation within 24 hours of a reminder counts as
  opened, and no reaction within 24 hours counts as ignored.

## Android wiring

### 1. A "Reminders" channel

```kotlin
const val REMINDERS_CHANNEL = "reminders"

val channel = NotificationChannel(
    REMINDERS_CHANNEL,
    "Reminders",                       // user-visible name in system settings
    NotificationManager.IMPORTANCE_DEFAULT,
).apply { description = "An occasional nudge when you haven't dictated in a while" }
context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
```

Default importance (sound allowed, no heads-up). Android 13+ needs `POST_NOTIFICATIONS`; ask in
context (for example when the user turns the Reminders switch on), never at first launch.

### 2. Call the engine from the periodic job

Run every 2 hours: the model was trained on decisions every 2 hours from 07:00 to 23:00, so a
much shorter period would make it send slightly more readily, and a much longer one would miss
the user's best hour. WorkManager `PeriodicWorkRequest` with a 2 hour interval and a flex window
is right; it needs no network and no charging constraint.

```kotlin
class ReminderWorker(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {
    override fun doWork(): Result {
        val engine = ReminderEngine(applicationContext.filesDir.path)
        val offset = ZoneId.systemDefault().rules.getOffset(Instant.now()).totalSeconds / 60
        val reminder = engine.check(System.currentTimeMillis().toULong(), offset) ?: return Result.success()
        if (!NotificationManagerCompat.from(applicationContext).areNotificationsEnabled()) return Result.success()
        postReminder(applicationContext, reminder)   // below
        return Result.success()
    }
}
```

`check` returns `null` almost every time. A returned `Reminder` is already recorded as sent, so
post it straight away and do not call `check` again to retry.

Use one fixed notification id (for example `REMINDER_ID = 4100`) so a new reminder replaces an
old one instead of stacking.

### 3. Record outcomes from taps and swipes

```kotlin
fun postReminder(ctx: Context, r: Reminder) {
    val open = PendingIntent.getBroadcast(ctx, 1,
        Intent(ctx, ReminderReceiver::class.java).setAction("voxa.reminder.OPENED"),
        PendingIntent.FLAG_IMMUTABLE)
    val dismiss = PendingIntent.getBroadcast(ctx, 2,
        Intent(ctx, ReminderReceiver::class.java).setAction("voxa.reminder.DISMISSED"),
        PendingIntent.FLAG_IMMUTABLE)
    val n = NotificationCompat.Builder(ctx, REMINDERS_CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentTitle(r.title)
        .setContentText(r.body)
        .setStyle(NotificationCompat.BigTextStyle().bigText(r.body))
        .setContentIntent(open)
        .setDeleteIntent(dismiss)
        .setAutoCancel(true)
        .build()
    NotificationManagerCompat.from(ctx).notify(REMINDER_ID, n)
}

class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val engine = ReminderEngine(ctx.filesDir.path)
        when (intent.action) {
            "voxa.reminder.OPENED" -> {
                engine.recordOutcome(ReminderOutcome.OPENED)
                // Open Voxa (Home, or the keyboard setup screen if it is not set up yet).
                ctx.startActivity(Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }
            "voxa.reminder.DISMISSED" -> engine.recordOutcome(ReminderOutcome.DISMISSED)
        }
    }
}
```

Android 12+ blocks starting an activity from a broadcast receiver that was opened by a
notification tap (the "trampoline" rule). If that bites, make the content intent a
`PendingIntent.getActivity` straight to `MainActivity` with an extra such as
`"from_reminder" = true`, and call `recordOutcome(OPENED)` in `MainActivity` when it sees the
extra. Keep the delete intent as a broadcast.

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
})
```

The switch lives in `nudge.json`, not in `Settings`, so no change to the shared settings file is
needed. Turning it off is immediate and complete: the engine refuses before the model runs.

### Checklist for the owner of the notification plumbing

- [ ] Channel "Reminders" created at app start.
- [ ] `ReminderWorker` scheduled as unique periodic work (2 hours), kept on app update.
- [ ] Tap reports `OPENED`, swipe reports `DISMISSED`, both with the fixed notification id.
- [ ] Settings switch "Reminders", default on.
- [ ] No reminder when system notifications for Voxa are off (the worker checks
      `areNotificationsEnabled()`).
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
uneven; the rules still hold, and the model only ever sends inside the allowed window. A desktop
version would deserve its own simulation run with an hourly check grid before shipping.
