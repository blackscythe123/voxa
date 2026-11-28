package com.voxa.android.ui

import android.graphics.Paint
import android.graphics.PathMeasure
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp
import androidx.core.graphics.PathParser

/** The voice line on the icon (64 x 40), as in the launcher icon and the mockup. */
const val VOICE_LINE = "M2,20H14L18,8L24,34L30,4L36,36L42,14L46,24L50,20H62"

/** Voxa's sepia: the app icon is this colour in both themes. */
val BrandSepia = Color(0xFF8F4C2C)

private const val TOTAL_MS = 950f

/**
 * Cold start (docs/ui-mocks/v6-app-redesign.html, "Opening the app"): the icon pops in, the white
 * voice line draws itself across it, "Voxa" fades up below, then the whole thing gives way to the
 * app. Under a second. [freezeAt] (debug builds) holds one frame for screenshots.
 */
@Composable
fun LaunchOverlay(freezeAt: Float?, onDone: () -> Unit) {
    val c = vx
    val p = remember { Animatable(freezeAt ?: 0f) }
    LaunchedEffect(Unit) {
        if (freezeAt == null) {
            p.animateTo(1f, tween(TOTAL_MS.toInt(), easing = LinearEasing))
            onDone()
        }
    }
    val ms = p.value * TOTAL_MS
    fun seg(a: Float, b: Float) = ((ms - a) / (b - a)).coerceIn(0f, 1f)
    fun easeOut(x: Float) = 1f - (1f - x) * (1f - x) * (1f - x)
    val pop = easeOut(seg(0f, 160f))
    val draw = easeOut(seg(120f, 480f))
    val name = easeOut(seg(380f, 620f))
    val out = seg(820f, 950f)
    val line = remember { PathParser.createPathFromPathData(VOICE_LINE) }
    val length = remember { PathMeasure(line, false).length }
    val paint = remember {
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            strokeWidth = 3.2f
            color = Color.White.toArgb()
        }
    }

    Box(Modifier.fillMaxSize().graphicsLayer { alpha = 1f - out }.background(c.base)) {
        val shape = RoundedCornerShape(28.dp)
        Box(
            Modifier.align(Alignment.Center)
                .graphicsLayer { scaleX = 0.86f + 0.14f * pop; scaleY = scaleX; alpha = pop }
                .size(96.dp)
                .shadow(20.dp, shape, ambientColor = BrandSepia.copy(alpha = 0.3f), spotColor = BrandSepia.copy(alpha = 0.6f))
                .clip(shape).background(BrandSepia),
            contentAlignment = Alignment.Center,
        ) {
            Canvas(Modifier.size(64.dp, 40.dp)) {
                if (draw <= 0f) return@Canvas
                val part = android.graphics.Path()
                PathMeasure(line, false).getSegment(0f, length * draw, part, true)
                drawIntoCanvas {
                    val nc = it.nativeCanvas
                    nc.save()
                    nc.scale(size.width / 64f, size.height / 40f)
                    nc.drawPath(part, paint)
                    nc.restore()
                }
            }
        }
        Text(
            "Voxa",
            Modifier.align(Alignment.Center).offset(y = 48.dp + 40.dp + 14.dp + 8.dp * (1f - name))
                .graphicsLayer { alpha = name },
            style = Type.L, color = c.text,
        )
    }
}
