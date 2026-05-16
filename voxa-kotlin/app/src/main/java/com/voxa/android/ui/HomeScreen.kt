package com.voxa.android.ui

import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.VoxaApp
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun HomeScreen(onLogout: () -> Unit) {
    val context = LocalContext.current
    val prefs = VoxaApp.prefs

    var maxDuration by remember { mutableIntStateOf(prefs.getMaxDuration()) }
    var holdToTalk by remember { mutableStateOf(prefs.getHoldToTalk()) }
    var trimSilence by remember { mutableStateOf(prefs.getTrimSilence()) }
    var autoPunct by remember { mutableStateOf(prefs.getAutoPunctuation()) }
    var haptics by remember { mutableStateOf(prefs.getHaptics()) }

    var sessionValid by remember { mutableStateOf(true) }
    var showDurationDialog by remember { mutableStateOf(false) }

    val imeEnabled = remember { mutableStateOf(false) }
    fun checkImeEnabled() {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imeEnabled.value = imm.enabledInputMethodList.any { it.packageName == context.packageName }
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
    ) {

        // Top bar — wordmark + connection pill
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 22.dp, end = 22.dp, top = 14.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "voxa",
                fontFamily = DisplayFamily,
                fontWeight = FontWeight.SemiBold,
                fontSize = 28.sp,
                letterSpacing = (-0.9).sp,
                color = VoxaColors.Ink,
            )
            Spacer(Modifier.weight(1f))
            StatusPill(
                connected = sessionValid,
            )
        }

        Text(
            text = "Dictate anywhere on your phone. Tap the keyboard switcher and pick Voxa, hold to talk, send.",
            fontSize = 13.sp,
            color = VoxaColors.Muted,
            lineHeight = 18.sp,
            modifier = Modifier.padding(horizontal = 22.dp).widthIn(max = 320.dp),
        )

        // KEYBOARD
        SectionHeader("Keyboard")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Keyboard,
                name = if (imeEnabled.value) "Voxa keyboard" else "Voxa keyboard not enabled",
                sub = if (imeEnabled.value) "Active. Switch to it in any text field." else "Tap to enable in system settings.",
                trailing = {
                    if (imeEnabled.value) {
                        Switch(
                            checked = true,
                            onCheckedChange = null,
                            colors = quietSwitchColors(),
                        )
                    } else {
                        TextButton(onClick = {
                            context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        }) {
                            Text("Enable", color = VoxaColors.Primary, fontSize = 13.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Star,
                name = "Set as default keyboard",
                sub = "Skip the system picker.",
                trailing = { Chevron() },
                onClick = {
                    context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Swap,
                name = "Switch keyboard now",
                sub = "Useful for testing.",
                trailing = { Chevron() },
                onClick = {
                    val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    @Suppress("DEPRECATION")
                    imm.showInputMethodPicker()
                },
            )
        }

        // RECORDING
        SectionHeader("Recording")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Infinity,
                name = "Max length",
                sub = formatDurationLabel(maxDuration) + ". Tap to choose Unlimited.",
                trailing = {
                    Text(
                        text = formatDurationShort(maxDuration),
                        fontFamily = MonoFamily,
                        fontSize = 11.sp,
                        color = VoxaColors.Muted,
                    )
                    Spacer(Modifier.width(6.dp))
                    Chevron()
                },
                onClick = { showDurationDialog = true },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Gauge,
                name = "Sample rate",
                sub = "Higher means more bandwidth.",
                trailing = {
                    Text("16 kHz", fontFamily = MonoFamily, fontSize = 11.sp, color = VoxaColors.Muted)
                    Spacer(Modifier.width(6.dp))
                    Chevron()
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Pulse,
                name = "Trim silence",
                sub = "Auto-stop on sustained quiet.",
                trailing = {
                    Switch(
                        checked = trimSilence,
                        onCheckedChange = { trimSilence = it; prefs.setTrimSilence(it) },
                        colors = quietSwitchColors(),
                    )
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Finger,
                name = "Hold to talk",
                sub = "Long-press the mic to record.",
                trailing = {
                    Switch(
                        checked = holdToTalk,
                        onCheckedChange = { holdToTalk = it; prefs.setHoldToTalk(it) },
                        colors = quietSwitchColors(),
                    )
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Mic,
                name = "Test microphone",
                sub = "Quick 3-second diagnostic.",
                trailing = { Chevron() },
            )
        }

        // TRANSCRIPTION
        SectionHeader("Transcription")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Globe,
                name = "Language",
                sub = null,
                trailing = {
                    Text("English (US)", fontFamily = MonoFamily, fontSize = 11.sp, color = VoxaColors.Muted)
                    Spacer(Modifier.width(6.dp))
                    Chevron()
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Chip,
                name = "Model",
                sub = "Powered by your ChatGPT account.",
                trailing = {
                    Text("whisper-1", fontFamily = MonoFamily, fontSize = 11.sp, color = VoxaColors.Muted)
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Period,
                name = "Auto-punctuation",
                sub = null,
                trailing = {
                    Switch(
                        checked = autoPunct,
                        onCheckedChange = { autoPunct = it; prefs.setAutoPunctuation(it) },
                        colors = quietSwitchColors(),
                    )
                },
            )
        }

        // FEEDBACK
        SectionHeader("Feedback")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Pulse,
                name = "Haptic feedback",
                sub = "Vibrate on tap, send, and error.",
                trailing = {
                    Switch(
                        checked = haptics,
                        onCheckedChange = { haptics = it; prefs.setHaptics(it) },
                        colors = quietSwitchColors(),
                    )
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Speaker,
                name = "Start and stop sound",
                sub = null,
                trailing = {
                    Switch(
                        checked = false,
                        onCheckedChange = {},
                        colors = quietSwitchColors(),
                    )
                },
            )
        }

        // ACCOUNT
        SectionHeader("Account")
        Panel {
            SettingsRow(
                icon = VoxaIcons.CircleCheck,
                name = "ChatGPT session",
                sub = if (sessionValid) "Connected." else "Session expired.",
                trailing = { Chevron() },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Refresh,
                name = "Refresh session",
                sub = null,
                trailing = { Chevron() },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.SignOut,
                name = "Sign out",
                sub = "Clear session and re-link.",
                destructive = true,
                trailing = { Chevron(tint = VoxaColors.Destructive) },
                onClick = onLogout,
            )
        }

        // ABOUT
        SectionHeader("About")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Info,
                name = "Privacy",
                sub = "What Voxa stores and sends.",
                trailing = { Chevron() },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Hash,
                name = "Version",
                sub = null,
                trailing = {
                    Text("0.1.4 · build 142", fontFamily = MonoFamily, fontSize = 11.sp, color = VoxaColors.Muted)
                },
            )
        }

        Spacer(Modifier.height(24.dp))
        Text(
            text = "voxa · dictate anywhere",
            fontFamily = MonoFamily,
            fontSize = 10.sp,
            color = VoxaColors.MutedSoft,
            modifier = Modifier.fillMaxWidth().padding(bottom = 30.dp),
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        )
    }

    if (showDurationDialog) {
        AlertDialog(
            onDismissRequest = { showDurationDialog = false },
            containerColor = VoxaColors.Surface,
            title = { Text("Max length", color = VoxaColors.Ink) },
            text = {
                Column {
                    listOf(60, 120, 180, 300, 0).forEach { sec ->
                        val label = if (sec == 0) "Unlimited (as needed)" else "${sec}s"
                        TextButton(
                            onClick = {
                                maxDuration = sec
                                prefs.setMaxDuration(sec)
                                showDurationDialog = false
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(label, color = VoxaColors.Primary, modifier = Modifier.fillMaxWidth(),
                                textAlign = androidx.compose.ui.text.style.TextAlign.Start)
                        }
                    }
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = { showDurationDialog = false }) {
                    Text("Cancel", color = VoxaColors.Muted)
                }
            },
        )
    }
}

private fun formatDurationLabel(sec: Int): String =
    if (sec == 0) "Unlimited"
    else "${sec / 60}:${(sec % 60).toString().padStart(2, '0')}"

private fun formatDurationShort(sec: Int): String =
    if (sec == 0) "∞" else formatDurationLabel(sec)

// ─────────────────── reusable bits ───────────────────

@Composable
private fun SectionHeader(title: String) {
    Text(
        text = title,
        fontFamily = DisplayFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 13.sp,
        letterSpacing = (-0.2).sp,
        color = VoxaColors.Ink,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 26.dp)
            .padding(top = 24.dp, bottom = 8.dp),
    )
}

@Composable
private fun Panel(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(VoxaColors.Surface)
            .border(1.dp, VoxaColors.Hair, RoundedCornerShape(12.dp)),
        content = content,
    )
}

@Composable
private fun HairDivider() {
    HorizontalDivider(color = VoxaColors.HairSoft, thickness = 1.dp)
}

@Composable
private fun StatusPill(connected: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .clip(RoundedCornerShape(99.dp))
            .background(if (connected) VoxaColors.SuccessSoft else VoxaColors.DestructiveSoft)
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) {
        Box(
            modifier = Modifier
                .size(5.dp)
                .clip(CircleShape)
                .background(if (connected) VoxaColors.Success else VoxaColors.Destructive)
        )
        Spacer(Modifier.width(6.dp))
        Text(
            text = if (connected) "Connected" else "Expired",
            fontFamily = MonoFamily,
            fontSize = 10.sp,
            color = if (connected) VoxaColors.SuccessFg else VoxaColors.Destructive,
        )
    }
}

@Composable
private fun Chevron(tint: Color = VoxaColors.MutedSoft) {
    Icon(
        imageVector = VoxaIcons.Chevron,
        contentDescription = null,
        tint = tint,
        modifier = Modifier.size(16.dp),
    )
}

@Composable
private fun SettingsRow(
    icon: ImageVector?,
    name: String,
    sub: String?,
    destructive: Boolean = false,
    onClick: (() -> Unit)? = null,
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val rowColor = if (destructive) VoxaColors.Destructive else VoxaColors.Ink
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable { onClick() } else Modifier)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Box(
                modifier = Modifier
                    .size(30.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (destructive) VoxaColors.DestructiveSoft else VoxaColors.IconChip),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = if (destructive) VoxaColors.Destructive else VoxaColors.Ink,
                    modifier = Modifier.size(18.dp),
                )
            }
            Spacer(Modifier.width(13.dp))
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(name, color = rowColor, fontSize = 14.5.sp, fontWeight = FontWeight.Medium)
            if (sub != null) {
                Text(sub, color = VoxaColors.Muted, fontSize = 11.5.sp, modifier = Modifier.padding(top = 2.dp))
            }
        }
        trailing()
    }
}

@Composable
private fun quietSwitchColors() = SwitchDefaults.colors(
    checkedTrackColor = VoxaColors.Primary,
    uncheckedTrackColor = Color(0xFFD6D2C7),
    checkedThumbColor = Color.White,
    uncheckedThumbColor = Color.White,
    checkedBorderColor = VoxaColors.Primary,
    uncheckedBorderColor = Color(0xFFD6D2C7),
)
