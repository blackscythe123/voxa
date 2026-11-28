package com.voxa.android

import android.content.Context
import com.voxa.android.notify.Updater
import android.content.Intent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The onboarding step "Keep Voxa up to date" (spec 6.3), driven through the accessibility tree
 * (no extra test library): it sits after the keyboard and before "Try it", "Not now" moves on and
 * is remembered, and "Install unknown apps" allowed makes it done.
 */
@RunWith(AndroidJUnit4::class)
class SetupUpdateStepTest {
    private val inst = InstrumentationRegistry.getInstrumentation()
    private val ctx = inst.targetContext
    private val app = ctx.applicationContext as VoxaApp
    private val ui = ctx.getSharedPreferences("ui", Context.MODE_PRIVATE)
    private val update = ctx.getSharedPreferences("update", Context.MODE_PRIVATE)
    private var onboarded = true
    private var step = 0
    private var scenario: ActivityScenario<MainActivity>? = null

    @Before
    fun keep() {
        onboarded = app.core.settings().onboarded
        step = ui.getInt("setup_step", 0)
        app.core.setSettings(app.core.settings().copy(onboarded = false))
        update.edit().remove("onboarding_ask").commit()
    }

    @After
    fun restore() {
        scenario?.close()
        app.core.setSettings(app.core.settings().copy(onboarded = onboarded))
        ui.edit().putInt("setup_step", step).commit()
        update.edit().remove("onboarding_ask").commit()
        appops("default")
    }

    /** "Install unknown apps" as Voxa sees it (the debug seam: revoking the real switch kills the
     *  process, so the real one is driven with adb on the emulator instead). */
    private fun appops(mode: String) {
        Updater.installAllowedForTest = when (mode) {
            "allow" -> true
            "deny" -> false
            else -> null
        }
    }

    /** Open setup on `at` (no splash: an "open" extra skips the launch animation). */
    private fun open(at: Int) {
        ui.edit().putInt("setup_step", at).commit()
        val i = Intent(ctx, MainActivity::class.java).putExtra(MainActivity.EXTRA_OPEN, "test").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        scenario = ActivityScenario.launch(i)
    }

    private fun roots(): List<AccessibilityNodeInfo> {
        val ua = inst.uiAutomation
        val info = ua.serviceInfo
        if (info != null && info.flags and android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS == 0) {
            info.flags = info.flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            ua.serviceInfo = info
        }
        return (ua.windows.mapNotNull { it.root } + listOfNotNull(ua.rootInActiveWindow)).filter { it.packageName == ctx.packageName }
    }

    private fun find(text: String, timeoutMs: Long = 5000): AccessibilityNodeInfo? {
        val until = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < until) {
            inst.waitForIdleSync()
            // A tree walk: Compose's nodes don't answer findAccessibilityNodeInfosByText.
            for (r in roots()) walkFind(r, text)?.let { return it }
            Thread.sleep(200)
        }
        return null
    }

    private fun walkFind(n: AccessibilityNodeInfo?, text: String): AccessibilityNodeInfo? {
        if (n == null) return null
        if (n.text?.toString() == text) return n
        for (i in 0 until n.childCount) walkFind(n.getChild(i), text)?.let { return it }
        return null
    }

    /** What is on screen, for a failure message. */
    private fun screen(): String {
        val out = mutableListOf<String>()
        fun walk(n: AccessibilityNodeInfo?) {
            if (n == null) return
            n.text?.let { out += it.toString() }
            for (i in 0 until n.childCount) walk(n.getChild(i))
        }
        roots().forEach { walk(it) }
        return out.joinToString(" | ")
    }

    private fun on(text: String) = find(text) ?: throw AssertionError("no \"$text\" on screen: ${screen()}")

    private fun absent(text: String) = find(text, 1500) == null

    private fun click(text: String) {
        var n = on(text)
        while (!n.isClickable && n.parent != null) n = n.parent
        n.performAction(AccessibilityNodeInfo.ACTION_CLICK)
        inst.waitForIdleSync()
    }

    @Test
    fun the_update_step_comes_after_the_keyboard_and_before_try() {
        open(2)
        assertNotNull(on("Turn on the Voxa keyboard"))
        scenario!!.close()
        open(3)
        assertNotNull(on("Keep Voxa up to date"))
        assertNotNull(on("New versions of Voxa install with one tap. Android asks you once to allow this. On the next screen, turn on the switch for Voxa, then come back."))
        scenario!!.close()
        open(4)
        assertNotNull(on("Switch to Voxa and try it"))
    }

    @Test
    fun not_now_moves_on_and_is_remembered_as_skipped() {
        appops("deny")
        open(3)
        assertNotNull(on("Allow updates"))
        click("Not now")
        assertNotNull(on("Switch to Voxa and try it"))
        assertEquals("skipped", update.getString("onboarding_ask", null))
        assertEquals(4, ui.getInt("setup_step", -1))
    }

    @Test
    fun the_step_is_done_once_install_is_allowed() {
        appops("deny")
        open(3)
        assertNotNull(on("Allow updates"))
        // Allowed in Android's settings, then back in Voxa: re-read on resume.
        appops("allow")
        scenario!!.moveToState(Lifecycle.State.STARTED)
        scenario!!.moveToState(Lifecycle.State.RESUMED)
        assertNotNull(on("Updates allowed"))
        click("Continue")
        assertEquals("granted", update.getString("onboarding_ask", null))
        assertEquals(4, ui.getInt("setup_step", -1))
    }

    @Test
    fun sign_in_is_never_blocked_by_the_update_step() {
        open(0)
        assertNotNull(on("Sign in to ChatGPT"))
        assert(absent("Keep Voxa up to date"))
        click(if (find("Not now", 1500) != null) "Not now" else "Continue")
        // The next step is the mic, not updates.
        assertNotNull(on("Allow the microphone"))
        assertNull(update.getString("onboarding_ask", null))
    }

    @Test
    fun setup_resumes_on_the_update_step_after_a_process_restart() {
        // The step is persisted; a recreated activity (the closest in-process stand-in for the
        // process restart Android does when the permission is toggled) opens on it again.
        open(3)
        assertNotNull(on("Keep Voxa up to date"))
        scenario!!.recreate()
        assertNotNull(on("Keep Voxa up to date"))
        assertEquals(3, ui.getInt("setup_step", -1))
    }
}
