package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.voxa.android.BuildConfig
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Route
import com.voxa.android.diag.Diag
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.SessionState

@Composable
fun SettingsScreen(activity: MainActivity) {
    val c = vx
    val app = activity.voxa
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val account by app.account.collectAsState()
    val version by app.historyVersion.collectAsState()
    var settings by remember { mutableStateOf(app.core.settings()) }
    val save = { s: uniffi.voxa_ffi.Settings ->
        settings = s
        app.core.setSettings(s)
        app.historyChanged()
    }
    val enabled = remember(tick) { app.keyboardEnabled() }
    val selected = remember(tick) { app.keyboardSelected() }
    var count by remember { mutableIntStateOf(0) }
    LaunchedEffect(version) { count = withContext(Dispatchers.IO) { app.core.history("").size } }
    var confirmClear by remember { mutableStateOf(false) }
    var confirmSignOut by remember { mutableStateOf(false) }
    var pickKeep by remember { mutableStateOf(false) }
    val signedOut = session == SessionState.EXPIRED

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 24.dp, end = 24.dp, bottom = 24.dp)) {
        ScreenTitle("Settings")

        // Profile
        VCard(
            Modifier.padding(bottom = 24.dp), padding = PaddingValues(16.dp),
            onClick = { if (signedOut) activity.go(Route.SignIn) else confirmSignOut = true },
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Avatar(account?.name?.ifEmpty { account?.email } ?: "", 56.dp, image = account?.image ?: "")
                Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        if (signedOut) "Not signed in" else account?.name?.ifEmpty { null } ?: account?.email?.ifEmpty { null } ?: "ChatGPT",
                        style = Type.MBold, color = c.text,
                    )
                    Text(if (signedOut) "Tap to sign in to ChatGPT" else "ChatGPT account", style = Type.S, color = c.text56)
                }
                val plan = account?.planLabel?.removePrefix("ChatGPT ")?.trim().orEmpty()
                if (!signedOut && plan.isNotEmpty()) Pill(plan, PillKind.Good)
            }
        }

        VCard(Modifier.padding(bottom = 16.dp), padding = PaddingValues(horizontal = 16.dp, vertical = 4.dp)) {
            OptionRow(R.drawable.ic_keyboard, "Voxa keyboard", onClick = { if (!enabled) activity.openKeyboardSettings() else activity.showKeyboardPicker() }) {
                OptionValue(
                    when {
                        selected -> "On"
                        enabled -> "On, not chosen"
                        else -> "Off"
                    },
                )
            }
            val lookLabel = remember(tick) { com.voxa.android.ime.KeyboardLook.label(com.voxa.android.ime.KeyboardLook.load(activity)) }
            OptionRow(R.drawable.ic_palette, "Keyboard look", onClick = { activity.go(Route.KeyboardLook) }) {
                OptionValue(lookLabel)
            }
            OptionRow(R.drawable.ic_hand, "Hold to talk", onClick = { save(settings.copy(holdToTalk = !settings.holdToTalk)) }) {
                VSwitch(settings.holdToTalk) { save(settings.copy(holdToTalk = it)) }
            }
            OptionRow(R.drawable.ic_words, "Remove filler words", onClick = { save(settings.copy(removeFillers = !settings.removeFillers)) }) {
                VSwitch(settings.removeFillers) { save(settings.copy(removeFillers = it)) }
            }
            OptionRow(R.drawable.ic_vibrate, "Vibrate on tap", onClick = { save(settings.copy(haptics = !settings.haptics)) }) {
                VSwitch(settings.haptics) { save(settings.copy(haptics = it)) }
            }
            // Android's own page for Voxa's notifications (sign-in, dictations).
            val notifyOn = remember(tick) { com.voxa.android.notify.Notifier.allowed(activity) }
            OptionRow(R.drawable.ic_bell, "Notifications", onClick = { activity.openNotificationSettings() }) {
                OptionValue(if (notifyOn) "On" else "Off")
            }
            val theme = ThemePref.current.value
            val ctx = androidx.compose.ui.platform.LocalContext.current
            OptionRow(R.drawable.ic_theme, "Appearance", last = true, onClick = { ThemePref.set(ctx, ThemePref.next(theme)) }) {
                OptionValue(ThemePref.label(theme))
            }
        }

        VCard(Modifier.padding(bottom = 16.dp), padding = PaddingValues(horizontal = 16.dp, vertical = 4.dp)) {
            OptionRow(R.drawable.ic_lock, "Keep history", onClick = { pickKeep = true }) { OptionValue(keepLabel(settings.keepDays)) }
            OptionRow(R.drawable.ic_delete, "Delete history", last = true, danger = true, onClick = { if (count > 0) confirmClear = true }) {
                OptionValue(if (count == 1) "1 dictation" else "$count dictations")
            }
        }

        VCard(Modifier.padding(bottom = 16.dp), padding = PaddingValues(horizontal = 16.dp, vertical = 4.dp)) {
            OptionRow(R.drawable.ic_flag, "Take the tour again", onClick = { activity.restartSetup() }) {
                Icon(painterResource(R.drawable.ic_chevron), null, Modifier.size(20.dp), tint = c.text40)
            }
            if (Diag.enabled && BuildConfig.DEBUG) {
                OptionRow(R.drawable.ic_pulse, "Share diagnostics", onClick = { activity.shareDiagnostics() }) {
                    Icon(painterResource(R.drawable.ic_chevron), null, Modifier.size(20.dp), tint = c.text40)
                }
            }
            OptionRow(R.drawable.ic_info, "Version", last = true) { OptionValue(BuildConfig.VERSION_NAME) }
        }
    }

    if (confirmClear) {
        ConfirmDialog(
            "Delete all history?", "Every dictation and recording on this phone is removed. This can't be undone.", "Delete all",
            onConfirm = {
                // Off the main thread: every recording is deleted.
                app.scope.launch(Dispatchers.IO) {
                    app.core.clearHistory()
                    app.historyChanged()
                }
                confirmClear = false
            },
            onDismiss = { confirmClear = false },
        )
    }
    if (confirmSignOut) {
        ConfirmDialog(
            "Sign out of ChatGPT?", "Voxa can't dictate until you sign in again. Your history stays on this phone.", "Sign out",
            onConfirm = {
                app.signOut()
                confirmSignOut = false
            },
            onDismiss = { confirmSignOut = false },
        )
    }
    if (pickKeep) {
        AlertDialog(
            onDismissRequest = { pickKeep = false },
            containerColor = c.card,
            shape = RoundedCornerShape(24.dp),
            title = { Text("Keep history", style = Type.L, color = c.text) },
            text = {
                Column {
                    Text("Dictations not sent yet are always kept.", Modifier.padding(bottom = 8.dp), style = Type.S, color = c.text56)
                    for (d in listOf(7u, 30u, 90u, 0u)) {
                        val on = settings.keepDays == d
                        Row(
                            Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable {
                                save(settings.copy(keepDays = d))
                                pickKeep = false
                            }.padding(vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(Modifier.size(20.dp).clip(CircleShape).background(if (on) c.accent else c.text16), contentAlignment = Alignment.Center) {
                                Box(Modifier.size(if (on) 8.dp else 16.dp).clip(CircleShape).background(if (on) c.onAccent else c.card))
                            }
                            Spacer(Modifier.width(12.dp))
                            Text(keepLabel(d), style = Type.M, color = c.text)
                        }
                    }
                }
            },
            confirmButton = { TextButton({ pickKeep = false }) { Text("Done", style = Type.MBold, color = c.accent) } },
        )
    }
}

private fun keepLabel(days: UInt) = if (days == 0u) "Forever" else "$days days"
