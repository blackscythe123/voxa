package com.voxa.android

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.voxa.android.ime.Contrast
import com.voxa.android.ime.KbColors
import com.voxa.android.ime.KeyboardLook
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** WCAG contrast of the keyboard's fixed colours (review findings S19 to S22). */
@RunWith(AndroidJUnit4::class)
class ContrastTest {
    private val ctx = InstrumentationRegistry.getInstrumentation().targetContext

    /** [fg] with its alpha over the opaque [bg]. */
    private fun over(fg: Int, bg: Int): Int {
        val a = Color.alpha(fg) / 255f
        fun ch(f: Int, b: Int) = Math.round(f * a + b * (1 - a))
        return Color.rgb(ch(Color.red(fg), Color.red(bg)), ch(Color.green(fg), Color.green(bg)), ch(Color.blue(fg), Color.blue(bg)))
    }

    @Test
    fun everyMicColourGivesItsIconAtLeast4_5() {
        for (m in KeyboardLook.micColors) {
            val r = Contrast.ratio(KbColors.micIcon(m.color), m.color)
            assertTrue("${m.name}: $r", r >= 4.5f)
        }
    }

    @Test
    fun voxaLookSecondaryTextAndPastedTitleReadInLightAndDark() {
        for (dark in listOf(false, true)) {
            val c = KbColors.voxa(dark, KeyboardLook.Look())
            val muted = Contrast.ratio(over(c.muted, c.surface), c.surface)
            assertTrue("muted dark=$dark: $muted", muted >= 4.5f)
            val saved = Contrast.ratio(over(c.stageMuted, c.surface), c.surface)
            assertTrue("stage dark=$dark: $saved", saved >= 4.5f)
            val title = Contrast.ratio(c.goodTitle, c.surface)
            assertTrue("pasted title dark=$dark: $title", title >= 4.5f)
            assertTrue(Contrast.ratio(c.onAccent, c.accent) >= 4.5f)
        }
    }

    @Test
    fun theOldTealAndRoseAreReadAsTheNewOnes() {
        ctx.getSharedPreferences("kb_look", 0).edit().putInt("mic_color", 0xFF138A7E.toInt()).commit()
        assertEquals(0xFF0F7A6F.toInt(), KeyboardLook.load(ctx).micColor)
        ctx.getSharedPreferences("kb_look", 0).edit().putInt("mic_color", 0xFFD04870.toInt()).commit()
        assertEquals(0xFFB83A60.toInt(), KeyboardLook.load(ctx).micColor)
        KeyboardLook.reset(ctx)
    }

    @Test
    fun aSmallBrightDetailInADarkCellIsNotAveragedAway() {
        // 320 x 240, black, with a 4 x 4 px white block in every 10 x 10 cell (16 % of each cell).
        val b = Bitmap.createBitmap(320, 240, Bitmap.Config.ARGB_8888)
        b.eraseColor(Color.BLACK)
        for (y in 0 until 240) for (x in 0 until 320) if (x % 10 < 4 && y % 10 < 4) b.setPixel(x, y, Color.WHITE)
        val g = KeyboardLook.gridOf(b)
        b.recycle()
        assertTrue("darkest stays dark", g.lo.all { it < 0.05f })
        assertTrue("brightest is the white detail", g.hi.all { it > 0.95f })
    }
}
