package com.voxa.android.ui

import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun TutorialScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    SubpageScaffold(title = "How to use", onBack = onBack) {
        Text(
            text = "Four short steps. Tap the action on each step to jump to the right system screen.",
            fontSize = 13.sp,
            color = VoxaColors.Muted,
            lineHeight = 18.sp,
            modifier = Modifier
                .padding(horizontal = 22.dp)
                .padding(bottom = 14.dp),
        )

        TutorialStep(
            number = 1,
            title = "Enable Voxa keyboard",
            body = "Android needs your permission before any keyboard can run. Tap below, then toggle Voxa on.",
            actionLabel = "Open keyboard settings",
            onAction = {
                context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
            },
        )

        TutorialStep(
            number = 2,
            title = "Switch to Voxa from any text field",
            body = "In any app, tap a text field so a keyboard pops up. Then long-press the space bar and pick Voxa. Or tap the keyboard icon in your navigation bar.",
            actionLabel = "Open keyboard picker",
            onAction = {
                val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                @Suppress("DEPRECATION")
                imm.showInputMethodPicker()
            },
            illustration = { MiniKeyboardIllustration() },
        )

        TutorialStep(
            number = 3,
            title = "Hold the mic to record",
            body = "When Voxa is up, press and hold the big mic button. The waveform shows live audio. Release to stop, or tap Pause to take a breath and resume.",
        )

        TutorialStep(
            number = 4,
            title = "Tap Send to insert text",
            body = "Voxa transcribes your audio through your ChatGPT session and inserts the result into the field you were typing in. No copy-paste needed.",
        )

        Spacer(Modifier.height(28.dp))
    }
}

@Composable
private fun TutorialStep(
    number: Int,
    title: String,
    body: String,
    actionLabel: String? = null,
    onAction: (() -> Unit)? = null,
    illustration: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .padding(bottom = 12.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(VoxaColors.Surface)
            .border(1.dp, VoxaColors.Hair, RoundedCornerShape(14.dp))
            .padding(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(28.dp)
                    .clip(CircleShape)
                    .background(VoxaColors.Primary),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = number.toString(),
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
            Spacer(Modifier.width(12.dp))
            Text(
                text = title,
                fontFamily = DisplayFamily,
                fontWeight = FontWeight.SemiBold,
                fontSize = 15.sp,
                letterSpacing = (-0.2).sp,
                color = VoxaColors.Ink,
            )
        }
        Text(
            text = body,
            fontSize = 12.5.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 18.sp,
            modifier = Modifier.padding(top = 10.dp),
        )
        if (illustration != null) {
            Spacer(Modifier.height(14.dp))
            illustration()
        }
        if (actionLabel != null && onAction != null) {
            Spacer(Modifier.height(12.dp))
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background(VoxaColors.Primary)
                    .clickable { onAction() }
                    .padding(horizontal = 14.dp, vertical = 9.dp),
            ) {
                Text(
                    text = actionLabel,
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
    }
}

@Composable
private fun MiniKeyboardIllustration() {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(VoxaColors.SurfaceAlt)
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        KeyRow(listOf("Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"))
        KeyRow(listOf("A", "S", "D", "F", "G", "H", "J", "K", "L"))
        KeyRow(listOf("⇧", "Z", "X", "C", "V", "B", "N", "M", "⌫"))
        SpacebarRow()
    }
}

@Composable
private fun KeyRow(keys: List<String>) {
    Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        keys.forEach { k ->
            Box(
                modifier = Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(4.dp))
                    .background(VoxaColors.Surface)
                    .border(1.dp, VoxaColors.Hair, RoundedCornerShape(4.dp))
                    .padding(vertical = 7.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(k, fontSize = 10.sp, color = VoxaColors.Ink)
            }
        }
    }
}

@Composable
private fun SpacebarRow() {
    val transition = rememberInfiniteTransition(label = "spacebar-pulse")
    val pulseScale by transition.animateFloat(
        initialValue = 1f,
        targetValue = 1.18f,
        animationSpec = infiniteRepeatable(
            animation = tween(1400, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "scale",
    )
    val pulseAlpha by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0f,
        animationSpec = infiniteRepeatable(
            animation = tween(1400, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "alpha",
    )
    Row(horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(
            modifier = Modifier
                .weight(0.8f)
                .clip(RoundedCornerShape(4.dp))
                .background(VoxaColors.Surface)
                .border(1.dp, VoxaColors.Hair, RoundedCornerShape(4.dp))
                .padding(vertical = 7.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text("123", fontSize = 10.sp, color = VoxaColors.Ink)
        }
        Box(
            modifier = Modifier
                .weight(6f)
                .padding(vertical = 0.dp),
            contentAlignment = Alignment.Center,
        ) {
            // Pulsing ring
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .scale(pulseScale)
                    .alpha(pulseAlpha)
                    .clip(RoundedCornerShape(7.dp))
                    .border(2.dp, Color(0xFFF1D86F), RoundedCornerShape(7.dp))
            )
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(4.dp))
                    .background(Color(0xFFFFF7CD))
                    .border(1.dp, Color(0xFFF1D86F), RoundedCornerShape(4.dp))
                    .padding(vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    "long-press space",
                    fontFamily = MonoFamily,
                    fontSize = 10.sp,
                    color = VoxaColors.Ink,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
        Box(
            modifier = Modifier
                .weight(0.8f)
                .clip(RoundedCornerShape(4.dp))
                .background(VoxaColors.Surface)
                .border(1.dp, VoxaColors.Hair, RoundedCornerShape(4.dp))
                .padding(vertical = 7.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text("↵", fontSize = 10.sp, color = VoxaColors.Ink)
        }
    }
}
