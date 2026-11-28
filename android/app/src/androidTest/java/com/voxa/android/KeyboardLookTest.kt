package com.voxa.android

import android.graphics.Bitmap
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.ime.Contrast
import com.voxa.android.ime.KbColors
import com.voxa.android.ime.KeyboardLook
import com.voxa.android.ime.Photo
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Settings, Keyboard look: the saved look, the colours computed from it, and the photo's safety nets. */
@RunWith(AndroidJUnit4::class)
class KeyboardLookTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun clean() = KeyboardLook.reset(ctx)

    @After
    fun after() = KeyboardLook.reset(ctx)

    @Test
    fun everyChangeBumpsTheVersion() {
        val v = KeyboardLook.version(ctx)
        val stamp = KeyboardLook.stamp(ctx)
        KeyboardLook.save(ctx, KeyboardLook.Look(type = KeyboardLook.COLOR, color = KeyboardLook.presets[3].color))
        assertTrue(KeyboardLook.version(ctx) > v)
        assertNotEquals(stamp, KeyboardLook.stamp(ctx))
        assertEquals("Navy", KeyboardLook.label(KeyboardLook.load(ctx)))
    }

    @Test
    fun theVoxaLookIsTheV6Palette() {
        val light = KbColors.voxa(false, KeyboardLook.Look())
        assertEquals(0xFFF3F3F1.toInt(), light.surface)
        assertEquals(0xFFFFFFFF.toInt(), light.key)
        assertEquals(0xFF8F4C2C.toInt(), light.accent)
        assertTrue("the 1 dp key elevation stays in Voxa light", light.keyElevation)
        val dark = KbColors.voxa(true, KeyboardLook.Look())
        assertEquals(0xFF1A1A19.toInt(), dark.surface)
        assertFalse(dark.keyElevation)
        assertEquals(0, KbColors.voxa(false, KeyboardLook.Look(keyShapes = false)).key)
    }

    @Test
    fun everyPresetReads() {
        for (p in KeyboardLook.presets) {
            val c = KbColors.solid(p.color, KeyboardLook.Look(type = KeyboardLook.COLOR, color = p.color))
            assertTrue("${p.name}: text on background", Contrast.ratio(c.text, c.surface) >= 4.5f)
            assertTrue("${p.name}: text on keys", Contrast.ratio(c.text, c.key) >= 4.5f)
            assertFalse("${p.name}: no key shadows", c.keyElevation)
            // The title is green only when green reads; otherwise the text colour.
            assertTrue("${p.name}: done title", Contrast.ratio(c.goodTitle, c.surface) >= 4.5f)
            // Mic icon: white or near-black, whichever reads on the mic.
            assertTrue("${p.name}: mic icon", Contrast.ratio(c.onAccent, c.accent) >= 3f)
        }
    }

    @Test
    fun aMicThatDoesNotStandOutGetsATextRing() {
        // Sepia mic on a sepia surface: below 3:1, so the ring is the text colour at 85 %.
        val ring = KbColors.ring(0xFF8F4C2C.toInt(), 0xFF8A4A2A.toInt(), KbColors.LIGHT_FG, true)
        assertEquals(KbColors.withAlpha(KbColors.LIGHT_FG, 0.85f), ring)
        // A sepia mic on near-white: the soft accent ring as before.
        assertEquals(KbColors.withAlpha(0xFF8F4C2C.toInt(), 0.12f), KbColors.ring(0xFF8F4C2C.toInt(), 0xFFF3F3F1.toInt(), KbColors.DARK_FG, false))
    }

    /** A photo with a white top half and a black bottom half (worst case for one text colour). */
    private fun importTest(name: String, draw: (Bitmap) -> Unit): Boolean {
        val bmp = Bitmap.createBitmap(1200, 900, Bitmap.Config.ARGB_8888)
        draw(bmp)
        val f = File(ctx.cacheDir, name)
        f.outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        return KeyboardLook.importPhoto(ctx, Uri.fromFile(f))
    }

    @Test
    fun aPhotoIsCopiedAndItsStrengthIsCappedForReading() {
        val ok = importTest("half.png") { b ->
            for (y in 0 until b.height) for (x in 0 until b.width) b.setPixel(x, y, if (y < b.height / 2) -1 else 0xFF000000.toInt())
        }
        assertTrue(ok)
        assertTrue(KeyboardLook.photoFile(ctx).length() in 1..400_000)
        val look = KeyboardLook.load(ctx)
        assertEquals(KeyboardLook.PHOTO, look.type)
        val grid = KeyboardLook.readGrid(ctx)
        val cap = Photo.maxStrength(grid, 0.5f, false, true, Photo.aspect(ctx))
        assertTrue("half white, half black can't show fully", cap < 1f)
        assertTrue(look.strength <= cap + 1e-4f)
        // At the cap, the computed colours meet the minimums (checked again on the real grid).
        val c = KbColors.forLook(ctx, look.copy(strength = 1f), false)
        assertTrue(c.washAlpha >= 1f - cap - 1e-4f)
        // Night: the wash is 20 points stronger.
        assertEquals((Photo.washAlpha(0.5f, false) + 0.2f).coerceAtMost(1f), Photo.washAlpha(0.5f, true), 1e-4f)
    }

    @Test
    fun aBrokenPhotoFallsBackToVoxaWithoutCrashing() {
        // The pick itself: junk bytes are refused, nothing changes.
        val junk = File(ctx.cacheDir, "junk.jpg").apply { writeBytes(ByteArray(4096) { (it * 31).toByte() }) }
        assertFalse(KeyboardLook.importPhoto(ctx, Uri.fromFile(junk)))
        assertEquals(KeyboardLook.VOXA, KeyboardLook.load(ctx).type)

        // A saved copy that later went bad: decoding throws, and the colours fall back.
        assertTrue(importTest("ok.png") { it.eraseColor(0xFF336699.toInt()) })
        KeyboardLook.photoFile(ctx).writeBytes(ByteArray(2048) { 7 })
        val m = ctx.resources.displayMetrics
        val threw = runCatching { KeyboardLook.decodePhoto(ctx, m.widthPixels, 800, 0.5f, false, true) }.isFailure
        assertTrue(threw)

        // A missing copy: KbColors.of gives the Voxa look and clears the saved choice.
        KeyboardLook.photoFile(ctx).delete()
        val c = KbColors.of(ctx)
        assertFalse(c.photo)
        assertEquals(KeyboardLook.VOXA, KeyboardLook.load(ctx).type)
    }
}
