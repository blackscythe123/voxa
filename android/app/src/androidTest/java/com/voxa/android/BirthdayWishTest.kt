package com.voxa.android

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.notify.BirthdayWish
import com.voxa.android.ui.Birthday
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.time.LocalDate
import java.time.ZoneId

/** The birthday wish's alarm on a real Android runtime (docs/notifications.md). */
@RunWith(AndroidJUnit4::class)
class BirthdayWishTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext
    private val ui = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE)
    private val notify = ctx.getSharedPreferences("notify", Context.MODE_PRIVATE)
    private var saved: Triple<Int, Int, Boolean>? = null
    private var wished = 0

    @Before
    fun keep() {
        saved = Triple(ui.getInt("birthday_month", 0), ui.getInt("birthday_day", 0), ui.getBoolean("birthday_wish", true))
        wished = notify.getInt("birthday_wished", 0)
        notify.edit().remove("birthday_wished").commit()
    }

    @After
    fun restore() {
        val (m, d, on) = saved!!
        ui.edit().putInt("birthday_month", m).putInt("birthday_day", d).putBoolean("birthday_wish", on).commit()
        notify.edit().putInt("birthday_wished", wished).commit()
        BirthdayWish.schedule(ctx)
    }

    @Test
    fun a_birthday_schedules_the_wish_at_local_midnight() {
        Birthday.set(ctx, 3, 14)
        BirthdayWish.setEnabled(ctx, true)
        assertTrue(BirthdayWish.scheduled(ctx))
        val zone = ZoneId.of("Asia/Kolkata")
        val at = BirthdayWish.nextAt(ctx, LocalDate.of(2026, 11, 28), zone)
        assertEquals(LocalDate.of(2027, 3, 14).atStartOfDay(zone).toInstant().toEpochMilli(), at)
    }

    @Test
    fun a_29_february_birthday_alarm_is_on_28_february_in_other_years() {
        Birthday.set(ctx, 2, 29)
        val zone = ZoneId.of("UTC")
        assertEquals(LocalDate.of(2027, 2, 28).atStartOfDay(zone).toInstant().toEpochMilli(), BirthdayWish.nextAt(ctx, LocalDate.of(2026, 11, 28), zone))
    }

    @Test
    fun a_midnight_skipped_by_daylight_saving_still_gets_the_wish_that_day() {
        // Santiago skips 00:00 to 01:00 on 2026-09-06: the wish is at the day's first moment.
        Birthday.set(ctx, 9, 6)
        val zone = ZoneId.of("America/Santiago")
        val at = BirthdayWish.nextAt(ctx, LocalDate.of(2026, 9, 1), zone)!!
        assertEquals(LocalDate.of(2026, 9, 6), java.time.Instant.ofEpochMilli(at).atZone(zone).toLocalDate())
    }

    @Test
    fun the_switch_off_schedules_nothing() {
        Birthday.set(ctx, 3, 14)
        BirthdayWish.setEnabled(ctx, false)
        assertFalse(BirthdayWish.scheduled(ctx))
        assertNull(BirthdayWish.nextAt(ctx))
    }

    @Test
    fun no_birthday_schedules_nothing() {
        Birthday.set(ctx, 3, 14)
        BirthdayWish.setEnabled(ctx, true)
        Birthday.clear(ctx)
        assertFalse(BirthdayWish.scheduled(ctx))
    }

    /** The wish pops up as a banner with sound: a high-importance channel, and the old silent
     *  "birthday" channel is gone (OnePlus showed no banner for default importance). */
    @Test
    fun the_birthday_wish_pops_up_as_a_banner_with_sound() {
        val ctx = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        com.voxa.android.notify.Notifier.createChannels(ctx)
        val nm = ctx.getSystemService(android.app.NotificationManager::class.java)
        val ch = nm.getNotificationChannel(com.voxa.android.notify.Notifier.BIRTHDAY.id)
        org.junit.Assert.assertNotNull(ch)
        org.junit.Assert.assertEquals(android.app.NotificationManager.IMPORTANCE_HIGH, ch!!.importance)
        org.junit.Assert.assertNotNull("the channel plays a sound", ch.sound)
        org.junit.Assert.assertNull("the old silent channel is deleted", nm.getNotificationChannel("birthday"))
    }
}
