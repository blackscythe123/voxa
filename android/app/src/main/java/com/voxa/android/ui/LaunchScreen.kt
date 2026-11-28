package com.voxa.android.ui

import android.graphics.Paint
import android.graphics.PathMeasure
import android.graphics.RectF
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.requiredWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.graphics.PathParser
import uniffi.voxa_ffi.splashEndFrame
import uniffi.voxa_ffi.splashFrame
import uniffi.voxa_ffi.splashSpec

/** The voice line on the icon (64 x 40), as in the launcher icon and the mockup. */
const val VOICE_LINE = "M2,20H14L18,8L24,34L30,4L36,36L42,14L46,24L50,20H62"

/** Voxa's sepia: the app icon is this colour in both themes. */
val BrandSepia = Color(0xFF7A3F22)

/** The icon's own colours for the dark splash: its tile, an off white ink and its light sepia nib
 *  (docs/ui-mocks/v7-home-greeting-and-splash.html, "Dark mode"). */
private val TileDark = Color(0xFF211B17)
private val InkDark = Color(0xFFF2EDE8)
private val NibDark = Color(0xFFD99366)

/** Splash phases. */
private enum class Phase { Writing, Leaving }

/**
 * Cold start (docs/ui-mocks/v7-home-greeting-and-splash.html, variant B): the nib drifts in, rests
 * on the paper, writes Voxa in one stroke, lifts away, the ink settles and "Talk instead of
 * typing" rises in; then Home crossfades in over 300 ms. A tap anywhere skips to the crossfade.
 * [reduced]: the finished wordmark, still, for 400 ms. [freezeAt] (debug builds, 0 to 1 of the
 * write) holds one frame for screenshots. The timing and the nib's path come from voxa-core.
 */
@Composable
fun LaunchOverlay(freezeAt: Float?, reduced: Boolean, onDone: () -> Unit) {
    val c = vx
    val spec = remember { splashSpec() }
    val total = spec.totalMs
    val clock = remember { Animatable(if (reduced) total else (freezeAt ?: 0f) * total) }
    val out = remember { Animatable(0f) }
    var phase by remember { mutableStateOf(Phase.Writing) }
    LaunchedEffect(phase) {
        if (freezeAt != null) return@LaunchedEffect
        if (phase == Phase.Writing) {
            if (reduced) kotlinx.coroutines.delay(spec.reducedHoldMs.toLong())
            else clock.animateTo(total, tween((total - clock.value).toInt().coerceAtLeast(1), easing = LinearEasing))
            phase = Phase.Leaving
        } else {
            out.animateTo(1f, tween(spec.crossfadeMs.toInt(), easing = FastOutSlowInEasing))
            onDone()
        }
    }
    val dark = c.dark
    val bg = if (dark) TileDark else c.base
    val ink = if (dark) InkDark else c.text
    val nib = if (dark) NibDark else c.accent
    val desc = if (dark) InkDark.copy(alpha = 0.74f) else c.text72
    val word = remember { PathParser.createPathFromPathData(spec.wordmark) }
    val wordLen = remember { PathMeasure(word, false).length }
    val nibPath = remember { PathParser.createPathFromPathData(spec.nib) }
    val slit = remember { PathParser.createPathFromPathData(spec.nibSlit) }
    val part = remember { android.graphics.Path() }
    val inkPaint = remember { Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND } }
    val fill = remember { Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL } }
    val line = remember { Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND; strokeWidth = 2.2f } }
    val f = if (reduced) splashEndFrame() else splashFrame(clock.value)

    Box(
        Modifier.fillMaxSize().graphicsLayer { alpha = 1f - out.value }.background(bg)
            .clearAndSetSemantics {}
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {
                if (freezeAt == null && phase == Phase.Writing) phase = Phase.Leaving
            },
    ) {
        Box(Modifier.align(Alignment.Center).size(280.dp, 140.dp)) {
            Canvas(Modifier.size(280.dp, 140.dp)) {
                val k = size.width / 280f
                drawIntoCanvas { cv ->
                    val nc = cv.nativeCanvas
                    nc.save()
                    nc.scale(k, k)
                    if (f.inkAlpha > 0f) {
                        inkPaint.color = ink.toArgb()
                        inkPaint.strokeWidth = f.inkWidth
                        if (f.ink >= 1f) nc.drawPath(word, inkPaint) else {
                            part.reset()
                            PathMeasure(word, false).getSegment(0f, wordLen * f.ink, part, true)
                            nc.drawPath(part, inkPaint)
                        }
                    }
                    if (f.nibAlpha > 0.001f) {
                        nc.save()
                        nc.translate(f.nibX, f.nibY)
                        nc.rotate(f.nibRot)
                        nc.scale(f.nibScale * 0.9f, f.nibScale * 0.9f)
                        nc.saveLayerAlpha(RectF(-20f, -60f, 20f, 4f), (f.nibAlpha * 255).toInt())
                        fill.color = nib.toArgb()
                        nc.drawPath(nibPath, fill)
                        fill.color = bg.toArgb()
                        nc.drawCircle(0f, -32f, 4.6f, fill)
                        nc.drawRect(-16f, -44f, 16f, -41.8f, fill)
                        line.color = bg.toArgb()
                        nc.drawPath(slit, line)
                        nc.restore()
                        nc.restore()
                    }
                    nc.restore()
                }
            }
            Text(
                spec.tagline,
                Modifier.requiredWidth(360.dp).align(Alignment.TopCenter).offset(y = 148.dp + f.descDy.dp).graphicsLayer { alpha = f.descAlpha },
                style = Type.M, color = desc, textAlign = TextAlign.Center,
            )
        }
    }
}
