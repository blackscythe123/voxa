package com.voxa.android

import android.Manifest
import android.app.Notification
import android.app.NotificationManager
import android.media.AudioManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.ime.KbColors
import com.voxa.android.ime.KeyRules
import com.voxa.android.ime.KeyboardLook
import com.voxa.android.ime.KeyboardPanel
import com.voxa.android.ime.Recorder
import com.voxa.android.notify.Notifier
import com.voxa.android.notify.VoxaJobs
import com.voxa.android.ui.ThemePref
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.SessionCheck
import uniffi.voxa_ffi.SessionState
import java.io.File

/**
 * One test per Android bug that was found and fixed, so none comes back (docs/regressions.md).
 * Emulator only; never the owner's phone.
 */
@RunWith(AndroidJUnit4::class)
class RegressionsTest {
    private val inst = InstrumentationRegistry.getInstrumentation()
    private val ctx = inst.targetContext

    // ── the mic key ──────────────────────────────────────────────────────────

    /** Tap to stop did nothing (only a hold stopped the take). */
    @Test
    fun aTapWhileRecordingStopsTheTake() {
        // The press that started the take is a tap: it keeps listening.
        assertFalse(KeyRules.stopOnRelease(recording = true, pressStarted = true, holdToTalk = true, heldMs = 120))
        // The next tap stops, held or not, with hold-to-talk on or off.
        for (hold in listOf(true, false)) for (held in listOf(50L, 900L)) {
            assertTrue("tap $held ms, hold $hold", KeyRules.stopOnRelease(recording = true, pressStarted = false, holdToTalk = hold, heldMs = held))
        }
        // Hold to talk: the starting press held past 400 ms stops on release; off, it never does.
        assertTrue(KeyRules.stopOnRelease(true, true, true, 401))
        assertFalse(KeyRules.stopOnRelease(true, true, false, 5000))
        assertFalse("nothing to stop", KeyRules.stopOnRelease(false, false, true, 900))
    }

    /** A keyboard rebuild (or the view going away) during a hold lost the release, leaving the mic on. */
    @Test
    fun aHoldWhosePressIsTakenAwayStillStops() {
        assertTrue(KeyRules.stopOnCancel(recording = true, pressStarted = true, holdToTalk = true, heldMs = 800))
        assertFalse("a tap's cancel keeps listening", KeyRules.stopOnCancel(true, true, true, 100))
        assertFalse(KeyRules.stopOnCancel(true, false, true, 800))
        assertFalse(KeyRules.stopOnCancel(false, true, true, 800))
    }

    /** Recording during a call gave silence and then "Didn't hear any speech": refuse and say why. */
    @Test
    fun aCallRefusesTheMicAndSaysWhy() {
        assertTrue(KeyRules.inCall(AudioManager.MODE_IN_CALL))
        assertTrue("WhatsApp, Meet", KeyRules.inCall(AudioManager.MODE_IN_COMMUNICATION))
        for (m in listOf(4, 5, 6)) assertTrue("mode $m", KeyRules.inCall(m))
        assertFalse(KeyRules.inCall(AudioManager.MODE_NORMAL))
        assertFalse(KeyRules.inCall(AudioManager.MODE_RINGTONE))
    }

    /** The system took the mic mid-take: what was said is sent; an empty take is dropped with the reason. */
    @Test
    fun aMicTakenMidTakeKeepsWhatWasSaid() {
        assertEquals(KeyRules.MicTaken.StopAndSend, KeyRules.micTaken(0.3f, inCall = true))
        assertEquals(KeyRules.MicTaken.CancelInCall, KeyRules.micTaken(0f, inCall = true))
        assertEquals(KeyRules.MicTaken.CancelMicBusy, KeyRules.micTaken(0.0005f, inCall = false))
    }

    /** Recording stopped after 0.2 s on the owner's phone: a focus blip or a "silenced" flag at start is not a call. */
    @Test
    fun aFocusOrSilenceBlipAtTheStartIsNotAnInterruption() {
        assertFalse(KeyRules.focusLossEndsTake(AudioManager.AUDIOFOCUS_LOSS, 200))
        assertTrue(KeyRules.focusLossEndsTake(AudioManager.AUDIOFOCUS_LOSS, 1500))
        assertFalse("an alarm or a navigation prompt", KeyRules.focusLossEndsTake(AudioManager.AUDIOFOCUS_LOSS_TRANSIENT, 5000))
        assertEquals("silenced at the start must last into the third second", 2300L, KeyRules.silencedConfirmDelayMs(200))
        assertEquals("later it must last 1.5 s", 1500L, KeyRules.silencedConfirmDelayMs(60_000))
    }

    /** A mic read error spun the CPU at 100 %: a lasting run of errors ends the take. */
    @Test
    fun aDeadRecorderEndsTheTakeInsteadOfSpinning() {
        assertFalse(KeyRules.readErrorsEndTake(1))
        assertFalse(KeyRules.readErrorsEndTake(Recorder.MAX_READ_ERRORS - 1))
        assertTrue(KeyRules.readErrorsEndTake(Recorder.MAX_READ_ERRORS))
        assertTrue("about half a second of errors at most", Recorder.MAX_READ_ERRORS * Recorder.READ_ERROR_WAIT_MS <= 1000)
    }

    /** Storage full crashed the keyboard: a recording that can't be written is refused cleanly. */
    @Test
    fun aRecordingThatCannotBeWrittenIsRefusedWithoutACrash() {
        inst.uiAutomation.grantRuntimePermission(ctx.packageName, Manifest.permission.RECORD_AUDIO)
        var cantSave = false
        val r = Recorder(ctx, onLevel = {}, onInterrupted = {}, onCantSave = { cantSave = true })
        val err = r.start("/proc/voxa-cannot-write/take.pcm")
        assertEquals(Recorder.StartError.CantSave, err)
        assertFalse(r.isRecording)
        r.discard()
        assertFalse(cantSave)
    }

    /** A dictation went into another chat of the same app: each focus of a field is its own target. */
    @Test
    fun aFieldTheUserMovedToAndBackIsNotTheOldField() {
        val chatA = KeyRules.fieldIdentity("com.whatsapp", 0x7f0a01, inputSession = 1L)
        val chatB = KeyRules.fieldIdentity("com.whatsapp", 0x7f0a01, inputSession = 2L)
        assertNotEquals("same compose box, another chat", chatA, chatB)
        assertEquals(chatA, KeyRules.fieldIdentity("com.whatsapp", 0x7f0a01, 1L))
        assertNotEquals(KeyRules.fieldIdentity("com.whatsapp", 0, 1L), KeyRules.fieldIdentity("org.telegram", 0, 1L))
    }

    // ── the app ──────────────────────────────────────────────────────────────

    /** The launch animation almost never showed (the keyboard keeps the process alive). */
    @Test
    fun theWrittenSplashPlaysOnEveryLauncherOpen() {
        assertTrue("a launcher open", MainActivity.showsLaunch(frozen = false, freshStart = true, openRequest = null))
        assertFalse("rotation", MainActivity.showsLaunch(false, freshStart = false, openRequest = null))
        assertFalse("the keyboard opening sign-in", MainActivity.showsLaunch(false, true, MainActivity.OPEN_SIGN_IN))
        assertFalse("the keyboard asking for the mic", MainActivity.showsLaunch(false, true, MainActivity.OPEN_MIC))
        assertTrue("a debug freeze", MainActivity.showsLaunch(true, false, MainActivity.OPEN_MIC))
    }

    /** Appearance changes never reached a keyboard already open: they change the stamp it rebuilds on. */
    @Test
    fun anAppearanceChangeReachesALiveKeyboard() {
        val before = ThemePref.load(ctx)
        try {
            val stamp = KeyboardLook.stamp(ctx)
            ThemePref.set(ctx, if (ThemePref.isDark(ctx)) ThemePref.LIGHT else ThemePref.DARK)
            assertNotEquals(stamp, KeyboardLook.stamp(ctx))
        } finally {
            ThemePref.set(ctx, before)
        }
    }

    /** A rebuilt keyboard lost the navigation bar gap and its keys sat under the nav buttons. */
    @Test
    fun aRebuiltPanelKeepsTheNavigationBarGap() {
        inst.runOnMainSync {
            val colors = KbColors.voxa(false, KeyboardLook.Look())
            val a = KeyboardPanel(ctx, colors).apply { bottomInset(0) }
            val b = KeyboardPanel(ctx, colors).apply { bottomInset(132) }
            assertEquals(132, b.paddingBottom - a.paddingBottom)
        }
    }

    /** Owner decision: History's "Your apps" shows at most 8, in rows of four, most used first. */
    @Test
    fun yourAppsStaysCappedAtEight() {
        val apps = (1..13).map { uniffi.voxa_ffi.AppCount(app = "App $it", count = (100 - it).toUInt()) }
        val rows = com.voxa.android.ui.yourApps(apps)
        assertEquals(8, com.voxa.android.ui.YOUR_APPS_CAP)
        assertEquals(listOf(4, 4), rows.map { it.size })
        assertEquals("App 1", rows[0][0].app)
    }

    /** Owner decision: the greeting sits on one line when it fits; it breaks only before the name. */
    @Test
    fun theGreetingIsOnOneLineWhenItFits() {
        assertEquals("Morning, Sam" to true, com.voxa.android.ui.greetingLayout("Morning", "Sam") { it.length <= 14 })
        assertEquals("Good evening,\nAlexandria" to true, com.voxa.android.ui.greetingLayout("Good evening", "Alexandria") { it.length <= 14 })
        assertEquals("Good evening" to false, com.voxa.android.ui.greetingLayout("Good evening", null) { it.length <= 5 })
    }

    /** Owner decision: the birthday is day and month only, kept on the phone (no backup, no year). */
    @Test
    fun theBirthdayIsDayAndMonthOnlyAndStaysOnThePhone() {
        val before = com.voxa.android.ui.Birthday.get(ctx)
        try {
            com.voxa.android.ui.Birthday.set(ctx, 2, 29)
            assertEquals(2 to 29, com.voxa.android.ui.Birthday.get(ctx))
            // Day and month only, never a year: the only other birthday key is the on/off switch
            // "Wish me on my birthday" (a yes/no, not personal data).
            val stored = ctx.getSharedPreferences("ui", 0).all.filterKeys { it.contains("birth", ignoreCase = true) }
            assertTrue(stored.keys.toString(), stored.keys.containsAll(setOf("birthday_month", "birthday_day")))
            assertTrue(stored.keys.toString(), (stored.keys - setOf("birthday_month", "birthday_day", "birthday_wish")).isEmpty())
            assertTrue("the switch is a yes/no", stored["birthday_wish"]?.let { it is Boolean } ?: true)
            assertEquals(2, stored["birthday_month"]); assertEquals(29, stored["birthday_day"])
            val flags = ctx.applicationInfo.flags
            assertEquals("app data is never backed up", 0, flags and android.content.pm.ApplicationInfo.FLAG_ALLOW_BACKUP)
        } finally {
            if (before == null) com.voxa.android.ui.Birthday.clear(ctx) else com.voxa.android.ui.Birthday.set(ctx, before.first, before.second)
        }
    }

    /** Sign out forgets the cached ChatGPT profile picture. */
    @Test
    fun signingOutForgetsTheProfilePicture() {
        val png = File(ctx.filesDir, "avatar.png").apply { writeBytes(byteArrayOf(1, 2, 3)) }
        val url = File(ctx.filesDir, "avatar.url").apply { writeText("https://example.invalid/a.png") }
        inst.runOnMainSync { (ctx.applicationContext as VoxaApp).signOut() }
        assertFalse(png.exists())
        assertFalse(url.exists())
    }

    // ── background send and notifications ────────────────────────────────────

    /** One attempt counter was shared by every dictation; Cloudflare blocking posted "couldn't send". */
    @Test
    fun eachDictationCountsItsOwnTriesAndCloudflareNeverPostsCouldntSend() {
        val m = VoxaJobs.parseAttempts("101:3,202:11,bad,303:x")
        assertEquals(mapOf(101uL to 3, 202uL to 11, 303uL to 0), m)
        assertEquals(m, VoxaJobs.parseAttempts(VoxaJobs.formatAttempts(m)))
        val now = 1_000_000_000_000L
        val recent = (now - 60_000).toULong()
        assertFalse(VoxaJobs.givesUp(recent, 3, now, failedBefore = false))
        assertTrue("after the cap", VoxaJobs.givesUp(recent, VoxaJobs.MAX_ATTEMPTS, now, false))
        assertTrue("a day after it was recorded", VoxaJobs.givesUp((now - 25 * 3_600_000L).toULong(), 1, now, false))
        assertTrue("the user's Retry of a failed one is its last try", VoxaJobs.givesUp(recent, 1, now, true))
        assertFalse(VoxaJobs.postsNotSent(blocked = true))
        assertTrue(VoxaJobs.postsNotSent(blocked = false))
    }

    private fun check(s: SessionState, definite: Boolean) =
        SessionCheck(session = s, network = NetworkState.UP, account = null, blocked = false, definite = definite)

    /** A false "signed out": only a definite answer after a known good session counts, and it is confirmed. */
    @Test
    fun signedOutIsOnlyReportedWhenDefiniteAfterAGoodSession() {
        ctx.getSharedPreferences("notify", 0).edit().clear().commit()
        assertFalse("never good before", Notifier.afterCheck(ctx, check(SessionState.EXPIRED, true), true))
        assertFalse(Notifier.afterCheck(ctx, check(SessionState.VALID, false), false))
        assertFalse("a 401 or 403 may be a rate limit", Notifier.afterCheck(ctx, check(SessionState.EXPIRED, false), false))
        assertTrue("good, then a definite no", Notifier.afterCheck(ctx, check(SessionState.EXPIRED, true), true))
        ctx.getSharedPreferences("notify", 0).edit().clear().commit()
    }

    /** Notices were dropped while the user looked at Voxa; they are held back and posted after. */
    @Test
    fun aNoticeWhileVoxaIsOnScreenIsHeldBackNotDropped() {
        inst.uiAutomation.grantRuntimePermission(ctx.packageName, Manifest.permission.POST_NOTIFICATIONS)
        ctx.getSharedPreferences("notify", 0).edit().clear().commit()
        Notifier.createChannels(ctx)
        Notifier.clearDictations(ctx)
        val nm = ctx.getSystemService(NotificationManager::class.java)
        try {
            Notifier.keyboardShowing = true
            Notifier.ready(ctx, listOf(1_700_000_000_000uL), "Notes")
            Thread.sleep(300)
            assertFalse("not posted while looking", nm.activeNotifications.any { it.id == Notifier.ID_DICTATIONS })
            Notifier.keyboardShowing = false
            var posted: Notification? = null
            for (i in 0 until 40) {
                posted = nm.activeNotifications.firstOrNull { it.id == Notifier.ID_DICTATIONS }?.notification
                if (posted != null) break
                Thread.sleep(100)
            }
            assertNotNull("posted once the user stopped looking", posted)
            assertEquals("lock screen shows only a public version", Notification.VISIBILITY_PRIVATE, posted!!.visibility)
            assertNotNull(posted.publicVersion)
        } finally {
            Notifier.keyboardShowing = false
            Notifier.clearDictations(ctx)
            ctx.getSharedPreferences("notify", 0).edit().clear().commit()
        }
    }
}
