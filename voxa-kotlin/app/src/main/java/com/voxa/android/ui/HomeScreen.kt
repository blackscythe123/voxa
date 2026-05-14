package com.voxa.android.ui

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.compose.animation.core.*
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.voxa.android.VoxaApp
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun HomeScreen(onLogout: () -> Unit) {
    val context = LocalContext.current
    val prefs = VoxaApp.prefs

    var maxDuration by remember { mutableIntStateOf(prefs.getMaxDuration()) }
    var autoStart by remember { mutableStateOf(prefs.getAutoStart()) }
    var sessionValid by remember { mutableStateOf(true) }
    var showDurationDialog by remember { mutableStateOf(false) }

    val imeEnabled = remember { mutableStateOf(false) }

    fun checkImeEnabled() {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imeEnabled.value = imm.enabledInputMethodList.any {
            it.packageName == context.packageName
        }
    }

    LaunchedEffect(Unit) {
        sessionValid = TranscriptionApi.validateSession()
        checkImeEnabled()
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(VoxaColors.Bg)
            .systemBarsPadding()
            .verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {

        // ── Header ────────────────────────────────────────────────────────────
        Spacer(Modifier.height(40.dp))
        Text(
            text = "voxa",
            fontSize = 48.sp,
            fontWeight = FontWeight.Bold,
            fontStyle = FontStyle.Italic,
            color = VoxaColors.AccentAmber,
            letterSpacing = 4.sp,
        )
        Text(
            text = "VOICE · TRANSCRIBED",
            fontSize = 11.sp,
            color = VoxaColors.TextMuted,
            letterSpacing = 3.sp,
        )

        // ── IME Status Card ───────────────────────────────────────────────────
        Spacer(Modifier.height(32.dp))
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    modifier = Modifier
                        .size(44.dp)
                        .background(
                            if (imeEnabled.value) VoxaColors.AccentGlow else VoxaColors.Surface,
                            CircleShape
                        )
                        .border(1.dp, VoxaColors.Border, CircleShape),
                    contentAlignment = Alignment.Center
                ) {
                    Text(text = if (imeEnabled.value) "✓" else "🎤", fontSize = 18.sp,
                        color = if (imeEnabled.value) VoxaColors.AccentAmber else VoxaColors.TextMuted)
                }
                Spacer(Modifier.width(14.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = if (imeEnabled.value) "Voxa keyboard active" else "Voxa keyboard not enabled",
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Medium,
                        color = VoxaColors.TextPrimary,
                    )
                    Text(
                        text = if (imeEnabled.value) "Switch keyboards in any text field to use it"
                               else "Tap to enable in Settings",
                        fontSize = 12.sp,
                        color = VoxaColors.TextMuted,
                        modifier = Modifier.padding(top = 2.dp),
                    )
                }
                if (!imeEnabled.value) {
                    TextButton(onClick = {
                        context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                    }) {
                        Text("Enable", color = VoxaColors.AccentAmber, fontSize = 13.sp)
                    }
                }
            }
            if (imeEnabled.value) {
                Spacer(Modifier.height(12.dp))
                TextButton(
                    onClick = {
                        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                        @Suppress("DEPRECATION")
                        imm.showInputMethodPicker()
                    },
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text("Switch keyboard in current app", color = VoxaColors.AccentAmber, fontSize = 13.sp)
                }
            }
        }

        // ── How it works ──────────────────────────────────────────────────────
        Spacer(Modifier.height(24.dp))
        SectionHeader(title = "How it works")
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            listOf(
                "1" to "Tap Enable above → turn on Voxa keyboard in Settings",
                "2" to "Open any app and tap a text field",
                "3" to "Tap the globe/keyboard icon to switch to Voxa",
                "4" to "Tap 🎤 to start, tap again to stop — text inserts automatically",
            ).forEachIndexed { i, (n, t) ->
                if (i > 0) Spacer(Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.Top) {
                    Box(
                        modifier = Modifier
                            .size(24.dp)
                            .background(VoxaColors.Surface, CircleShape)
                            .border(1.dp, VoxaColors.Border, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(text = n, fontSize = 11.sp, color = VoxaColors.AccentAmber,
                            fontWeight = FontWeight.Medium)
                    }
                    Spacer(Modifier.width(12.dp))
                    Text(
                        text = t, fontSize = 14.sp, color = VoxaColors.TextMuted,
                        lineHeight = 20.sp, modifier = Modifier.weight(1f),
                    )
                }
            }
        }

        // ── Settings ──────────────────────────────────────────────────────────
        Spacer(Modifier.height(24.dp))
        SectionHeader(title = "Settings")
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            SettingsRow(
                label = "Max recording duration",
                sublabel = "${maxDuration} seconds",
                trailingText = "${maxDuration}s ›",
                onClick = { showDurationDialog = true },
            )
        }

        // ── Account ───────────────────────────────────────────────────────────
        Spacer(Modifier.height(24.dp))
        SectionHeader(title = "Account")
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            SettingsRow(
                label = "ChatGPT session",
                sublabel = if (sessionValid) "Connected" else "Session expired",
                trailingText = if (sessionValid) "✓" else "!",
            )
            Divider()
            SettingsRow(
                label = "Sign out",
                sublabel = "Clear session and re-login",
                trailingText = "›",
                destructive = true,
                onClick = onLogout,
            )
        }

        // ── Footer ────────────────────────────────────────────────────────────
        Spacer(Modifier.height(32.dp))
        Text(
            text = "voxa · powered by Whisper",
            fontSize = 11.sp,
            color = VoxaColors.TextDim,
            letterSpacing = 1.sp,
        )
        Spacer(Modifier.height(48.dp))
    }

    if (showDurationDialog) {
        AlertDialog(
            onDismissRequest = { showDurationDialog = false },
            containerColor = VoxaColors.Card,
            title = { Text("Max duration", color = VoxaColors.TextPrimary) },
            text = {
                Column {
                    listOf(60, 120, 180).forEach { sec ->
                        TextButton(onClick = {
                            maxDuration = sec
                            prefs.setMaxDuration(sec)
                            showDurationDialog = false
                        }) {
                            Text("${sec}s", color = VoxaColors.AccentAmber)
                        }
                    }
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = { showDurationDialog = false }) {
                    Text("Cancel", color = VoxaColors.TextMuted)
                }
            },
        )
    }
}

// ── Reusable components ───────────────────────────────────────────────────────

@Composable
private fun SectionHeader(title: String) {
    Text(
        text = title.uppercase(),
        fontSize = 11.sp,
        color = VoxaColors.TextMuted,
        letterSpacing = 1.5.sp,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 28.dp)
            .padding(bottom = 8.dp),
    )
}

@Composable
private fun SectionCard(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(VoxaColors.Card)
            .border(1.dp, VoxaColors.Border, RoundedCornerShape(14.dp))
            .padding(16.dp),
        content = content,
    )
}

@Composable
private fun Divider() {
    HorizontalDivider(
        modifier = Modifier.padding(vertical = 4.dp),
        color = VoxaColors.Border,
    )
}

@Composable
private fun SettingsRow(
    label: String,
    sublabel: String? = null,
    checked: Boolean? = null,
    onCheckedChange: ((Boolean) -> Unit)? = null,
    trailingText: String? = null,
    destructive: Boolean = false,
    onClick: (() -> Unit)? = null,
) {
    val rowColor = if (destructive) VoxaColors.RecordingRed else VoxaColors.TextPrimary
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable { onClick() } else Modifier)
            .padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(label, fontSize = 15.sp, fontWeight = FontWeight.Medium, color = rowColor)
            if (sublabel != null) {
                Text(sublabel, fontSize = 12.sp, color = VoxaColors.TextMuted,
                    modifier = Modifier.padding(top = 2.dp))
            }
        }
        if (checked != null && onCheckedChange != null) {
            Switch(
                checked = checked,
                onCheckedChange = onCheckedChange,
                colors = SwitchDefaults.colors(
                    checkedTrackColor = VoxaColors.AccentAmber,
                    uncheckedTrackColor = VoxaColors.Border,
                    checkedThumbColor = VoxaColors.AccentBright,
                    uncheckedThumbColor = VoxaColors.TextDim,
                ),
            )
        } else if (trailingText != null) {
            Text(trailingText, fontSize = 14.sp,
                color = if (destructive) VoxaColors.RecordingRed else VoxaColors.TextMuted)
        }
    }
}

@Composable
fun WaveBars(
    active: Boolean,
    color: Color,
    barCount: Int = 5,
    heightDp: Int = 20,
) {
    val infiniteTransition = rememberInfiniteTransition(label = "wave")
    Row(
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.height(heightDp.dp),
    ) {
        repeat(barCount) { i ->
            val scale by if (active) {
                infiniteTransition.animateFloat(
                    initialValue = 0.2f,
                    targetValue = 1f,
                    animationSpec = infiniteRepeatable(
                        animation = tween(300 + i * 60, easing = FastOutSlowInEasing),
                        repeatMode = RepeatMode.Reverse,
                        initialStartOffset = StartOffset(i * 80),
                    ),
                    label = "bar$i",
                )
            } else {
                remember { mutableFloatStateOf(0.2f) }
            }
            Box(
                modifier = Modifier
                    .width(3.dp)
                    .fillMaxHeight(scale)
                    .clip(RoundedCornerShape(2.dp))
                    .background(color)
            )
        }
    }
}