package com.voxa.android

import android.util.Log
import android.webkit.WebSettings
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.DeliveryResult
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.FfiTarget
import uniffi.voxa_ffi.FieldKind
import uniffi.voxa_ffi.FlowEffect
import uniffi.voxa_ffi.FlowEvent
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.ReadinessInfo
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.VoxaCore
import uniffi.voxa_ffi.VoxaFlow
import uniffi.voxa_ffi.WordPair
import uniffi.voxa_ffi.Words
import uniffi.voxa_ffi.cleanText
import uniffi.voxa_ffi.coreVersion
import uniffi.voxa_ffi.fieldKind
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** The Rust core through UniFFI, on a real Android runtime (emulator or phone). */
@RunWith(AndroidJUnit4::class)
class CoreFfiTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext

    private fun tmp(name: String) = File(ctx.cacheDir, "ffi-test-$name").apply { deleteRecursively(); mkdirs() }.path

    @Test
    fun nativeLibraryLoads() {
        val v = coreVersion()
        Log.i(TAG, "FFI coreVersion() = $v")
        assertTrue(v.isNotEmpty())
    }

    @Test
    fun textPipelineRunsInRust() {
        val words = Words(
            fixes = listOf(WordPair("clawed code", "Claude Code")),
            snippets = listOf(WordPair("my email", "sam@example.com")),
            spokenFormatting = true,
            rules = emptyList(),
        )
        val out = cleanText("Um, ask clawed code. New line. Mail my email.", true, words, "Notes")
        Log.i(TAG, "FFI cleanText -> ${out.replace("\n", "\\n")}")
        assertEquals("Ask Claude Code.\nMail sam@example.com.", out)
    }

    @Test
    fun flowRoundTripInsertsAndSavesHistory() {
        val dir = tmp("flow")
        val flow = VoxaFlow(dir, ReadinessInfo(SessionState.VALID, NetworkState.UP, MicState.GRANTED))
        val target = FfiTarget("Notes", 42uL)
        val start = flow.handle(FlowEvent.MicPressed(target, false)).filterIsInstance<FlowEffect.StartCapture>().single()
        assertEquals("Listening", flow.status()?.title)
        flow.handle(FlowEvent.Stop)
        assertEquals("Writing it out", flow.status()?.title)
        val paste = flow.handle(FlowEvent.Transcribed(start.job, "Hello from the emulator.")).filterIsInstance<FlowEffect.Paste>().single()
        assertEquals(target, paste.target)
        flow.handle(FlowEvent.Delivered(start.job, DeliveryResult.Inserted))
        assertEquals("Pasted in Notes", flow.status()?.title)
        val h = VoxaCore(dir).history("")
        assertEquals(EntryStatus.Pasted, h.single().status)
        assertEquals("Hello from the emulator.", h.single().text)
        flow.destroy()
    }

    @Test
    fun passwordFieldsAreBlocked() {
        assertEquals(FieldKind.PASSWORD, fieldKind(0x81, 0))
        assertEquals(FieldKind.NORMAL, fieldKind(0x01, 0))
    }

    /**
     * Spike R2: does chatgpt.com answer the Rust client (reqwest + rustls) from Android, or does
     * Cloudflare challenge it? Uses a dummy cookie, so the expected good answer is "signed out" (a
     * JSON reply without a token), not "blocked". Android's own HTTP stack is asked the same thing
     * for comparison. Needs network; logs the outcome.
     */
    @Test
    fun spikeChatGptReachableFromRust() {
        val ua = WebSettings.getDefaultUserAgent(ctx).replace("; wv", "")
        val core = VoxaCore(tmp("spike"))
        core.useSession("voxa-spike=1", ua)
        val r = core.checkSession()
        Log.i(TAG, "SPIKE rust/rustls session check: session=${r.session} network=${r.network} blocked=${r.blocked}")

        val conn = URL("https://chatgpt.com/api/auth/session").openConnection() as HttpURLConnection
        conn.setRequestProperty("User-Agent", ua)
        conn.setRequestProperty("Cookie", "voxa-spike=1")
        val code = runCatching { conn.responseCode }.getOrDefault(-1)
        val type = conn.contentType
        val cf = conn.getHeaderField("cf-mitigated")
        Log.i(TAG, "SPIKE android/platform same request: status=$code content-type=$type cf-mitigated=$cf")
        conn.disconnect()

        assertTrue("Rust client got no classified answer (offline?)", r.network == NetworkState.UP)
        assertTrue("Cloudflare challenged the Rust client (blocked=${r.blocked})", !r.blocked && r.session == SessionState.EXPIRED)
    }

    companion object {
        const val TAG = "VoxaTest"
    }
}
