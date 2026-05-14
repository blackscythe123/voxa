package com.voxa.android.ui

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.ui.draw.scale
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
import com.voxa.android.service.OverlayService
import com.voxa.android.ui.theme.VoxaColors
import kotlinx.coroutines.launch

private enum class RecorderState { Idle, Recording, Processing, Error }

@Composable
fun HomeScreen(onLogout: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val prefs = VoxaApp.prefs

    var overlayActive by remember { mutableStateOf(false) }
    var recorderState by remember { mutableStateOf(RecorderState.Idle) }
    var lastTranscript by remember { mutableStateOf("") }
    var maxDuration by remember { mutableIntStateOf(prefs.getMaxDuration()) }
    var autoStart by remember { mutableStateOf(prefs.getAutoStart()) }
    var sessionValid by remember { mutableStateOf(true) }
    var showDurationDialog by remember { mutableStateOf(false) }

    // Orb pulse animation when recording
    val infiniteTransition = rememberInfiniteTransition(label = "orb")
    val pulseScale by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = if (recorderState == RecorderState.Recording) 1.12f else 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(600, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "pulse",
    )
    val glowAlpha by animateFloatAsState(
        targetValue = if (overlayActive) 1f else 0f,
        animationSpec = spring(),
        label = "glow",
    )

    // Load settings and session validity
    LaunchedEffect(Unit) {
        sessionValid = TranscriptionApi.validateSession()
    }

    // Listen for transcripts from OverlayService
    DisposableEffect(Unit) {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(ctx: Context, intent: Intent) {
                val text = intent.getStringExtra(VoxaApp.EXTRA_TRANSCRIPT) ?: return
                lastTranscript = text
            }
        }
        val filter = IntentFilter(VoxaApp.ACTION_TRANSCRIPT)
        ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
        onDispose { context.unregisterReceiver(receiver) }
    }

    val micPermLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (!granted) return@rememberLauncherForActivityResult
        // Start service after permission granted
        OverlayService.start(context)
        overlayActive = true
    }

    fun checkOverlayPermission(): Boolean = Settings.canDrawOverlays(context)

    fun toggleOverlay() {
        if (overlayActive) {
            OverlayService.stop(context)
            overlayActive = false
        } else {
            if (!checkOverlayPermission()) {
                val intent = Intent(
                    Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                    Uri.parse("package:${context.packageName}")
                )
                context.startActivity(intent)
                return
            }
            val hasMic = ContextCompat.checkSelfPermission(
                context, Manifest.permission.RECORD_AUDIO
            ) == PackageManager.PERMISSION_GRANTED
            if (!hasMic) {
                micPermLauncher.launch(Manifest.permission.RECORD_AUDIO)
            } else {
                OverlayService.start(context)
                overlayActive = true
            }
        }
    }

    val stateColor = when (recorderState) {
        RecorderState.Idle -> VoxaColors.AccentAmber
        RecorderState.Recording -> VoxaColors.RecordingRed
        RecorderState.Processing -> VoxaColors.AccentBright
        RecorderState.Error -> VoxaColors.RecordingRed
    }

    val stateLabel = when (recorderState) {
        RecorderState.Idle -> if (overlayActive) "Overlay active — tap mic to record" else "Overlay off"
        RecorderState.Recording -> "Recording… tap mic to stop"
        RecorderState.Processing -> "Transcribing…"
        RecorderState.Error -> "Error — try again"
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

        // ── Central Orb ───────────────────────────────────────────────────────
        Spacer(Modifier.height(40.dp))
        Box(contentAlignment = Alignment.Center) {
            // Glow ring
            Box(
                modifier = Modifier
                    .size(200.dp)
                    .background(
                        color = if (recorderState == RecorderState.Recording)
                            VoxaColors.RecordingGlow.copy(alpha = 0.22f * glowAlpha)
                        else
                            VoxaColors.AccentGlow.copy(alpha = 0.18f * glowAlpha),
                        shape = CircleShape,
                    )
            )
            // Orb button
            Box(
                modifier = Modifier
                    .scale(if (recorderState == RecorderState.Recording) pulseScale else 1f)
                    .size(140.dp)
                    .background(VoxaColors.Card, CircleShape)
                    .border(
                        width = if (overlayActive) 2.dp else 1.5.dp,
                        color = when {
                            recorderState == RecorderState.Recording -> VoxaColors.RecordingRed
                            overlayActive -> VoxaColors.AccentAmber
                            else -> VoxaColors.Border
                        },
                        shape = CircleShape,
                    )
                    .clickable { toggleOverlay() },
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = when (recorderState) {
                            RecorderState.Processing -> "⏳"
                            RecorderState.Recording -> "🎙"
                            else -> "🎤"
                        },
                        fontSize = 32.sp,
                    )
                    WaveBars(
                        active = recorderState == RecorderState.Recording,
                        color = if (recorderState == RecorderState.Recording)
                            VoxaColors.RecordingRed else VoxaColors.AccentAmber,
                        barCount = 5,
                        heightDp = 20,
                    )
                }
            }
        }

        Spacer(Modifier.height(16.dp))
        Text(
            text = stateLabel,
            fontSize = 14.sp,
            fontWeight = FontWeight.Medium,
            color = stateColor,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(horizontal = 32.dp),
        )
        if (!overlayActive) {
            Spacer(Modifier.height(6.dp))
            Text(
                text = "Tap to enable floating mic overlay",
                fontSize = 12.sp,
                color = VoxaColors.TextDim,
            )
        }

        // ── Last Transcript ───────────────────────────────────────────────────
        if (lastTranscript.isNotEmpty()) {
            Spacer(Modifier.height(24.dp))
            SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
                Text(
                    text = "LAST TRANSCRIPT",
                    fontSize = 11.sp,
                    color = VoxaColors.TextMuted,
                    letterSpacing = 1.sp,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = lastTranscript,
                    fontSize = 16.sp,
                    fontStyle = FontStyle.Italic,
                    color = VoxaColors.TextPrimary,
                    lineHeight = 24.sp,
                )
            }
        }

        // ── Settings ──────────────────────────────────────────────────────────
        Spacer(Modifier.height(24.dp))
        SectionHeader(title = "Settings")
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            SettingsRow(
                label = "Auto-start on boot",
                sublabel = "Show overlay whenever you unlock your phone",
                checked = autoStart,
                onCheckedChange = {
                    autoStart = it
                    prefs.setAutoStart(it)
                },
            )
            Divider()
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

        // ── How it works ──────────────────────────────────────────────────────
        Spacer(Modifier.height(24.dp))
        SectionHeader(title = "How it works")
        SectionCard(modifier = Modifier.padding(horizontal = 24.dp)) {
            listOf(
                "1" to "Enable the floating overlay above",
                "2" to "Open any app with a text field",
                "3" to "Tap the Voxa mic that appears above the keyboard",
                "4" to "Speak — tap again to stop. Text is copied to clipboard",
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
                        Text(text = n, fontSize = 11.sp, color = VoxaColors.AccentAmber, fontWeight = FontWeight.Medium)
                    }
                    Spacer(Modifier.width(12.dp))
                    Text(
                        text = t,
                        fontSize = 14.sp,
                        color = VoxaColors.TextMuted,
                        lineHeight = 20.sp,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
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
                Text(sublabel, fontSize = 12.sp, color = VoxaColors.TextMuted, modifier = Modifier.padding(top = 2.dp))
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
            Text(trailingText, fontSize = 14.sp, color = if (destructive) VoxaColors.RecordingRed else VoxaColors.TextMuted)
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
    val anims = remember(barCount) { List(barCount) { Animatable(0.2f) } }
    val infiniteTransition = rememberInfiniteTransition(label = "wave")

    LaunchedEffect(active) {
        if (!active) {
            anims.forEach { anim ->
                launch { anim.animateTo(0.2f, spring()) }
            }
        }
    }

    Row(
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.height(heightDp.dp),
    ) {
        anims.forEachIndexed { i, anim ->
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
