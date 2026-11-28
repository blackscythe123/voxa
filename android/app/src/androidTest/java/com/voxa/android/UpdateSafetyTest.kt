package com.voxa.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.notify.Updater
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.updateKeysReady

/** The 1.0.0 release review's Android update fixes: a refused file is never fetched again, a
 *  manual-only offer is never downloaded, and a build that can't update says so. */
@RunWith(AndroidJUnit4::class)
class UpdateSafetyTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext
    private val p = Updater.prefs(ctx)
    private val sha = "ab".repeat(32)

    private fun offer(auto: Boolean) {
        p.edit().putString("offer_version", "99.0.0").putLong("offer_code", BuildConfig.VERSION_CODE + 1_000_000L)
            .putString("offer_abi", "android-x86_64").putString("offer_url", "https://github.com/blackscythe123/voxa/releases/download/android-v99.0.0/Voxa-Android-99.0.0-x86_64.apk")
            .putString("offer_sha256", sha).putLong("offer_size", 10L).putBoolean("offer_auto", auto).remove("staged_path").commit()
    }

    @Before
    fun clean() {
        p.edit().remove("rejected_sha256").commit()
    }

    @After
    fun tidy() {
        Updater.cleanUpdates(ctx)
        p.edit().apply { for (k in listOf("version", "code", "abi", "url", "sha256", "size", "notes", "serial", "signer", "auto")) remove("offer_$k") }
            .remove("staged_path").remove("rejected_sha256").commit()
    }

    @Test
    fun a_download_the_phone_refused_is_never_fetched_again() {
        offer(auto = true)
        Updater.reject(ctx, sha.uppercase())
        assertTrue(sha in Updater.rejected(ctx))
        // The job returns "done" at once: no network, nothing staged, no daily retry.
        assertTrue(runBlocking { Updater.download(ctx) })
        assertNull(Updater.staged(ctx))
        assertTrue(Updater.updatesDir(ctx).listFiles().orEmpty().none { it.name.startsWith(".part-") })
    }

    @Test
    fun a_manual_only_offer_is_never_downloaded_and_points_to_the_release_page() {
        offer(auto = false)
        assertTrue(runBlocking { Updater.download(ctx) })
        assertNull(Updater.staged(ctx))
        assertEquals("New version", Updater.rowValue(Updater.Status.MANUAL, null, 0))
        assertEquals("Voxa 99.0.0 is out. Download it from its release page to update.", Updater.statusLine(Updater.Status.MANUAL, "99.0.0", 1))
    }

    @Test
    fun a_build_with_placeholder_keys_says_updates_are_off_and_never_checks() {
        assertEquals("Off in this build", Updater.rowValue(Updater.Status.OFF, null, 0))
        assertEquals("Updates are off in this build. Download Voxa from its release page to get updates.", Updater.statusLine(Updater.Status.OFF, null, 1))
        if (!updateKeysReady() && ctx.getSharedPreferences("debug", 0).getString("update_feed", null).isNullOrBlank()) {
            assertTrue(Updater.updatesOff(ctx))
            assertEquals(Updater.Checked.Skipped, runBlocking { Updater.check(ctx, manual = true) })
            assertEquals(Updater.Status.OFF, Updater.status.value)
        }
    }
}
