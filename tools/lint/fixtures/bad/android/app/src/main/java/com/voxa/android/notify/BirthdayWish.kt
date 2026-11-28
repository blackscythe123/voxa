package com.voxa.android.notify

// Bad: exact alarms need an extra permission and wake the phone at an exact moment.
fun bad(am: android.app.AlarmManager, at: Long, pi: android.app.PendingIntent) {
    am.setExactAndAllowWhileIdle(android.app.AlarmManager.RTC_WAKEUP, at, pi)
    am.setAlarmClock(android.app.AlarmManager.AlarmClockInfo(at, pi), pi)
}

// Good: an inexact window from midnight. A comment saying setExact( is not a call.
fun good(am: android.app.AlarmManager, at: Long, pi: android.app.PendingIntent) {
    am.setWindow(android.app.AlarmManager.RTC_WAKEUP, at, 600_000L, pi)
}
