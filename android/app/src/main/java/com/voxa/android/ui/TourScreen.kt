package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.SessionState

/**
 * First-run setup: one thing per step, each re-checked live when the user comes back from Android
 * settings. Every step can be skipped; Home then shows what is still missing.
 */
@Composable
fun TourScreen(activity: MainActivity, onSignIn: () -> Unit, onDone: () -> Unit) {
    val app = activity.voxa
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val account by app.account.collectAsState()
    val history by app.historyVersion.collectAsState()
    var step by remember { mutableIntStateOf(activity.getSharedPreferences("ui", 0).getInt("tour_step", 0)) }
    val goTo = { s: Int ->
        step = s
        activity.getSharedPreferences("ui", 0).edit().putInt("tour_step", s).apply()
    }
    val finish = {
        app.core.setSettings(app.core.settings().copy(onboarded = true))
        goTo(0)
        onDone()
    }
    val mic = remember(tick) { app.micGranted() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    val selected = remember(tick) { app.keyboardSelected() }
    val practiceFrom = remember(step) { System.currentTimeMillis().toULong() }
    var practiced by remember { mutableStateOf(false) }
    LaunchedEffect(history, step) {
        if (step == 5) practiced = withContext(Dispatchers.IO) {
            app.core.history("").any { it.id >= practiceFrom - 120_000uL && it.status is EntryStatus.Pasted && it.text.isNotBlank() }
        }
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Wordmark(26)
            Spacer(Modifier.weight(1f))
            if (step in 1..5) TextButton({ goTo(step + 1) }) { Text("Skip", color = V.Muted) }
        }
        Spacer(Modifier.height(8.dp))
        if (step in 1..5) Dots(step - 1, 5)
        Spacer(Modifier.height(24.dp))
        when (step) {
            0 -> Step(R.drawable.ic_feather, "Talk instead of typing, in any app.",
                "Say what you want to write. Voxa turns it into text with your own ChatGPT account and puts it where you are typing.") {
                VButton("Start", accent = true) { goTo(1) }
                Spacer(Modifier.width(8.dp))
                VButton("Skip setup") { finish() }
            }
            1 -> Step(R.drawable.ic_account, "Sign in to ChatGPT",
                "Voxa uses your own ChatGPT account, free works. You sign in on ChatGPT's own page; Voxa never sees your password, and your sign-in stays on this phone.") {
                if (session == SessionState.VALID) {
                    Done(listOfNotNull(account?.name?.ifEmpty { null } ?: account?.email, account?.planLabel).joinToString(" · ").ifEmpty { "Signed in" })
                    Spacer(Modifier.width(12.dp))
                    VButton("Next", accent = true) { goTo(2) }
                } else {
                    VButton(if (session == SessionState.UNKNOWN && app.core.hasSession()) "Sign in again" else "Sign in to ChatGPT", accent = true) { onSignIn() }
                }
            }
            2 -> Step(R.drawable.ic_mic, "Allow the microphone",
                "Voxa listens only while you tap the mic, and sends the recording to ChatGPT to write it out.") {
                if (mic) {
                    Done("Microphone allowed")
                    Spacer(Modifier.width(12.dp))
                    VButton("Next", accent = true) { goTo(3) }
                } else VButton("Allow microphone", accent = true) { activity.requestMic() }
            }
            3 -> Step(R.drawable.ic_keyboard, "Turn on the Voxa keyboard",
                "Android asks you to switch it on once, and warns that a keyboard can collect what you type. Voxa only listens when you tap the mic, and keeps your history on this phone.") {
                if (enabled) {
                    Done("Voxa keyboard is on")
                    Spacer(Modifier.width(12.dp))
                    VButton("Next", accent = true) { goTo(4) }
                } else VButton("Open keyboard settings", accent = true) { activity.openKeyboardSettings() }
            }
            4 -> Step(R.drawable.ic_keyboard, "Choose Voxa as your keyboard",
                "Tap the field below, then pick Voxa. Later, switch back to your usual keyboard with the keyboard key at the bottom left of Voxa.") {
                if (selected) {
                    Done("Voxa is your keyboard now")
                    Spacer(Modifier.width(12.dp))
                    VButton("Next", accent = true) { goTo(5) }
                } else VButton("Choose keyboard", accent = true) { activity.showKeyboardPicker() }
            }
            5 -> Step(R.drawable.ic_mic, "Try it",
                "Tap the field, tap the mic, say a sentence, then tap again. The text goes in where the cursor is.") {
                if (practiced) {
                    Done("That's it")
                    Spacer(Modifier.width(12.dp))
                    VButton("Next", accent = true) { goTo(6) }
                }
            }
            else -> Step(R.drawable.ic_check, "You're set",
                "Pick Voxa whenever you would rather talk. Everything you dictate is in History, on this phone.") {
                VButton("Done", accent = true) { finish() }
            }
        }
        if (step == 4 || step == 5) {
            Spacer(Modifier.height(20.dp))
            TryField(if (step == 4) "Tap here first" else "Tap here, then tap the mic")
        }
    }
}

@Composable
private fun Step(icon: Int, title: String, body: String, actions: @Composable () -> Unit) {
    Box(Modifier.size(64.dp).clip(RoundedCornerShape(16.dp)).background(V.InkContainer), contentAlignment = Alignment.Center) {
        Icon(painterResource(icon), null, Modifier.size(30.dp), tint = V.Ink)
    }
    Spacer(Modifier.height(20.dp))
    Text(title, fontSize = 24.sp, lineHeight = 30.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
    Spacer(Modifier.height(10.dp))
    Text(body, fontSize = 15.sp, lineHeight = 22.sp, color = V.Muted)
    Spacer(Modifier.height(24.dp))
    Row(verticalAlignment = Alignment.CenterVertically) { actions() }
}

@Composable
private fun Done(text: String) {
    Icon(painterResource(R.drawable.ic_check), null, Modifier.size(18.dp), tint = V.Ready)
    Spacer(Modifier.width(6.dp))
    Text(text, fontSize = 14.sp, color = V.Ready)
}

@Composable
private fun Dots(at: Int, n: Int) {
    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        repeat(n) { i ->
            Box(Modifier.size(width = if (i == at) 18.dp else 6.dp, height = 6.dp).clip(CircleShape).background(if (i <= at) V.Ink else Color(0x26000000)))
        }
    }
}
