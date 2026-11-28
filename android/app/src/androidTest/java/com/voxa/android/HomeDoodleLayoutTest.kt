package com.voxa.android

import android.graphics.Path
import android.graphics.PathMeasure
import androidx.core.graphics.PathParser
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.voxa.android.ui.DoodleLayout
import com.voxa.android.ui.DpBox
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.voxa_ffi.DoodleArt
import uniffi.voxa_ffi.doodleArt
import java.time.LocalDate

/**
 * The phone's Home header layout, locked by the owner (docs/ui-mocks/v7-home-fullscreen-compare.html,
 * "Phone, new", doodle above, bigger): each doodle cropped to its own drawing, scaled up to 150 dp
 * tall, centred, never under the avatar, the greeting right under it.
 */
@RunWith(AndroidJUnit4::class)
class HomeDoodleLayoutTest {
    /** The 12 doodles of the mockup: the 11, with the streak at 7 and at 30 days. */
    private val twelve = listOf(
        "sunrise" to 0u, "morning" to 0u, "midday" to 0u, "evening" to 0u, "night" to 0u, "weekend" to 0u,
        "streak" to 7u, "streak" to 30u, "words" to 0u, "first" to 0u, "birthday" to 0u, "newyear" to 0u,
    )

    /** Content widths of 360, 393 and 412 dp phones (Home's 24 dp side padding). */
    private val widths = listOf(360f, 393f, 412f).map { it - 48f }

    private fun arts(days: Int = 14): List<Triple<String, String, DoodleArt>> =
        (0 until days).flatMap { k ->
            val date = LocalDate.of(2026, 11, 28).plusDays(k.toLong()).toString()
            twelve.map { (d, n) -> Triple("$d $n $date", date, doodleArt(d, date, n.toUInt())) }
        }

    @Test
    fun every_doodle_is_cropped_to_its_own_drawing_not_the_band() {
        for ((name, _, art) in arts()) {
            val crop = DoodleLayout.crop(art.bounds)
            val inside = { x: Float, y: Float -> x >= crop[0] && x <= crop[2] && y >= crop[1] && y <= crop[3] }
            assertEquals("$name: one horizon", 1, art.strokes.count { it.horizon })
            for (s in art.strokes.filter { !it.horizon }) {
                // Sample the parsed path along its length, as the screen draws it.
                val pm = PathMeasure(PathParser.createPathFromPathData(s.d), false)
                val pos = FloatArray(2)
                do {
                    var at = 0f
                    while (at <= pm.length) {
                        pm.getPosTan(at, pos, null)
                        assertTrue("$name: (${pos[0]}, ${pos[1]}) outside ${crop.toList()}", inside(pos[0], pos[1]))
                        at += 0.5f
                    }
                } while (pm.nextContour())
            }
            // Smaller than the 343 x 112 band, and the horizon's 6 to 337 run is left out.
            assertTrue("$name: ${crop.toList()}", crop[0] > 2f || crop[2] < 341f)
        }
    }

    @Test
    fun the_doodle_never_runs_under_the_avatar_at_360_393_and_412_dp() {
        for (w in widths) {
            val avatar = DoodleLayout.avatar(w)
            for ((name, _, art) in arts()) {
                val c = DoodleLayout.crop(art.bounds)
                val box = DoodleLayout.place(w, c[2] - c[0], c[3] - c[1])
                assertFalse("$name at $w dp: $box under $avatar", box.overlaps(avatar))
                assertFalse("$name at $w dp: $box in the corner", box.overlaps(DoodleLayout.safeCorner(w)))
                assertTrue("$name at $w dp: $box inside the width", box.x >= -0.01f && box.right <= w + 0.01f)
            }
        }
    }

    @Test
    fun the_doodle_is_at_most_150_dp_tall_keeps_its_shape_and_is_centred() {
        for (w in widths) {
            for ((name, _, art) in arts(3)) {
                val c = DoodleLayout.crop(art.bounds)
                val box = DoodleLayout.place(w, c[2] - c[0], c[3] - c[1])
                assertTrue("$name at $w: ${box.h}", box.h <= DoodleLayout.MAX_H + 0.01f && box.h > 40f)
                assertEquals("$name at $w: aspect", (c[2] - c[0]) / (c[3] - c[1]), box.w / box.h, 0.01f)
                assertEquals("$name at $w: centred", w / 2f, box.x + box.w / 2f, 0.01f)
                // Bigger than the old band: the old one drew it at the band's scale, w / 343.
                assertTrue("$name at $w: ${box.w / (c[2] - c[0])} not bigger", box.w / (c[2] - c[0]) > w / 343f)
            }
        }
    }

    @Test
    fun the_greeting_follows_the_doodle_directly() {
        assertTrue(DoodleLayout.GREETING_GAP in 8f..12f)
        for (w in widths) {
            for ((name, _, art) in arts(3)) {
                val c = DoodleLayout.crop(art.bounds)
                val box = DoodleLayout.place(w, c[2] - c[0], c[3] - c[1])
                // The row ends at the doodle's bottom: no band of empty space under it.
                assertEquals("$name at $w", box.bottom, DoodleLayout.rowHeight(w, box), 0.01f)
            }
        }
    }

    @Test
    fun the_notebook_that_ran_under_the_avatar_now_stays_clear() {
        // The owner's check: the big notebook (first words) ran under the avatar.
        val art = doodleArt("first", "2026-11-28", 0u)
        val c = DoodleLayout.crop(art.bounds)
        for (w in widths) {
            val box = DoodleLayout.place(w, c[2] - c[0], c[3] - c[1])
            val full = minOf(DoodleLayout.MAX_H, w / ((c[2] - c[0]) / (c[3] - c[1])))
            assertFalse(box.overlaps(DoodleLayout.avatar(w)))
            assertTrue("at $w: $box", box.h >= DoodleLayout.KEEP_NARROW * full - 0.01f)
        }
    }

    @Test
    fun a_doodle_clear_of_the_corner_sits_at_the_top_with_no_band_above() {
        val w = 345f
        val tall = DoodleLayout.place(w, 80f, 60f)
        assertEquals(0f, tall.y, 0f)
        assertEquals(DoodleLayout.MAX_H, tall.h, 0.01f)
        // A wide one that would run under the avatar is narrowed or moved below the avatar row.
        val wide = DoodleLayout.place(w, 330f, 70f)
        assertFalse(wide.overlaps(DoodleLayout.safeCorner(w)))
        assertTrue(wide.y == 0f || wide.y == DoodleLayout.safeCorner(w).bottom)
        assertEquals(DpBox(0f, 0f, 1f, 1f).overlaps(DpBox(1f, 1f, 1f, 1f)), false)
    }
}
