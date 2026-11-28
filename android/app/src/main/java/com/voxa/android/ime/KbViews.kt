package com.voxa.android.ime

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.Drawable
import android.view.View
import kotlin.math.abs
import kotlin.math.pow

/**
 * A round button with a soft ring and a tinted glow (the big mic, and the green check when the
 * words land). Drawn by hand so it costs one view and no bitmaps.
 */
class RoundView(context: Context, private val diameterDp: Float, private val ringDp: Float) : View(context) {
    private val d = resources.displayMetrics.density
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val ring = Paint(Paint.ANTI_ALIAS_FLAG)
    private var glowOffsetY = 0f
    var icon: Drawable? = null
        set(v) {
            field = v
            invalidate()
        }
    var iconSizeDp = 32f

    /** [glow]: the colour of the soft shadow/glow around it; [offsetYdp] moves it down (a shadow). */
    fun style(color: Int, ringColor: Int, glow: Int, glowRadiusDp: Float, offsetYdp: Float, iconTint: Int) {
        fill.color = color
        ring.color = ringColor
        glowOffsetY = offsetYdp * d
        fill.setShadowLayer(glowRadiusDp * d, 0f, glowOffsetY, glow)
        icon?.setTint(iconTint)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val cx = width / 2f
        val cy = height / 2f
        val r = diameterDp * d / 2f
        if (ringDp > 0f) canvas.drawCircle(cx, cy, r + ringDp * d, ring)
        canvas.drawCircle(cx, cy, r, fill)
        icon?.let {
            val h = (iconSizeDp * d / 2f).toInt()
            it.setBounds((cx - h).toInt(), (cy - h).toInt(), (cx + h).toInt(), (cy + h).toInt())
            it.draw(canvas)
        }
        if (isPressed) {
            canvas.drawCircle(cx, cy, r, pressPaint)
        }
    }

    private val pressPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0x26000000 }

    override fun drawableStateChanged() {
        super.drawableStateChanged()
        invalidate()
    }

    /** Taps count only on the circle and its ring, not the glow around it. */
    fun hit(x: Float, y: Float): Boolean {
        val r = diameterDp * d / 2f + ringDp * d
        return abs(x - width / 2f) <= r && abs(y - height / 2f) <= r
    }
}

/**
 * The live level bars behind the mic while listening: the newest level enters at the mic and moves
 * outward on both sides, so speaking makes the bars swell around it. One update per 40 ms chunk.
 */
class LevelBarsView(context: Context) : View(context) {
    private val d = resources.displayMetrics.density
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val levels = FloatArray(BARS_PER_SIDE)
    private val rect = RectF()
    /** Half the width kept clear in the middle (behind the mic). */
    var clearHalfDp = 60f

    fun color(c: Int) {
        paint.color = c
        invalidate()
    }

    fun reset() {
        levels.fill(0f)
        invalidate()
    }

    /** The newest mic level, 0..1 (the core's meter scale). */
    fun level(l: Float) {
        System.arraycopy(levels, 0, levels, 1, levels.size - 1)
        levels[0] = maxOf(l, levels[1] * 0.75f)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        val gap = 4 * d
        val clear = clearHalfDp * d
        val side = (w / 2f - clear - 8 * d).coerceAtLeast(0f)
        val bw = ((side - gap * (BARS_PER_SIDE - 1)) / BARS_PER_SIDE).coerceAtLeast(2 * d)
        val radius = 3 * d
        for (i in 0 until BARS_PER_SIDE) {
            val l = levels[i]
            val amp = if (l < GATE) 0f else ((l - GATE) / (1f - GATE)).coerceAtMost(1f).pow(0.7f)
            val bh = maxOf(4 * d, amp * h)
            val top = (h - bh) / 2f
            val off = clear + i * (bw + gap)
            rect.set(w / 2f + off, top, w / 2f + off + bw, top + bh)
            canvas.drawRoundRect(rect, radius, radius, paint)
            rect.set(w / 2f - off - bw, top, w / 2f - off, top + bh)
            canvas.drawRoundRect(rect, radius, radius, paint)
        }
    }

    companion object {
        const val BARS_PER_SIDE = 8
        const val GATE = 0.12f
    }
}
