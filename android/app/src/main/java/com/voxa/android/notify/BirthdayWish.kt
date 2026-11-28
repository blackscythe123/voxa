package com.voxa.android.notify

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.voxa.android.diag.Diag
import com.voxa.android.ui.Birthday
import com.voxa.android.voxa
import uniffi.voxa_ffi.birthdayNextWish
import uniffi.voxa_ffi.birthdayWishDue
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId

/**
 * The birthday wish (docs/notifications.md): one notification at midnight on the user's birthday,
 * if they added one in Settings and left "Wish me on my birthday" on. The owner's one exception
 * to "never at night".
 *
 * An inexact window alarm from 00:00 local time, about 10 minutes wide (no exact-alarm
 * permission). Set again after it fires (for next year), when the birthday or the switch changes,
 * at app start, after a reboot, and when the clock or time zone changes. The dates come from the
 * core (`voxa_core::notices`), as local dates, so a time zone or daylight saving change can't move
 * the day; only the alarm's moment is worked out here, in the zone the phone is in now.
 */
object BirthdayWish {
    private const val WINDOW_MS = 10 * 60_000L
    private const val ACTION_WISH = "com.voxa.android.notify.BIRTHDAY"

    /** "Wish me on my birthday": on unless the user turned it off. Kept with the birthday. */
    fun enabled(ctx: Context) = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).getBoolean("birthday_wish", true)

    fun setEnabled(ctx: Context, on: Boolean) {
        ctx.getSharedPreferences("ui", Context.MODE_PRIVATE).edit().putBoolean("birthday_wish", on).apply()
        schedule(ctx)
    }

    /** The year the wish was last shown (0: never). */
    private fun wishedYear(ctx: Context) = ctx.getSharedPreferences("notify", Context.MODE_PRIVATE).getInt("birthday_wished", 0)

    private fun intent(ctx: Context, flags: Int): PendingIntent? = PendingIntent.getBroadcast(
        ctx, 30, Intent(ctx, BirthdayReceiver::class.java).setAction(ACTION_WISH), PendingIntent.FLAG_IMMUTABLE or flags,
    )

    /** When the next wish is due, in epoch milliseconds, or null when there is nothing to wish. */
    fun nextAt(ctx: Context, today: LocalDate = LocalDate.now(), zone: ZoneId = ZoneId.systemDefault()): Long? {
        if (!enabled(ctx)) return null
        val (m, d) = Birthday.get(ctx) ?: return null
        val date = birthdayNextWish(today.toString(), m.toUByte(), d.toUByte(), wishedYear(ctx)) ?: return null
        // atStartOfDay: the first moment of that day, even where a DST change skips midnight.
        return LocalDate.parse(date).atStartOfDay(zone).toInstant().toEpochMilli()
    }

    /** Set (or cancel) the alarm for the next wish. Cheap; safe to call often. */
    fun schedule(ctx: Context) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        val at = nextAt(ctx)
        if (at == null) {
            intent(ctx, PendingIntent.FLAG_NO_CREATE)?.let {
                am.cancel(it)
                it.cancel()
                Diag.log("birthday", "no wish scheduled")
            }
            return
        }
        am.setWindow(AlarmManager.RTC_WAKEUP, at, WINDOW_MS, intent(ctx, PendingIntent.FLAG_UPDATE_CURRENT)!!)
        Diag.log("birthday", "wish scheduled in ${(at - System.currentTimeMillis()) / 60_000} min")
    }

    /** Whether an alarm for the wish is set (for tests and diagnostics). */
    fun scheduled(ctx: Context) = intent(ctx, PendingIntent.FLAG_NO_CREATE) != null

    /** The alarm went off: wish if it is still the day and not yet wished this year, then set next year's. */
    internal fun fire(ctx: Context, now: LocalDateTime = LocalDateTime.now()) {
        val bday = Birthday.get(ctx)
        val due = bday != null && enabled(ctx) &&
            birthdayWishDue(now.toLocalDate().toString(), (now.hour * 60 + now.minute).toUShort(), bday.first.toUByte(), bday.second.toUByte(), wishedYear(ctx))
        Diag.log("birthday", "alarm: ${if (due) "wishing" else "not due"}")
        if (due) {
            val name = runCatching { ctx.voxa.core.cachedAccount()?.name }.getOrNull()
            if (Notifier.birthday(ctx, name)) {
                ctx.getSharedPreferences("notify", Context.MODE_PRIVATE).edit().putInt("birthday_wished", now.year).apply()
            }
        }
        schedule(ctx)
    }

    internal const val ACTION = ACTION_WISH
}

/**
 * The wish's alarm, and the moments it must be set again: after a reboot (alarms don't survive
 * one), after an app update, and when the clock or the time zone changes. Not exported: these
 * system broadcasts and Voxa's own alarm still reach it.
 */
class BirthdayReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val done = goAsync()
        // The account name is read from the data folder: off the main thread.
        Thread {
            try {
                if (intent.action == BirthdayWish.ACTION) BirthdayWish.fire(context) else BirthdayWish.schedule(context)
            } catch (e: Exception) {
                Diag.log("birthday", "failed: ${e.javaClass.simpleName}")
            } finally {
                done.finish()
            }
        }.start()
    }
}
