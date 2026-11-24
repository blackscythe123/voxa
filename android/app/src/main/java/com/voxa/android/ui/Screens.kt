package com.voxa.android.ui

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Screen
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.HistoryEntry
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.WordPair
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter

// ── Home ─────────────────────────────────────────────────────────────────────

@Composable
fun HomeScreen(activity: MainActivity, go: (Screen) -> Unit) {
    val app = activity.voxa
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val account by app.account.collectAsState()
    val online by app.online.collectAsState()
    val blocked by app.blocked.collectAsState()
    val historyVersion by app.historyVersion.collectAsState()
    val mic = remember(tick) { app.micGranted() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    val selected = remember(tick) { app.keyboardSelected() }
    var recent by remember { mutableStateOf(emptyList<HistoryEntry>()) }
    LaunchedEffect(historyVersion, tick) { recent = withContext(Dispatchers.IO) { app.core.history("") } }
    val unsent = recent.count { it.status is EntryStatus.Waiting || it.status is EntryStatus.Failed }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
        TopBar(null, { Wordmark() }) {
            IconButton({ go(Screen.History) }) { Icon(painterResource(R.drawable.ic_history), "History", tint = V.Text) }
            IconButton({ go(Screen.Settings) }) { Icon(painterResource(R.drawable.ic_settings), "Settings", tint = V.Text) }
        }
        // What needs you, on top (desktop InfoBars). Wording from voxa-core copy.rs.
        if (session == SessionState.EXPIRED) InfoBar("Sign in to ChatGPT to dictate", "Voxa uses your own ChatGPT account", action = "Sign in") { go(Screen.SignIn) }
        if (!online) InfoBar("No connection. Sends when you're back", "Your recordings are kept")
        if (blocked && online) InfoBar("Couldn't reach ChatGPT", "Your sign-in is fine", error = true, action = "Retry") { app.checkSession(force = true) }
        if (unsent > 0) InfoBar(if (unsent == 1) "1 dictation not sent yet" else "$unsent dictations not sent yet", "Your recordings are kept", action = "See them") { go(Screen.History) }

        val ready = session != SessionState.EXPIRED && mic && enabled
        Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(painterResource(if (ready) R.drawable.ic_check else R.drawable.ic_alert), null, Modifier.size(16.dp), tint = if (ready) V.Ready else V.Warn)
            Spacer(Modifier.width(8.dp))
            Text(
                when {
                    ready && session == SessionState.UNKNOWN -> "Checking your sign-in"
                    ready -> "Ready to dictate"
                    else -> "Needs you first"
                },
                fontSize = 14.sp, color = if (ready) V.Ready else V.Warn,
            )
        }

        Section("Set up")
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            VCard {
                CardIcon(R.drawable.ic_account)
                Column(Modifier.weight(1f)) {
                    Text("ChatGPT account", fontSize = 14.sp, color = V.Text)
                    Muted(
                        when {
                            session == SessionState.EXPIRED -> "Not signed in"
                            account != null -> listOf(account!!.name.ifEmpty { account!!.email }, account!!.planLabel).filter { it.isNotEmpty() }.joinToString(" · ")
                            else -> "Checking your sign-in"
                        },
                    )
                }
                if (session == SessionState.EXPIRED) VButton("Sign in", accent = true) { go(Screen.SignIn) }
            }
            VCard {
                CardIcon(R.drawable.ic_mic)
                Column(Modifier.weight(1f)) {
                    Text("Microphone", fontSize = 14.sp, color = V.Text)
                    Muted(if (mic) "Allowed" else "Voxa listens only while you tap the mic")
                }
                if (!mic) VButton("Allow", accent = true) { activity.requestMic() }
            }
            VCard {
                CardIcon(R.drawable.ic_keyboard)
                Column(Modifier.weight(1f)) {
                    Text("Voxa keyboard", fontSize = 14.sp, color = V.Text)
                    Muted(
                        when {
                            selected -> "On and selected"
                            enabled -> "On. Choose it when you want to talk"
                            else -> "Off"
                        },
                    )
                }
                when {
                    !enabled -> VButton("Turn on", accent = true) { activity.openKeyboardSettings() }
                    !selected -> VButton("Choose") { activity.showKeyboardPicker() }
                }
            }
        }

        Section("Try it")
        TryField()

        if (recent.isNotEmpty()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Section("Recent")
                Spacer(Modifier.weight(1f))
                TextButton({ go(Screen.History) }) { Text("See all", color = V.Ink) }
            }
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                recent.take(3).forEach { HistoryRow(it, compact = true, onChanged = {}) }
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
fun TryField(placeholder: String = "Tap here, switch to Voxa, tap the mic and talk") {
    var text by remember { mutableStateOf("") }
    OutlinedTextField(
        text, { text = it },
        modifier = Modifier.fillMaxWidth(),
        placeholder = { Text(placeholder, fontSize = 14.sp) },
        minLines = 3,
        shape = RoundedCornerShape(4.dp),
        colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = V.Ink, unfocusedBorderColor = V.CardBorder, focusedContainerColor = Color.White, unfocusedContainerColor = Color.White),
    )
}

@Composable
fun TopBar(onBack: (() -> Unit)?, title: @Composable () -> Unit, actions: @Composable () -> Unit = {}) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (onBack != null) IconButton(onBack) { Icon(painterResource(R.drawable.ic_back), "Back", tint = V.Text) }
        Row(Modifier.weight(1f)) { title() }
        actions()
    }
}

@Composable
fun PageTitle(text: String) {
    Text(text, fontSize = 26.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
}

// ── History ──────────────────────────────────────────────────────────────────

@Composable
fun HistoryScreen(activity: MainActivity, go: (Screen) -> Unit) {
    val app = activity.voxa
    val version by app.historyVersion.collectAsState()
    var query by remember { mutableStateOf("") }
    var entries by remember { mutableStateOf(emptyList<HistoryEntry>()) }
    LaunchedEffect(version, query) { entries = withContext(Dispatchers.IO) { app.core.history(query) } }

    Column(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        TopBar({ go(Screen.Home) }, { PageTitle("History") })
        OutlinedTextField(
            query, { query = it },
            modifier = Modifier.fillMaxWidth(),
            placeholder = { Text("Search what you said or the app") },
            leadingIcon = { Icon(painterResource(R.drawable.ic_search), null, tint = V.Muted) },
            singleLine = true,
            shape = RoundedCornerShape(4.dp),
            colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = V.Ink, unfocusedBorderColor = V.CardBorder, focusedContainerColor = Color.White, unfocusedContainerColor = Color.White),
        )
        Spacer(Modifier.height(12.dp))
        if (entries.isEmpty()) {
            Muted(if (query.isBlank()) "Nothing yet. What you dictate shows up here, and stays on this phone." else "Nothing matches.")
        }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(entries, key = { it.id.toLong() }) { e -> HistoryRow(e, compact = false, onChanged = { app.historyChanged() }) }
            item { Spacer(Modifier.height(24.dp)) }
        }
    }
}

private val inFlight = setOf("Recording", "Writing it out")

fun statusLabel(s: EntryStatus): String = when (s) {
    is EntryStatus.Pasted -> "Pasted"
    is EntryStatus.Ready -> "Not pasted"
    is EntryStatus.Waiting -> s.v1
    is EntryStatus.Failed -> s.v1
}

fun whenLabel(id: ULong): String {
    val t = Instant.ofEpochMilli(id.toLong()).atZone(ZoneId.systemDefault())
    val today = LocalDate.now()
    val time = t.format(DateTimeFormatter.ofPattern("HH:mm"))
    return when (t.toLocalDate()) {
        today -> time
        today.minusDays(1) -> "Yesterday $time"
        else -> t.format(DateTimeFormatter.ofPattern("EEE d MMM HH:mm"))
    }
}

@Composable
fun HistoryRow(e: HistoryEntry, compact: Boolean, onChanged: () -> Unit) {
    val context = LocalContext.current
    val app = context.voxa
    val scope = rememberCoroutineScope()
    var sending by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    val live = e.status is EntryStatus.Waiting && (e.status as EntryStatus.Waiting).v1 in inFlight
    val written = e.status is EntryStatus.Pasted || e.status is EntryStatus.Ready

    VColumnCard {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(e.app.ifEmpty { "Voxa" }, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
            Text("  ·  ${whenLabel(e.id)}", fontSize = 13.sp, color = V.Muted)
            Spacer(Modifier.weight(1f))
            val label = statusLabel(e.status)
            Text(label, fontSize = 12.sp, color = if (e.status is EntryStatus.Pasted) V.Muted else V.Warn)
        }
        if (e.text.isNotEmpty()) {
            Text(e.text, Modifier.padding(top = 6.dp), fontSize = 15.sp, color = V.Text, maxLines = if (compact) 2 else 12)
        }
        if (!compact && !live) {
            Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (written) VButton("Copy") {
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voxa", e.text))
                }
                if (e.hasAudio) {
                    VButton(if (written) "Write it out again" else "Send now", accent = !written, enabled = !sending) {
                        sending = true
                        scope.launch {
                            withContext(Dispatchers.IO) { app.core.resend(e.id) }
                            sending = false
                            onChanged()
                        }
                    }
                }
                if (sending) CircularProgressIndicator(Modifier.size(18.dp), color = V.Ink, strokeWidth = 2.dp)
                Spacer(Modifier.weight(1f))
                IconButton({ confirmDelete = true }) { Icon(painterResource(R.drawable.ic_delete), "Delete", tint = V.Muted) }
            }
        }
    }
    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete this dictation?") },
            text = { Text("The text and the recording are removed from this phone.") },
            confirmButton = { TextButton({ app.core.deleteEntry(e.id); confirmDelete = false; onChanged() }) { Text("Delete", color = V.Error) } },
            dismissButton = { TextButton({ confirmDelete = false }) { Text("Cancel", color = V.Text) } },
        )
    }
}

// ── Settings ─────────────────────────────────────────────────────────────────

@Composable
fun SettingsScreen(activity: MainActivity, go: (Screen) -> Unit) {
    val app = activity.voxa
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val account by app.account.collectAsState()
    val version by app.historyVersion.collectAsState()
    var settings by remember { mutableStateOf(app.core.settings()) }
    val save = { s: uniffi.voxa_ffi.Settings -> settings = s; app.core.setSettings(s); app.historyChanged() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    val selected = remember(tick) { app.keyboardSelected() }
    var count by remember { mutableStateOf(0) }
    var bytes by remember { mutableStateOf(0uL) }
    LaunchedEffect(version) {
        withContext(Dispatchers.IO) {
            count = app.core.history("").size
            bytes = app.core.historyAudioBytes()
        }
    }
    var confirmClear by remember { mutableStateOf(false) }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
        TopBar({ go(Screen.Home) }, { PageTitle("Settings") })

        Section("Account")
        VCard {
            CardIcon(R.drawable.ic_account)
            Column(Modifier.weight(1f)) {
                if (session == SessionState.EXPIRED) {
                    Text("Not signed in", fontSize = 14.sp, color = V.Text)
                    Muted("Voxa uses your own ChatGPT account")
                } else {
                    Text(account?.name?.ifEmpty { null } ?: account?.email ?: "ChatGPT", fontSize = 14.sp, color = V.Text)
                    Muted(listOfNotNull(account?.email?.takeIf { it.isNotEmpty() && account?.name?.isNotEmpty() == true }, account?.planLabel).joinToString(" · "))
                }
            }
            if (session == SessionState.EXPIRED) VButton("Sign in", accent = true) { go(Screen.SignIn) } else VButton("Sign out") { app.signOut() }
        }

        Section("Keyboard")
        VCard {
            CardIcon(R.drawable.ic_keyboard)
            Column(Modifier.weight(1f)) {
                Text("Voxa keyboard", fontSize = 14.sp, color = V.Text)
                Muted(if (selected) "On and selected" else if (enabled) "On" else "Off")
            }
            if (!enabled) VButton("Turn on", accent = true) { activity.openKeyboardSettings() } else VButton("Choose") { activity.showKeyboardPicker() }
        }
        Spacer(Modifier.height(4.dp))
        ToggleRow("Key vibration", "A short tap when you press a key", settings.haptics) { save(settings.copy(haptics = it)) }

        Section("Writing")
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            ToggleRow("Remove filler words", "Drops \"um\", \"uh\" and \"hmm\". Off keeps your words as spoken", settings.removeFillers) { save(settings.copy(removeFillers = it)) }
            ToggleRow("Spoken formatting", "Say \"new line\", \"new paragraph\" or \"bullet point\"", settings.spokenFormatting) { save(settings.copy(spokenFormatting = it)) }
            VCard(onClick = { go(Screen.Words) }) {
                CardIcon(R.drawable.ic_words)
                Column(Modifier.weight(1f)) {
                    Text("Words", fontSize = 14.sp, color = V.Text)
                    Muted("Fix names ChatGPT gets wrong, and snippets")
                }
                Icon(painterResource(R.drawable.ic_chevron), null, tint = V.Muted)
            }
        }

        Section("History")
        VColumnCard {
            Text("Keep history", fontSize = 14.sp, color = V.Text)
            Muted("Dictations not sent yet are always kept")
            Row(Modifier.padding(top = 10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (d in listOf(7u, 30u, 90u, 0u)) {
                    VButton(if (d == 0u) "Forever" else "$d days", accent = settings.keepDays == d) { save(settings.copy(keepDays = d)) }
                }
            }
        }
        Spacer(Modifier.height(4.dp))
        VCard {
            CardIcon(R.drawable.ic_delete)
            Column(Modifier.weight(1f)) {
                Text("Delete history", fontSize = 14.sp, color = V.Text)
                Muted("$count dictations and %.1f MB of recordings".format(bytes.toLong() / 1_048_576.0))
            }
            VButton("Delete", enabled = count > 0) { confirmClear = true }
        }

        Section("Help")
        VCard(onClick = { save(settings.copy(onboarded = false)); activity.recreate() }) {
            CardIcon(R.drawable.ic_refresh)
            Text("Take the tour again", Modifier.weight(1f), fontSize = 14.sp, color = V.Text)
        }
        Spacer(Modifier.height(8.dp))
        Muted("Voxa ${activity.packageManager.getPackageInfo(activity.packageName, 0).versionName} · core ${uniffi.voxa_ffi.coreVersion()}")
        Spacer(Modifier.height(24.dp))
    }
    if (confirmClear) {
        AlertDialog(
            onDismissRequest = { confirmClear = false },
            title = { Text("Delete all history?") },
            text = { Text("Every dictation and recording on this phone is removed. This can't be undone.") },
            confirmButton = { TextButton({ app.core.clearHistory(); app.historyChanged(); confirmClear = false }) { Text("Delete all", color = V.Error) } },
            dismissButton = { TextButton({ confirmClear = false }) { Text("Cancel", color = V.Text) } },
        )
    }
}

@Composable
fun ToggleRow(title: String, detail: String, on: Boolean, onChange: (Boolean) -> Unit) {
    VCard {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 14.sp, color = V.Text)
            Muted(detail)
        }
        VToggle(on, onChange)
    }
}

// ── Words ────────────────────────────────────────────────────────────────────

@Composable
fun WordsScreen(onBack: () -> Unit) {
    val app = LocalContext.current.voxa
    val initial = remember { app.core.words() }
    val fixes = remember { mutableStateListOf(*initial.fixes.toTypedArray()) }
    val snippets = remember { mutableStateListOf(*initial.snippets.toTypedArray()) }
    val save = { app.core.setWords(app.core.words().copy(fixes = fixes.toList(), snippets = snippets.toList())) }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
        TopBar(onBack, { PageTitle("Words") })
        Muted("Applied on this phone after every dictation: fixes first, then snippets.")
        Section("Fixes")
        PairList(fixes, "When it writes", "Write instead") { save() }
        Section("Snippets")
        PairList(snippets, "When I say", "Type this") { save() }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun PairList(list: MutableList<WordPair>, fromLabel: String, toLabel: String, onChange: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        list.forEachIndexed { i, p ->
            VCard {
                Column(Modifier.weight(1f)) {
                    Text(p.from, fontSize = 14.sp, color = V.Muted)
                    Text(p.to, fontSize = 15.sp, color = V.Text)
                }
                IconButton({ list.removeAt(i); onChange() }) { Icon(painterResource(R.drawable.ic_delete), "Remove", tint = V.Muted) }
            }
        }
        var from by remember { mutableStateOf("") }
        var to by remember { mutableStateOf("") }
        VColumnCard {
            val colors = OutlinedTextFieldDefaults.colors(focusedBorderColor = V.Ink, unfocusedBorderColor = V.CardBorder)
            OutlinedTextField(from, { from = it }, Modifier.fillMaxWidth(), label = { Text(fromLabel) }, singleLine = true, colors = colors)
            OutlinedTextField(to, { to = it }, Modifier.fillMaxWidth(), label = { Text(toLabel) }, singleLine = true, colors = colors)
            Row(Modifier.padding(top = 8.dp)) {
                VButton("Add", accent = true, enabled = from.isNotBlank() && to.isNotBlank()) {
                    list.add(WordPair(from.trim(), to.trim()))
                    from = ""
                    to = ""
                    onChange()
                }
            }
        }
    }
}
