package com.voxa.android.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.VoxaColors
import kotlin.math.abs
import kotlin.random.Random
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val BAR_COUNT = 9
// Scaled up so the bar V matches OXA cap-height.
private val V_SHAPE = floatArrayOf(64f, 48f, 34f, 22f, 10f, 22f, 34f, 48f, 64f)
private const val BAR_WIDTH_DP = 7f
private const val BAR_SPACING_DP = 10f
private const val CANVAS_W_DP = 100f
private const val CANVAS_H_DP = 96f

@Composable
fun SplashScreen(onFinished: () -> Unit) {
    val heights = remember { List(BAR_COUNT) { Animatable(6f) } }
    val centerOffsets = remember { List(BAR_COUNT) { Animatable(0f) } }
    val showOxa = remember { mutableStateOf(false) }
    val showTagline = remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        val rnd = Random(System.currentTimeMillis())
        val easeOutExpo = CubicBezierEasing(0.16f, 1f, 0.3f, 1f)

        // Stage 1 — live wave (5 amplitude frames)
        repeat(5) {
            for (i in 0 until BAR_COUNT) {
                val base = 14f + rnd.nextFloat() * 30f
                val centerBoost = (1f - abs(i - (BAR_COUNT - 1) / 2f) / ((BAR_COUNT - 1) / 2f)) * 12f
                heights[i].snapTo(base + centerBoost)
                centerOffsets[i].snapTo(0f)
            }
            delay(170L)
        }

        // Stage 2 — bars settle into V shape, bottoms aligned to the canvas bottom edge
        // so the V's bottom matches OXA's baseline (Row uses Alignment.Bottom).
        coroutineScope {
            // Canvas axisY = CANVAS_H_DP / 2. We want bar bottoms at y = CANVAS_H_DP, i.e. center offset = CANVAS_H_DP/2 - h/2.
            val barBottomOffset = CANVAS_H_DP / 2f
            for (i in 0 until BAR_COUNT) {
                launch { heights[i].animateTo(V_SHAPE[i], tween(520, easing = easeOutExpo)) }
                launch {
                    centerOffsets[i].animateTo(
                        targetValue = barBottomOffset - V_SHAPE[i] / 2f,
                        animationSpec = tween(520, easing = easeOutExpo),
                    )
                }
            }
        }
        delay(200L)

        // Stage 3 + 4 — Row reflows: OXA expands in from 0 width, wave appears to slide left to keep the group centered
        showOxa.value = true
        delay(600L)

        // Stage 5 — tagline
        showTagline.value = true
        delay(700L)

        onFinished()
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(VoxaColors.Surface),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            // Lockup row — Row centers its children. As OXA expands in, the Row width grows and re-centers,
            // making the wave appear to slide left organically. No manual offset math.
            Row(
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.Center,
                modifier = Modifier.height(110.dp),
            ) {
                val density = LocalDensity.current.density
                Canvas(modifier = Modifier.size(width = CANVAS_W_DP.dp, height = CANVAS_H_DP.dp)) {
                    drawBars(heights, centerOffsets, density)
                }

                AnimatedVisibility(
                    visible = showOxa.value,
                    enter = expandHorizontally(tween(580, easing = CubicBezierEasing(0.16f, 1f, 0.3f, 1f))) +
                            fadeIn(tween(420, delayMillis = 160)),
                ) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Spacer(Modifier.width(4.dp))
                        Text(
                            text = "OXA",
                            color = VoxaColors.Ink,
                            fontFamily = DisplayFamily,
                            fontWeight = FontWeight.ExtraBold,
                            fontSize = 56.sp,
                            letterSpacing = (-2.5).sp,
                            style = TextStyle(
                                platformStyle = PlatformTextStyle(includeFontPadding = false),
                                lineHeightStyle = LineHeightStyle(
                                    alignment = LineHeightStyle.Alignment.Bottom,
                                    trim = LineHeightStyle.Trim.Both,
                                ),
                            ),
                        )
                    }
                }
            }

            Spacer(Modifier.height(20.dp))

            AnimatedVisibility(
                visible = showTagline.value,
                enter = fadeIn(tween(320)),
            ) {
                Text(
                    text = "dictate anything from anywhere",
                    color = VoxaColors.Muted,
                    fontFamily = DisplayFamily,
                    fontSize = 14.sp,
                )
            }
        }
    }
}

private fun DrawScope.drawBars(
    heights: List<Animatable<Float, *>>,
    centerOffsets: List<Animatable<Float, *>>,
    densityScale: Float,
) {
    val barWidth = BAR_WIDTH_DP * densityScale
    val spacing = BAR_SPACING_DP * densityScale
    val totalWidth = (BAR_COUNT - 1) * spacing
    val startX = size.width / 2f - totalWidth / 2f
    val axisY = size.height / 2f

    for (i in 0 until BAR_COUNT) {
        val h = heights[i].value * densityScale
        val centerYOffset = centerOffsets[i].value * densityScale
        val cx = startX + i * spacing
        val cy = axisY + centerYOffset
        drawRoundRect(
            color = VoxaColors.Ink,
            topLeft = Offset(cx - barWidth / 2f, cy - h / 2f),
            size = Size(barWidth, h.coerceAtLeast(2f)),
            cornerRadius = CornerRadius(barWidth / 2f),
        )
    }
}
