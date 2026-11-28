package com.voxa.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.UpdateFailure
import uniffi.voxa_ffi.updateCheck
import uniffi.voxa_ffi.updateVerifyFeedBytes

/**
 * The shared verifier through FFI on a real Android runtime (Kotlin never verifies a signature
 * itself). The feeds in androidTest/assets/update are signed with the TEST key with OpenSSL,
 * exactly as CI signs a real one (tools/update/make_test_feed.py).
 */
@RunWith(AndroidJUnit4::class)
class UpdateFfiTest {
    private val assets = InstrumentationRegistry.getInstrumentation().context.assets
    private fun asset(name: String) = assets.open("update/$name").use { it.readBytes() }
    private val key = String(asset("test-k1.pub.hex")).trim()
    private val arm = listOf("arm64-v8a", "armeabi-v7a")

    @Test
    fun a_test_signed_android_feed_verifies_and_offers_the_phone_abi() {
        val r = updateVerifyFeedBytes(asset("test-android-10001.json"), key, arm, 10000uL, 0uL)
        val o = r.offer!!
        assertEquals("1.0.1", o.version)
        assertEquals(10001uL, o.versionCode)
        assertEquals("android-arm64-v8a", o.abi)
        assertTrue(o.url.endsWith("Voxa-Android-1.0.1-arm64-v8a.apk"))
        assertEquals(1764331200uL, r.serial)
    }

    @Test
    fun an_unknown_abi_falls_back_to_the_universal_apk() {
        val o = updateVerifyFeedBytes(asset("test-android-10001.json"), key, listOf("x86_64"), 10000uL, 0uL).offer!!
        assertEquals("android-universal", o.abi)
    }

    @Test
    fun an_older_version_code_gives_no_offer() {
        assertNull(updateVerifyFeedBytes(asset("test-android-10001.json"), key, arm, 10001uL, 0uL).offer)
        assertNull(updateVerifyFeedBytes(asset("test-android-10001.json"), key, arm, 20000uL, 0uL).offer)
        // The version code decides, never the name: 20000 is offered to 10001.
        assertEquals(20000uL, updateVerifyFeedBytes(asset("test-android-20000.json"), key, arm, 10001uL, 0uL).offer!!.versionCode)
    }

    @Test
    fun one_changed_byte_or_another_key_is_refused() {
        val feed = asset("test-android-10001.json")
        // Change one character inside the base64 payload.
        val text = String(feed)
        val i = text.indexOf("\"payload\": \"") + 20
        val tampered = (text.substring(0, i) + (if (text[i] == 'A') 'B' else 'A') + text.substring(i + 1)).toByteArray()
        try {
            updateVerifyFeedBytes(tampered, key, arm, 10000uL, 0uL)
            fail("a changed payload verified")
        } catch (e: UpdateFailure) {
            // A changed byte in the base64 is a bad signature (refused) or an unreadable envelope.
            assertTrue(e is UpdateFailure.Refused || e is UpdateFailure.Offline)
        }
        try {
            updateVerifyFeedBytes(feed, "11".repeat(32), arm, 10000uL, 0uL)
            fail("another key verified")
        } catch (e: UpdateFailure) {
            assertTrue(e is UpdateFailure.Refused)
        }
    }

    @Test
    fun a_lower_serial_than_already_seen_is_refused() {
        try {
            updateVerifyFeedBytes(asset("test-android-10001.json"), key, arm, 10000uL, 1764331201uL)
            fail("a replayed feed verified")
        } catch (e: UpdateFailure) {
            assertTrue(e is UpdateFailure.Refused)
        }
    }

    @Test
    fun a_second_check_right_after_one_does_no_network() {
        // A local test origin nobody listens on: the first manual check goes out (and fails offline).
        val feed = "http://127.0.0.1:9/feed/android.json"
        val now = System.currentTimeMillis().toULong()
        val first = updateCheck(arm, 10000uL, "", now, true, feed, key)
        assertTrue(first.checked)
        assertEquals("offline", first.failure)
        // Pressed again within a minute: the last answer stands, nothing goes to the network.
        val again = updateCheck(arm, 10000uL, first.state, now + 5_000uL, true, feed, key)
        assertTrue(!again.checked)
        // An automatic check right after is not due either (this install's slot, then backoff).
        val auto = updateCheck(arm, 10000uL, again.state, now + 6_000uL, false, feed, key)
        assertTrue(!auto.checked)
    }
}
