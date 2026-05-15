package com.voxa.android.ui

import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import com.voxa.android.VoxaApp
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun SettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val prefs = VoxaApp.prefs

    var maxDuration by remember { mutableIntStateOf(prefs.getMaxDuration()) }
    var holdToTalk by remember { mutableStateOf(prefs.getHoldToTalk()) }
    var trimSilence by remember { mutableStateOf(prefs.getTrimSilence()) }
    var haptics by remember { mutableStateOf(prefs.getHaptics()) }
    var showDurationDialog by remember { mutableStateOf(false) }

    val imeEnabled = remember { mutableStateOf(false) }
    fun checkImeEnabled() {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        imeEnabled.value = imm.enabledInputMethodList.any { it.packageName == context.packageName }
    }
    LaunchedEffect(Unit) { checkImeEnabled() }

    SubpageScaffold(title = "Settings", onBack = onBack) {

        SectionHeader("Keyboard")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Keyboard,
                name = if (imeEnabled.value) "Voxa keyboard" else "Voxa keyboard not enabled",
                sub = if (imeEnabled.value) "Enabled. Switch to it in any text field." else "Tap to enable in system settings.",
                onClick = if (imeEnabled.value) null else { ->
                    context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                },
                trailing = {
                    if (imeEnabled.value) {
                        Switch(checked = true, onCheckedChange = null, colors = quietSwitchColors())
                    } else {
                        TextButton(onClick = {
                            context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        }) {
                            Text("Enable", color = VoxaColors.Primary, fontSize = 13.sp)
                        }
                    }
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Swap,
                name = "Show keyboard picker",
                sub = "Opens the system switcher.",
                trailing = { Chevron() },
                onClick = {
                    val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                    @Suppress("DEPRECATION")
                    imm.showInputMethodPicker()
                },
            )
        }

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
        }

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
        }

        Spacer(Modifier.height(28.dp))
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
                            Text(
                                label,
                                color = VoxaColors.Primary,
                                modifier = Modifier.fillMaxWidth(),
                                textAlign = TextAlign.Start,
                            )
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
