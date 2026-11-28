package com.voxa.android.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Route
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.SessionState

private const val STEPS = 5
/** "Keep Voxa up to date": after the keyboard, before "Try it". */
private const val UPDATE_STEP = 3

/**
 * First-run setup, one step per screen: sign in, allow the mic, turn on the keyboard, allow updates
 * (optional, the owner's one onboarding permission exception), switch to it and try once. Each step
 * is re-checked live when the user comes back from Android settings.
 */
@Composable
fun SetupScreen(activity: MainActivity) {
    val c = vx
    val app = activity.voxa
    val prefs = remember { activity.getSharedPreferences("ui", 0) }
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val account by app.account.collectAsState()
    val history by app.historyVersion.collectAsState()
    var step by remember { mutableIntStateOf(prefs.getInt("setup_step", 0).coerceIn(0, STEPS - 1)) }
    val goTo = { s: Int ->
        step = s
        prefs.edit().putInt("setup_step", s).apply()
        if (s == STEPS - 1) prefs.edit().putLong("setup_try_from", System.currentTimeMillis()).apply()
    }
    val signedIn = session == SessionState.VALID
    val mic = remember(tick) { app.micGranted() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    val selected = remember(tick) { app.keyboardSelected() }
    // Read live: the pref only records whether Voxa may ask again.
    val updates = remember(tick) { com.voxa.android.notify.Updater.canInstall(activity) }
    val updatePrefs = remember { activity.getSharedPreferences("update", 0) }
    var tried by remember { mutableStateOf(false) }
    LaunchedEffect(history, step) {
        if (step == STEPS - 1) {
            val from = prefs.getLong("setup_try_from", System.currentTimeMillis()) - 120_000
            tried = withContext(Dispatchers.IO) {
                app.core.history("").any { it.id.toLong() >= from && it.status is EntryStatus.Pasted && it.text.isNotBlank() }
            }
        }
    }
    val name = account?.name?.ifEmpty { null } ?: account?.email?.ifEmpty { null }
    val checks = listOf(
        (if (signedIn && name != null) "Signed in as $name" else "Signed in to ChatGPT") to signedIn,
        "Microphone allowed" to mic,
        "Voxa keyboard turned on" to enabled,
        "Updates allowed" to updates,
        "Tried Voxa once" to tried,
    )
    val done = checks[step].second
    val next = {
        if (step == UPDATE_STEP) updatePrefs.edit().putString("onboarding_ask", if (updates) "granted" else "skipped").apply()
        if (step < STEPS - 1) goTo(step + 1) else { goTo(0); activity.finishSetup() }
    }
    var practice by remember { mutableStateOf("") }

    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp)) {
        val progress by animateFloatAsState((step + 1f) / STEPS, label = "progress")
        Box(Modifier.padding(top = 8.dp, bottom = 32.dp).fillMaxWidth().height(4.dp).clip(RoundedCornerShape(2.dp)).background(c.surface)) {
            Box(Modifier.fillMaxWidth(progress).fillMaxHeight().clip(RoundedCornerShape(2.dp)).background(c.accent))
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            Box(Modifier.fillMaxWidth().height(232.dp).clip(RoundedCornerShape(24.dp)).background(c.surface), contentAlignment = Alignment.Center) {
                when (step) {
                    0 -> SignInArt(name, signedIn, account?.image ?: "")
                    1 -> MicArt(mic)
                    2 -> KeyboardArt()
                    UPDATE_STEP -> UpdateArt(updates)
                    else -> Box(Modifier.padding(24.dp)) {
                        VTextField(practice, { practice = it }, "Tap here, then tap the mic", singleLine = false, minHeight = 120.dp, rest = c.base)
                    }
                }
            }
            Spacer(Modifier.height(32.dp))
            val (title, body) = when (step) {
                0 -> "Sign in to ChatGPT" to "Voxa writes out what you say with your own ChatGPT account, free works. You sign in on ChatGPT's own page; Voxa never sees your password."
                1 -> "Allow the microphone" to "Voxa listens only while the mic is on, and sends the recording to ChatGPT to write it out."
                2 -> "Turn on the Voxa keyboard" to "Android will show a warning that keyboards can read what you type. Voxa only sends what you say, and only while the mic is on."
                UPDATE_STEP -> "Keep Voxa up to date" to "New versions of Voxa install with one tap. Android asks you once to allow this. On the next screen, turn on the switch for Voxa, then come back."
                else -> "Switch to Voxa and try it" to "Tap the field above and pick the Voxa keyboard. Tap the mic, say a sentence, and tap it again."
            }
            Text(title, Modifier.padding(bottom = 8.dp), style = Type.L, color = c.text)
            Text(body, Modifier.padding(bottom = 16.dp), style = Type.M.copy(lineHeight = Type.M.fontSize * 1.5f), color = c.text72)
            Column(Modifier.padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                checks.take(step + 1).forEach { (label, ok) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        CheckMark(ok)
                        Spacer(Modifier.width(12.dp))
                        Text(label, style = Type.M, color = c.text)
                    }
                }
            }
        }
        Column(Modifier.padding(bottom = 16.dp).navigationBarsPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
            when {
                done -> PrimaryButton(if (step == STEPS - 1) "Finish" else "Continue", onClick = next)
                step == 0 -> PrimaryButton(if (app.core.hasSession()) "Sign in again" else "Sign in to ChatGPT") { activity.go(Route.SignIn) }
                step == 1 -> PrimaryButton("Allow microphone") { activity.requestMic() }
                step == 2 -> PrimaryButton("Open keyboard settings") { activity.openKeyboardSettings() }
                step == UPDATE_STEP -> PrimaryButton("Allow updates") { activity.openInstallSettings() }
                !selected -> PrimaryButton("Switch keyboard", icon = R.drawable.ic_keyboard) { activity.showKeyboardPicker() }
                else -> PrimaryButton("Finish", onClick = next)
            }
            // A way past the step, unless the button already is one.
            if (!done && !(step == STEPS - 1 && selected)) {
                Spacer(Modifier.height(8.dp))
                TextAction(if (step == STEPS - 1) "Skip for now" else "Not now", c.text56) { next() }
            } else {
                Spacer(Modifier.height(48.dp))
            }
        }
    }
}

/** A small card: the account, checked once signed in. */
@Composable
private fun SignInArt(name: String?, signedIn: Boolean, image: String) {
    val c = vx
    Row(Modifier.width(232.dp).vcard(c, 16.dp).padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
        Avatar(name ?: "", 40.dp, image = image)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(if (signedIn) name ?: "Signed in" else "ChatGPT", style = Type.SBold, color = c.text, maxLines = 1)
            Text(if (signedIn) "ChatGPT account" else "Your own account", style = Type.XS, color = c.text56)
        }
        CheckMark(signedIn)
    }
}

/** The mic in its rings. */
@Composable
private fun MicArt(granted: Boolean) {
    val c = vx
    Box(
        Modifier.size(144.dp).drawBehind {
            drawCircle(c.accent.copy(alpha = 0.06f), radius = size.minDimension / 2)
            drawCircle(c.accent.copy(alpha = 0.12f), radius = size.minDimension / 2 - 24.dp.toPx())
        },
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(64.dp).clip(CircleShape).background(if (granted) c.good else c.accent), contentAlignment = Alignment.Center) {
            Icon(painterResource(if (granted) R.drawable.ic_check else R.drawable.ic_mic), null, Modifier.size(28.dp), tint = c.onAccent)
        }
    }
}

/** The mockup's mini keyboard: a voice wave over the mic. */
@Composable
private fun KeyboardArt() {
    val c = vx
    Column(Modifier.width(220.dp).vcard(c, 16.dp).padding(16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(Modifier.height(32.dp), horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
            for (h in listOf(10, 22, 14, 30, 18, 26, 12, 20, 8)) {
                Box(Modifier.size(3.dp, h.dp).clip(RoundedCornerShape(2.dp)).background(c.accent))
            }
        }
        Spacer(Modifier.height(12.dp))
        Box(Modifier.size(48.dp).clip(CircleShape).background(c.accent), contentAlignment = Alignment.Center) {
            Icon(painterResource(R.drawable.ic_mic), null, Modifier.size(24.dp), tint = c.onAccent)
        }
    }
}
