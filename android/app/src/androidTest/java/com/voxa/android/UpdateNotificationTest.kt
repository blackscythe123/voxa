package com.voxa.android

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.notify.Notifier
import com.voxa.android.notify.Updater
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.noticeUpdatePublicTitle
import uniffi.voxa_ffi.noticeUpdateReady

/** A4 "Voxa 1.1.0 is ready to install" on a real Android runtime (docs/notifications.md). */
@RunWith(AndroidJUnit4::class)
class UpdateNotificationTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext
    private val nm = ctx.getSystemService(NotificationManager::class.java)
    private val notify = ctx.getSharedPreferences("notify", Context.MODE_PRIVATE)
    private val update = Updater.prefs(ctx)
    private var posted: String? = null

    @Before
    fun clean() {
        if (Build.VERSION.SDK_INT >= 33) {
            InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(ctx.packageName, Manifest.permission.POST_NOTIFICATIONS)
        }
        Notifier.createChannels(ctx)
        // Anything an earlier test left held back is posted now (off the main thread): let it land.
        Notifier.activityResumed = false
        Notifier.keyboardShowing = false
        Thread.sleep(500)
        posted = notify.getString("posted", null)
        notify.edit().remove("posted").commit()
        update.edit().remove("notified_for").commit()
        nm.cancel(Notifier.ID_UPDATE)
    }

    @After
    fun restore() {
        nm.cancel(Notifier.ID_UPDATE)
        notify.edit().apply { if (posted == null) remove("posted") else putString("posted", posted) }.commit()
        update.edit().remove("notified_for").commit()
        Notifier.activityResumed = false
    }

    private fun showing() = nm.activeNotifications.firstOrNull { it.id == Notifier.ID_UPDATE }

    @Test
    fun the_updates_channel_is_low_importance_and_a4_uses_id_4() {
        val ch = nm.getNotificationChannel("updates")
        assertNotNull(ch)
        assertEquals("Updates", ch.name.toString())
        assertEquals("When a new version of Voxa is ready to install", ch.description)
        assertEquals(NotificationManager.IMPORTANCE_LOW, ch.importance)
        assertEquals(4, Notifier.ID_UPDATE)
    }

    @Test
    fun a4_has_the_title_and_the_public_lock_screen_text() {
        assertEquals("Voxa 1.1.0 is ready to install", noticeUpdateReady("1.1.0").title)
        assertEquals("Tap to install. Your history and settings stay as they are.", noticeUpdateReady("1.1.0").body)
        assertEquals("Install", noticeUpdateReady("1.1.0").action)
        assertEquals("Voxa update ready", noticeUpdatePublicTitle())
        assertTrue(Notifier.updateReady(ctx, "1.1.0"))
        val n = showing()!!.notification
        assertEquals("updates", n.channelId)
        assertEquals("Voxa 1.1.0 is ready to install", n.extras.getString("android.title"))
        // Private: the lock screen shows only the public version ("Voxa update ready").
        assertEquals(android.app.Notification.VISIBILITY_PRIVATE, n.visibility)
        assertEquals("Voxa update ready", n.publicVersion.extras.getString("android.title"))
        assertEquals("Install", n.actions.single().title.toString())
    }

    @Test
    fun a4_posts_once_per_version_even_after_it_is_swiped_away() {
        assertTrue(Notifier.updateReady(ctx, "1.1.0"))
        nm.cancel(Notifier.ID_UPDATE)
        assertFalse(Notifier.updateReady(ctx, "1.1.0"))
        assertTrue(showing() == null)
        notify.edit().remove("posted").commit()
        assertTrue(Notifier.updateReady(ctx, "1.2.0"))
    }

    @Test
    fun a4_is_not_posted_while_voxa_is_on_screen_and_posts_once_it_is_not() {
        Notifier.activityResumed = true
        assertFalse(Notifier.updateReady(ctx, "1.1.0"))
        assertTrue(showing() == null)
        // Held back, not dropped: posted once the user stops looking.
        Notifier.activityResumed = false
        val until = System.currentTimeMillis() + 5000
        while (showing() == null && System.currentTimeMillis() < until) Thread.sleep(100)
        assertNotNull(showing())
    }

    @Test
    fun a4_waits_when_the_daily_cap_is_used() {
        // Two notifications in the last day: A4 waits (deferred, not dropped).
        val now = System.currentTimeMillis()
        notify.edit().putString("posted", "${now - 1000},${now - 2000}").commit()
        assertFalse(Notifier.updateReady(ctx, "1.1.0"))
        assertTrue(showing() == null)
        assertTrue(update.getString("notified_for", "").isNullOrEmpty())
        // A day later (the cap has room again) the same version still posts.
        notify.edit().putString("posted", "${now - 25 * 3600_000L}").commit()
        assertTrue(Notifier.updateReady(ctx, "1.1.0"))
    }
}
