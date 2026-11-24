package com.voxa.android.ime

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.os.SystemClock
import android.view.View
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

/**
 * The desktop strip's ink line (desktop/app/src/main.rs `ink_line`): a quill glides across every
 * 4 s, leaving a seismograph trace. No voice, flat line; speech lifts it. One tick per 40 ms.
 */
class InkLineView(context: Context) : View(context) {
    private val ink = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = dp(1.6f)
        strokeJoin = Paint.Join.ROUND
        strokeCap = Paint.Cap.ROUND
        color = INK
        alpha = 230
    }
    private val pen = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = PEN; style = Paint.Style.FILL }
    private val levels = ArrayList<Float>(CYCLE + 1)
    private var latest = 0f
    private var running = false
    private var nextTickAt = 0L

    private val ticker = object : Runnable {
        override fun run() {
            if (!running) return
            val lvl = maxOf(latest, (levels.lastOrNull() ?: 0f) * 0.88f)
            if (levels.size >= CYCLE) levels.clear()
            levels.add(lvl)
            invalidate()
            nextTickAt += TICK_MS
            postDelayed(this, maxOf(1L, nextTickAt - SystemClock.uptimeMillis()))
        }
    }

    /** The newest mic level, 0..1 (the core's meter scale). */
    fun level(l: Float) {
        latest = l
    }

    fun start() {
        if (running) return
        running = true
        levels.clear()
        latest = 0f
        nextTickAt = SystemClock.uptimeMillis()
        post(ticker)
    }

    fun stop() {
        running = false
        removeCallbacks(ticker)
    }

    override fun onDetachedFromWindow() {
        stop()
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val base = height * 0.62f
        val n = CYCLE.toFloat()
        val tick = (levels.size - 1).coerceAtLeast(0)
        val amp = { l: Float -> if (l < GATE) 0f else ((l - GATE) / (1f - GATE)).coerceAtMost(1f).pow(0.6f) * dp(11f) }
        fun at(k: Int): Pair<Float, Float> {
            val l = levels.getOrElse(k) { 0f }
            val jitter = 0.55f + 0.45f * (((k * 7919) % 97) / 96f)
            val dir = if (k % 2 == 0) 1f else -1f
            return Pair(k / n * w, base - dir * jitter * amp(l))
        }
        val pNow = tick / n
        val alpha = min(min(pNow / 0.06f, 1f), (1f - pNow) / 0.08f).coerceIn(0f, 1f)
        if (tick >= 1) {
            val path = Path()
            val (x0, y0) = at(0)
            path.moveTo(x0, y0)
            for (k in 1..tick) {
                val (x, y) = at(k)
                path.lineTo(x, y)
            }
            ink.alpha = (230 * alpha).toInt()
            canvas.drawPath(path, ink)
        }
        // The quill, nib on the head of the line, tilting with the voice.
        val (hx, hy) = at(tick)
        val lvl = levels.getOrElse(tick) { 0f }
        val ang = Math.toRadians((-12.0 - 5.0 * lvl)).toFloat()
        val sc = dp(1.45f)
        fun q(x: Float, y: Float): Pair<Float, Float> {
            val dx = (x - NIB_X) * sc
            val dy = (y - NIB_Y) * sc
            return Pair(hx + dx * cos(ang) - dy * sin(ang), hy + dx * sin(ang) + dy * cos(ang))
        }
        val body = Path()
        fun Path.m(p: Pair<Float, Float>) = moveTo(p.first, p.second)
        fun Path.l(p: Pair<Float, Float>) = lineTo(p.first, p.second)
        fun Path.c(a: Pair<Float, Float>, b: Pair<Float, Float>, e: Pair<Float, Float>) =
            cubicTo(a.first, a.second, b.first, b.second, e.first, e.second)
        body.m(q(12.5f, 1.5f))
        body.c(q(5.5f, 13f), q(9f, 4f), q(6.5f, 8f))
        body.l(q(4.5f, 14.5f))
        body.l(q(4f, 15f))
        body.c(q(9.5f, 11f), q(6f, 14.5f), q(8f, 13f))
        body.c(q(12.5f, 1.5f), q(11.5f, 8.5f), q(13f, 5f))
        body.close()
        body.m(q(4f, 15f))
        body.l(q(2f, 15.6f))
        body.l(q(3f, 14f))
        body.close()
        pen.alpha = (255 * alpha).toInt()
        canvas.drawPath(body, pen)
    }

    private fun dp(v: Float) = v * resources.displayMetrics.density

    companion object {
        const val CYCLE = 100
        const val TICK_MS = 40L
        const val GATE = 0.14f
        const val NIB_X = 3f
        const val NIB_Y = 15.2f
        val INK = Color.parseColor("#8F4C2C")
        val PEN = Color.parseColor("#2D2418")
    }
}
