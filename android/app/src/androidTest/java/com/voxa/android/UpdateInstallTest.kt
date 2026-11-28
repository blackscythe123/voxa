package com.voxa.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.notify.Updater
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.FfiTarget
import uniffi.voxa_ffi.FlowEvent
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.ReadinessInfo
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.VoxaFlow

/** Tapping Install: never while a dictation is live, and the permission is asked only once. */
@RunWith(AndroidJUnit4::class)
class UpdateInstallTest {
    private val inst = InstrumentationRegistry.getInstrumentation()
    private val ctx = inst.targetContext
    private val app = ctx.applicationContext as VoxaApp
    private val p = Updater.prefs(ctx)
    private lateinit var staged: File

    /** "Install unknown apps" as Voxa sees it (the debug seam: revoking the real switch kills the
     *  process, so the real one is driven with adb on the emulator instead). */
    private fun appops(mode: String) {
        Updater.installAllowedForTest = when (mode) {
            "allow" -> true
            "deny" -> false
            else -> null
        }
    }

    @Before
    fun stage() {
        // A fake staged offer for a newer versionCode (never installed: the tests stop before the session).
        staged = File(Updater.updatesDir(ctx).apply { mkdirs() }, "Voxa-Android-test.apk").apply { writeBytes(ByteArray(10)) }
        p.edit().putString("offer_version", "99.0.0").putLong("offer_code", BuildConfig.VERSION_CODE + 1_000_000L)
            .putString("offer_abi", "android-x86_64").putString("offer_url", "https://github.com/blackscythe123/voxa/releases/download/android-v99.0.0/x.apk")
            .putString("offer_sha256", "0".repeat(64)).putLong("offer_size", 10L).putString("staged_path", staged.path)
            .remove("tap_ask_done").remove("await_grant").commit()
    }

    @After
    fun clean() {
        app.keyboardFlow = null
        Updater.cleanUpdates(ctx)
        p.edit().apply { for (k in listOf("version", "code", "abi", "url", "sha256", "size", "notes", "serial", "signer", "auto")) remove("offer_$k") }
            .remove("staged_path").remove("tap_ask_done").remove("await_grant").commit()
        appops("default")
    }

    @Test
    fun install_is_refused_while_a_keyboard_dictation_is_live() {
        appops("allow")
        val dir = File(ctx.cacheDir, "flow-${System.nanoTime()}").apply { mkdirs() }.path
        val flow = VoxaFlow(dir, ReadinessInfo(SessionState.VALID, NetworkState.UP, MicState.GRANTED))
        app.keyboardFlow = flow
        flow.handle(FlowEvent.MicPressed(FfiTarget("Notes", 1uL), false))
        assertTrue(Updater.dictationLive(ctx))
        assertEquals(Updater.Tap.DICTATION_LIVE, Updater.tapInstall(ctx))
        assertEquals("Finish your dictation first, then tap Install again.", Updater.DICTATION_LIVE_TEXT)
        flow.handle(FlowEvent.Cancel)
        assertFalse(Updater.dictationLive(ctx))
        assertEquals(Updater.Tap.INSTALLING, Updater.tapInstall(ctx))
        flow.destroy()
    }

    @Test
    fun without_the_permission_the_one_time_ask_shows_once_then_never_again() {
        appops("deny")
        assertFalse(Updater.canInstall(ctx))
        assertEquals(Updater.Tap.ASK_PERMISSION, Updater.tapInstall(ctx))
        // "Continue" on the sheet: remembered, then Android's switch.
        Updater.askedAtTap(ctx)
        assertTrue(p.getBoolean("tap_ask_done", false))
        // Back without granting: "Voxa can't install updates yet", and the ask never comes again.
        assertEquals(Updater.Tap.CANT_INSTALL, Updater.returnedFromGrant(ctx))
        assertEquals(Updater.Tap.CANT_INSTALL, Updater.tapInstall(ctx))
        assertEquals(Updater.Tap.CANT_INSTALL, Updater.tapInstall(ctx))
        // Allowed later in Settings: installs straight away.
        appops("allow")
        assertEquals(Updater.Tap.INSTALLING, Updater.tapInstall(ctx))
    }

    @Test
    fun nothing_is_installed_without_a_verified_staged_file() {
        staged.delete()
        assertEquals(Updater.Tap.NOT_READY, Updater.tapInstall(ctx))
    }
}
