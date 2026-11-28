package com.voxa.android.ui

import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PathMeasure
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.PathParser
import uniffi.voxa_ffi.DoodleStroke
import uniffi.voxa_ffi.HomeGreeting
import uniffi.voxa_ffi.StrokeKind
import uniffi.voxa_ffi.StrokeTag
import uniffi.voxa_ffi.doodleFlourish

/** The band the doodles are drawn in (voxa-core's doodle.rs). */
const val BAND_W = 343f
const val BAND_H = 112f

/** One stroke, parsed once: its contours and their lengths, for the reveal along its length. */
private class Inked(val src: DoodleStroke) {
    val path: Path = PathParser.createPathFromPathData(src.d)
    val contours: List<Pair<PathMeasure, Float>>
    val length: Float

    init {
        val list = mutableListOf<Pair<PathMeasure, Float>>()
        val pm = PathMeasure(path, false)
        do {
            val c = PathMeasure()
            // A fresh measure per contour (getSegment works on the measure's current contour).
            val one = Path()
            pm.getSegment(0f, pm.length, one, true)
            c.setPath(one, false)
            if (pm.length > 0f) list += c to pm.length
        } while (pm.nextContour())
        contours = list
        length = list.sumOf { it.second.toDouble() }.toFloat()
    }

    /** The first [p] of the stroke's length, across its contours in order. */
    fun part(p: Float, into: Path) {
        into.reset()
        var left = length * p
        for ((m, len) in contours) {
            if (left <= 0f) break
            m.getSegment(0f, minOf(left, len), into, true)
            left -= len
        }
    }
}

/**
 * Home's doodle (docs/ui-mocks/v7-home-greeting-and-splash.html): a sepia pen line draws the
 * horizon, then the subject, stroke by stroke, then one short flourish; finished by 1.4 s.
 * Plays when [play] turns true, if the core says it should today (once per doodle per day);
 * [still] shows it finished
 * at once (a problem notice is showing, or reduced motion). Decorative: no semantics.
 * [crop] is the part of the band shown, [x0, y0, x1, y1] in band units ([DoodleLayout.crop]); it
 * fills the canvas's width. [horizon] false leaves the long ground line out (the phone's Home).
 */
@Composable
fun DoodleBand(
    g: HomeGreeting, play: Boolean, still: Boolean, onPlayed: () -> Unit, modifier: Modifier = Modifier,
    crop: FloatArray = floatArrayOf(0f, 0f, BAND_W, BAND_H), horizon: Boolean = true,
) {
    val c = vx
    val strokes = remember(g, horizon) { g.strokes.filter { horizon || !it.horizon }.map { Inked(it) } }
    val done = g.doneMs.toFloat()
    // Waiting to play (the splash is up): nothing drawn yet. Not playing: finished.
    val clock = remember(g) { Animatable(if (g.play && !still) 0f else done) }
    LaunchedEffect(g, still, play) {
        if (still || !g.play) {
            clock.snapTo(done)
        } else if (play && clock.value < done) {
            onPlayed()
            clock.animateTo(done, tween(((done - clock.value)).toInt().coerceAtLeast(1), easing = LinearEasing))
        }
    }
    val ink = c.accent.toArgb()
    val paints = remember(ink, c.dark) {
        fun stroke(w: Float, alpha: Float) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            strokeWidth = w
            color = ink
            this.alpha = (alpha * 255).toInt()
        }
        fun fill(alpha: Float) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.FILL
            color = ink
            this.alpha = (alpha * 255).toInt()
        }
        mapOf(
            StrokeKind.INK to stroke(2f, 1f),
            StrokeKind.GROUND to stroke(1.5f, if (c.dark) 0.5f else 0.55f),
            StrokeKind.HATCH to stroke(1.2f, 0.7f),
            StrokeKind.WASH to fill(if (c.dark) 0.16f else 0.12f),
            StrokeKind.DOT to fill(1f),
        )
    }
    val widths = remember { mapOf(StrokeKind.INK to 2f, StrokeKind.GROUND to 1.5f, StrokeKind.HATCH to 1.2f) }
    val part = remember { Path() }
    val m = remember { Matrix() }
    Canvas(modifier) {
        val t = clock.value
        val k = size.width / (crop[2] - crop[0])
        // The pen keeps a pen's weight on a big doodle ([DoodleLayout.strokeScale]).
        val kDp = k / density
        val pen = DoodleLayout.strokeScale(kDp) / kDp
        drawIntoCanvas { cv ->
            val nc = cv.nativeCanvas
            nc.save()
            nc.scale(k, k)
            nc.translate(-crop[0], -crop[1])
            for (s in strokes) {
                val st = s.src.startMs.toFloat()
                val du = s.src.durMs.toFloat()
                if (t < st) continue
                val p = if (du <= 0f) 1f else FastOutSlowInEasing.transform(((t - st) / du).coerceIn(0f, 1f))
                val paint = paints.getValue(s.src.kind)
                val moving = s.src.tag != StrokeTag.NONE && t >= 1000f && t < 1400f
                val f = if (moving) doodleFlourish(g.doodle, s.src, g.scale, t) else null
                if (f != null) {
                    nc.save()
                    val v = f.matrix
                    m.setValues(floatArrayOf(v[0], v[2], v[4], v[1], v[3], v[5], 0f, 0f, 1f))
                    nc.concat(m)
                }
                val base = paint.alpha
                widths[s.src.kind]?.let { paint.strokeWidth = it * pen }
                when (s.src.kind) {
                    StrokeKind.WASH, StrokeKind.DOT -> {
                        paint.alpha = (base * p * (f?.alpha ?: 1f)).toInt()
                        nc.drawPath(s.path, paint)
                    }
                    else -> {
                        paint.alpha = (base * (f?.alpha ?: 1f)).toInt()
                        if (p >= 1f) nc.drawPath(s.path, paint) else {
                            s.part(p, part)
                            nc.drawPath(part, paint)
                        }
                    }
                }
                paint.alpha = base
                if (f != null) nc.restore()
            }
            nc.restore()
        }
    }
}
