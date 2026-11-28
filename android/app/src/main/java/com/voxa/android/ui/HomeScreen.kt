package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Route
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.HistoryEntry
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.WeekStats
import java.time.LocalTime
import java.time.ZoneId

/** What stands between the user and dictating, most important first; null when ready. */
enum class Problem(val text: String) {
    SignedOut("Sign in to ChatGPT to dictate"),
    MicDenied("Allow the microphone"),
    KeyboardOff("Turn on the Voxa keyboard"),
    Offline("You're offline. Dictations send when you're back"),
    Blocked("Couldn't reach ChatGPT"),
}

fun fix(activity: MainActivity, p: Problem) {
    when (p) {
        Problem.SignedOut -> activity.go(Route.SignIn)
        Problem.MicDenied -> activity.requestMic()
        Problem.KeyboardOff -> activity.openKeyboardSettings()
        Problem.Offline -> Unit
        Problem.Blocked -> activity.voxa.checkSession(force = true)
    }
}

@Composable
fun HomeScreen(activity: MainActivity) {
    val c = vx
    val app = activity.voxa
    val account by app.account.collectAsState()
    val version by app.historyVersion.collectAsState()
    val problems = rememberProblems(activity)
    var recent by remember { mutableStateOf(emptyList<HistoryEntry>()) }
    var week by remember { mutableStateOf<WeekStats?>(null) }
    // The second notifications ask: once the first dictation has worked (Android 13+, at most twice).
    var askNotify by remember { mutableStateOf(false) }
    val tick = activity.tick.intValue
    LaunchedEffect(version, tick) {
        askNotify = com.voxa.android.notify.Notifier.shouldAsk(activity) && !com.voxa.android.notify.Notifier.homeCardDismissed(activity) &&
            withContext(Dispatchers.IO) { app.core.history("").any { it.status is uniffi.voxa_ffi.EntryStatus.Pasted } }
    }
    LaunchedEffect(version) {
        withContext(Dispatchers.IO) {
            val offset = ZoneId.systemDefault().rules.getOffset(java.time.Instant.now()).totalSeconds / 60
            week = app.core.weekStats(offset)
            recent = app.core.history("").take(3)
        }
    }

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 24.dp, end = 24.dp, top = 8.dp, bottom = 104.dp)) {
            // The doodle, the greeting, and a notice only while something is wrong.
            HomeHeader(activity, account, problems)
            Spacer(Modifier.height(16.dp))
            if (askNotify) NotifyHomeCard(activity) { askNotify = false }
            UpdateHomeCard(activity)
            StatsCard(week)
            Spacer(Modifier.height(24.dp))
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text("Recent", Modifier.weight(1f), style = Type.MBold, color = c.text)
                if (recent.isNotEmpty()) TextAction("See all") { activity.go(Route.Tabbed(Tab.History)) }
            }
            Spacer(Modifier.height(8.dp))
            if (recent.isEmpty()) {
                Text("Nothing yet. Tap Try Voxa, then talk: your words show up here, and stay on this phone.", Modifier.padding(vertical = 12.dp), style = Type.M, color = c.text56)
            }
            recent.forEach { e -> HistoryItem(e) { activity.go(Route.Detail(e.id)) } }
        }
        Fab("Try Voxa", R.drawable.ic_mic, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) {
            activity.showSheet { PracticeSheet(activity) }
        }
    }
}

/** Time saved first and big, the week as bars, then words and speed. */
@Composable
private fun StatsCard(week: WeekStats?) {
    val c = vx
    VCard {
        Text(savedLabel(week?.savedSecs ?: 0u), style = Type.Big, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text("saved this week versus typing", style = Type.S, color = c.text56)
        Spacer(Modifier.height(16.dp))
        val days = week?.perDay ?: List(7) { 0u }
        val max = (days.maxOrNull() ?: 0u).coerceAtLeast(1u).toFloat()
        val today = week?.today?.toInt() ?: -1
        Row(Modifier.fillMaxWidth().height(48.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.Bottom) {
            days.forEachIndexed { i, n ->
                val f = (n.toFloat() / max).coerceIn(0.06f, 1f)
                Box(Modifier.weight(1f).fillMaxHeight(f).clip(RoundedCornerShape(6.dp)).background(if (i == today) c.accent else c.accent12))
            }
        }
        Spacer(Modifier.height(24.dp))
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
        Spacer(Modifier.height(16.dp))
        Row {
            Column(Modifier.weight(1f)) {
                Text("%,d".format((week?.words ?: 0uL).toLong()), style = Type.L.copy(fontFeatureSettings = "tnum"), color = c.text)
                Text("words", style = Type.S, color = c.text56)
            }
            Spacer(Modifier.width(16.dp))
            Column(Modifier.weight(1f)) {
                Text("${week?.wpm ?: 0u}", style = Type.L.copy(fontFeatureSettings = "tnum"), color = c.text)
                Text("words a minute", style = Type.S, color = c.text56)
            }
        }
    }
}

/** 1h 24m, 12m, 45s. */
fun savedLabel(secs: UInt): String {
    val s = secs.toInt()
    return when {
        s < 60 -> "${s}s"
        s < 3600 -> "${s / 60}m"
        else -> "${s / 3600}h ${(s % 3600) / 60}m"
    }
}

/** The Try Voxa sheet: a field to dictate into, with what to do. */
@Composable
fun PracticeSheet(activity: MainActivity) {
    val c = vx
    val app = activity.voxa
    val tick = activity.tick.intValue
    val selected = remember(tick) { app.keyboardSelected() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    var text by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        delay(300)
        runCatching { focus.requestFocus() }
    }
    Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 24.dp).navigationBarsPadding()) {
        Text("Try Voxa", style = Type.L, color = c.text)
        Spacer(Modifier.height(8.dp))
        Text(
            when {
                !enabled -> "Turn on the Voxa keyboard first, then come back here."
                !selected -> "Switch to the Voxa keyboard, then tap the mic and say a sentence."
                else -> "Tap the mic below and say a sentence. Tap it again when you're done."
            },
            style = Type.M, color = c.text72,
        )
        Spacer(Modifier.height(16.dp))
        VTextField(text, { text = it }, "Your words land here", Modifier.focusRequester(focus), singleLine = false, minHeight = 120.dp)
        Spacer(Modifier.height(16.dp))
        when {
            !enabled -> PrimaryButton("Open keyboard settings") { activity.openKeyboardSettings() }
            !selected -> PrimaryButton("Switch keyboard", icon = R.drawable.ic_keyboard) { activity.showKeyboardPicker() }
        }
    }
}
