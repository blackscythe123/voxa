package com.voxa.android

import android.content.Intent
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.ui.Licences
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Settings, Open source licences (docs/guardrails.md#11-licences): the APK carries Voxa's NOTICE
 * (Apache-2.0 section 4(d)) and the licences of what it ships, and the page shows them. Driven
 * through the accessibility tree, like SetupUpdateStepTest.
 */
@RunWith(AndroidJUnit4::class)
class LicencesTest {
    private val inst = InstrumentationRegistry.getInstrumentation()
    private val ctx = inst.targetContext
    private val app = ctx.applicationContext as VoxaApp
    private var onboarded = true
    private var scenario: ActivityScenario<MainActivity>? = null

    @Before
    fun keep() {
        onboarded = app.core.settings().onboarded
        app.core.setSettings(app.core.settings().copy(onboarded = true))
    }

    @After
    fun restore() {
        scenario?.close()
        app.core.setSettings(app.core.settings().copy(onboarded = onboarded))
    }

    @Test
    fun the_shipped_licences_start_with_voxas_notice_and_list_the_libraries() {
        val n = Licences.load(ctx)
        assertEquals("android", n.app)
        assertTrue(n.notice, n.notice.startsWith("Voxa\n"))
        assertTrue(n.notice.contains("created by Simiyon Vinscent Samuel L"))
        assertTrue(n.notice.contains("Apache License, Version 2.0"))
        val names = n.libraries.map { it.name }
        for (want in listOf("androidx.core:core", "org.jetbrains.kotlin:kotlin-stdlib", "net.java.dev.jna:jna", "uniffi", "Manrope font", "Rust standard library")) {
            assertTrue("$want is listed", want in names)
        }
        for (l in n.libraries) {
            assertTrue("${l.name} has a licence", l.licence.isNotEmpty())
            assertTrue("${l.name} has a licence text", l.texts.isNotEmpty() && l.texts.all { it in n.texts.indices })
        }
        // JNA is dual licensed; Voxa takes it under Apache-2.0 and says so.
        val jna = n.libraries.first { it.name == "net.java.dev.jna:jna" }
        assertTrue(jna.note, jna.note.contains("Apache License 2.0"))
    }

    @Test
    fun the_notice_reflows_sentences_but_keeps_its_lines_and_paragraphs() {
        val n = "Voxa\nCopyright 2026 A\n\nIf you redistribute it, section 4(d)\nrequires you to keep this.\nLicensed under\n0.3.x releases.\n"
        assertEquals(
            "Voxa\nCopyright 2026 A\n\nIf you redistribute it, section 4(d) requires you to keep this.\nLicensed under 0.3.x releases.",
            Licences.reflow(n),
        )
    }

    @Test
    fun settings_opens_the_licences_page_with_the_notice_first_and_a_library_licence_text() {
        val i = Intent(ctx, MainActivity::class.java).putExtra(MainActivity.EXTRA_OPEN, MainActivity.OPEN_STORAGE).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        scenario = ActivityScenario.launch(i)
        // The last row of Settings: scrolled to, as a person would.
        tap(scrollTo("Open source licences") ?: throw AssertionError("no Open source licences row in Settings: ${screen()}"))
        assertNotNull(on("Built with", prefix = true))
        val notice = on("Voxa\nCopyright", prefix = true).text.toString()
        assertTrue(notice, notice.contains("created by Simiyon Vinscent Samuel L"))
        // A known library, scrolled to, then its licence text opened.
        val core = scrollTo("androidx.core:core ") ?: throw AssertionError("androidx.core:core is not on the page: ${screen()}")
        assertTrue(core.text.toString(), core.text.toString().matches(Regex("androidx\\.core:core \\d+(\\.\\d+)+")))
        tap(core)
        // Words only the Apache License text has (the NOTICE names the licence too).
        assertNotNull(on("TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION", prefix = false))
    }

    // ── the accessibility tree ───────────────────────────────────────────────────────────────

    private fun roots(): List<AccessibilityNodeInfo> {
        val ua = inst.uiAutomation
        val info = ua.serviceInfo
        if (info != null && info.flags and android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS == 0) {
            info.flags = info.flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
            ua.serviceInfo = info
        }
        return (ua.windows.mapNotNull { it.root } + listOfNotNull(ua.rootInActiveWindow)).filter { it.packageName == ctx.packageName }
    }

    /** prefix = true: the text starts with `text`; false: contains it; null: equals it. */
    private fun walkFind(n: AccessibilityNodeInfo?, text: String, prefix: Boolean?): AccessibilityNodeInfo? {
        if (n == null) return null
        val t = n.text?.toString()
        if (t != null && when (prefix) {
                null -> t == text
                true -> t.startsWith(text)
                false -> t.contains(text)
            }
        ) return n
        for (i in 0 until n.childCount) walkFind(n.getChild(i), text, prefix)?.let { return it }
        return null
    }

    private fun find(text: String, prefix: Boolean? = null, timeoutMs: Long = 5000): AccessibilityNodeInfo? {
        val until = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < until) {
            inst.waitForIdleSync()
            for (r in roots()) walkFind(r, text, prefix)?.let { return it }
            Thread.sleep(200)
        }
        return null
    }

    private fun on(text: String, prefix: Boolean? = null) = find(text, prefix) ?: throw AssertionError("no \"$text\" on screen: ${screen()}")

    private fun screen(): String {
        val out = mutableListOf<String>()
        fun walk(n: AccessibilityNodeInfo?) {
            if (n == null) return
            n.text?.let { out += it.toString().take(60) }
            for (i in 0 until n.childCount) walk(n.getChild(i))
        }
        roots().forEach { walk(it) }
        return out.joinToString(" | ")
    }

    private fun tap(node: AccessibilityNodeInfo) {
        var n = node
        while (!n.isClickable && n.parent != null) n = n.parent
        n.performAction(AccessibilityNodeInfo.ACTION_CLICK)
        inst.waitForIdleSync()
    }

    private fun scrollable(n: AccessibilityNodeInfo?): AccessibilityNodeInfo? {
        if (n == null) return null
        if (n.isScrollable) return n
        for (i in 0 until n.childCount) scrollable(n.getChild(i))?.let { return it }
        return null
    }

    /** The lazy list only composes what is on screen: scroll forward until `prefix` shows. */
    private fun scrollTo(prefix: String): AccessibilityNodeInfo? {
        repeat(80) {
            find(prefix, prefix = true, timeoutMs = 300)?.let { return it }
            val list = roots().firstNotNullOfOrNull { scrollable(it) } ?: return null
            if (!list.performAction(AccessibilityNodeInfo.ACTION_SCROLL_FORWARD)) return find(prefix, prefix = true, timeoutMs = 300)
            inst.waitForIdleSync()
        }
        return null
    }
}
