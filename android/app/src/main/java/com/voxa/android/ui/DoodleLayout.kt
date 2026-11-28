package com.voxa.android.ui

/** A rectangle in dp, from its top left corner. */
data class DpBox(val x: Float, val y: Float, val w: Float, val h: Float) {
    val right get() = x + w
    val bottom get() = y + h
    fun overlaps(o: DpBox) = x < o.right && o.x < right && y < o.bottom && o.y < bottom
}

/**
 * Where the phone's Home doodle goes (docs/ui-mocks/v7-home-fullscreen-compare.html, "Phone, new",
 * doodle above, bigger; owner's locked layout). Pure math, so it is tested without a screen.
 *
 * The doodle is cropped to its own drawing (voxa-core's `drawn_bounds`, no horizon) plus [PAD],
 * then scaled to fill the width up to [MAX_H] tall, keeping its shape, and centred. The avatar's
 * corner stays clear: a doodle that would run under it is either narrowed (still centred) or moved
 * below the avatar row, whichever keeps it bigger, unless narrowing costs less than a quarter.
 */
object DoodleLayout {
    /** Band units of air around the drawing, as in the mockup. */
    const val PAD = 4f
    const val MAX_H = 150f
    /** The avatar at the top right of the content: 40 dp, 4 dp down. */
    const val AVATAR = 40f
    const val AVATAR_TOP = 4f
    /** Air the doodle keeps from the avatar. */
    const val AVATAR_MARGIN = 8f
    /** Narrowing is kept when the doodle is at least this much of its height below the avatar. */
    const val KEEP_NARROW = 0.75f

    /** From the doodle's bottom to the greeting: the greeting sits right under it. */
    const val GREETING_GAP = 10f

    /** How tall the doodle's row is: down to the doodle's bottom (or the avatar's, with no doodle
     *  yet). No fixed band, so no empty space under a short doodle. */
    fun rowHeight(width: Float, doodle: DpBox?): Float = maxOf(doodle?.bottom ?: 0f, avatar(width).bottom)

    /** The crop [x0, y0, x1, y1] in band units for the core's drawn bounds. */
    fun crop(bounds: List<Float>): FloatArray {
        val b = if (bounds.size == 4) bounds else listOf(0f, 0f, BAND_W, BAND_H)
        return floatArrayOf(b[0] - PAD, b[1] - PAD, b[2] + PAD, b[3] + PAD)
    }

    /** The avatar in a header [width] dp wide. */
    fun avatar(width: Float) = DpBox(width - AVATAR, AVATAR_TOP, AVATAR, AVATAR)

    /** The corner the doodle never enters: the avatar plus [AVATAR_MARGIN] left and below. */
    fun safeCorner(width: Float) = DpBox(width - AVATAR - AVATAR_MARGIN, 0f, AVATAR + AVATAR_MARGIN, AVATAR_TOP + AVATAR + AVATAR_MARGIN)

    /** The doodle's box in a header [width] dp wide, for a crop [cropW] x [cropH] band units. */
    fun place(width: Float, cropW: Float, cropH: Float): DpBox {
        val ar = cropW / cropH
        fun fit(maxW: Float, top: Float): DpBox {
            val h = minOf(MAX_H, maxW / ar)
            val w = h * ar
            return DpBox((width - w) / 2f, top, w, h)
        }
        val safe = safeCorner(width)
        val full = fit(width, 0f)
        if (!full.overlaps(safe)) return full
        // Centred with its right edge left of the corner.
        val narrow = fit(2f * safe.x - width, 0f)
        val below = fit(width, safe.bottom)
        return if (narrow.w > 0f && narrow.h >= KEEP_NARROW * below.h) narrow else below
    }

    /** How thick the pen draws when the doodle is [k] times its band size: with the drawing up to
     *  the band, then with the square root, so a big doodle keeps a pen line (as the desktop). */
    fun strokeScale(k: Float): Float = if (k <= 1f) k else kotlin.math.sqrt(k)
}
