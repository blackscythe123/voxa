package com.voxa.android.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.HistoryEntry
import uniffi.voxa_ffi.recordingLevels

/** One dictation: the words first and large, the recording, then copy, edit, write again; delete apart. */
@Composable
fun DetailScreen(activity: MainActivity, id: ULong) {
    val c = vx
    val app = activity.voxa
    val version by app.historyVersion.collectAsState()
    val scope = rememberCoroutineScope()
    var entry by remember { mutableStateOf<HistoryEntry?>(null) }
    var loaded by remember { mutableStateOf(false) }
    LaunchedEffect(version, id) {
        entry = withContext(Dispatchers.IO) { app.core.history("").firstOrNull { it.id == id } }
        loaded = true
    }
    var editing by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    var sending by remember { mutableStateOf(false) }
    val e = entry

    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp)) {
        Row(
            Modifier.height(48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button) { activity.back() }.padding(end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(painterResource(R.drawable.ic_back), null, Modifier.size(24.dp), tint = c.text)
            Spacer(Modifier.width(8.dp))
            Text("History", style = Type.MBold, color = c.text)
        }
        if (e == null) {
            if (loaded) Text("This dictation is no longer in History.", Modifier.padding(top = 24.dp), style = Type.M, color = c.text56)
            return@Column
        }
        val written = e.status is EntryStatus.Pasted || e.status is EntryStatus.Ready
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            Row(Modifier.padding(top = 8.dp, bottom = 24.dp), verticalAlignment = Alignment.CenterVertically) {
                AppBadge(e.app.ifEmpty { "Voxa" })
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(e.app.ifEmpty { "Voxa" }, style = Type.MBold, color = c.text)
                    Text("${whenLabel(e.id)}, ${statusWord(e.status)}", style = Type.S, color = c.text56)
                }
            }
            if (written) {
                Text(e.text, Modifier.padding(bottom = 24.dp), style = Type.Said, color = c.text)
            } else {
                Text(
                    if (e.hasAudio) "Not written out yet. Your recording is kept: send it when you're ready." else "Not written out, and the recording is gone.",
                    Modifier.padding(bottom = 24.dp), style = Type.Said, color = c.text56,
                )
            }
            e.audioPath?.let { Player(it) }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                ActionTile("Copy", R.drawable.ic_copy, enabled = written) {
                    activity.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voxa", e.text))
                    if (Build.VERSION.SDK_INT < 33) Toast.makeText(activity, "Copied", Toast.LENGTH_SHORT).show()
                }
                ActionTile("Edit", R.drawable.ic_edit, enabled = written) { editing = true }
                ActionTile(if (written) "Write again" else "Send now", R.drawable.ic_refresh, enabled = e.hasAudio && !sending, busy = sending) {
                    sending = true
                    scope.launch {
                        // The keyboard or the background send may be sending it right now: never twice.
                        val busy = withContext(Dispatchers.IO) {
                            if (app.keyboardOwns(e.id) || app.core.isSending(e.id)) true else app.core.resend(e.id) == null && app.core.isSending(e.id)
                        }
                        if (busy) Toast.makeText(activity, "It's being sent already", Toast.LENGTH_SHORT).show()
                        sending = false
                        app.historyChanged()
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 16.dp, bottom = 32.dp).navigationBarsPadding(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Box(
                Modifier.size(56.dp).clip(RoundedCornerShape(16.dp)).background(c.danger8).clickable(role = Role.Button) { confirmDelete = true },
                contentAlignment = Alignment.Center,
            ) {
                Icon(painterResource(R.drawable.ic_delete), "Delete", Modifier.size(24.dp), tint = c.danger)
            }
            SecondaryButton("Share", Modifier.weight(1f), enabled = written) { activity.shareText(e.text) }
        }
    }

    if (confirmDelete && e != null) {
        ConfirmDialog(
            "Delete this dictation?", "The text and the recording are removed from this phone.", "Delete",
            onConfirm = {
                confirmDelete = false
                app.core.deleteEntry(e.id)
                app.historyChanged()
                activity.back()
            },
            onDismiss = { confirmDelete = false },
        )
    }
    if (editing && e != null) {
        var text by remember { mutableStateOf(e.text) }
        AlertDialog(
            onDismissRequest = { editing = false },
            containerColor = c.card,
            shape = RoundedCornerShape(24.dp),
            title = { Text("Edit", style = Type.L, color = c.text) },
            text = { VTextField(text, { text = it }, "What you said", singleLine = false, minHeight = 160.dp) },
            confirmButton = {
                TextButton({
                    editing = false
                    app.core.setEntryText(e.id, text.trim())
                    app.historyChanged()
                }, enabled = text.isNotBlank()) { Text("Save", style = Type.MBold, color = c.accent) }
            },
            dismissButton = { TextButton({ editing = false }) { Text("Cancel", style = Type.MBold, color = c.text72) } },
        )
    }
}

@Composable
private fun androidx.compose.foundation.layout.RowScope.ActionTile(label: String, icon: Int, enabled: Boolean = true, busy: Boolean = false, onClick: () -> Unit) {
    val c = vx
    Column(
        Modifier.weight(1f).height(72.dp).clip(RoundedCornerShape(16.dp)).background(c.surface)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
    ) {
        val tint = if (enabled || busy) c.text else c.text40
        if (busy) CircularProgressIndicator(Modifier.size(24.dp), color = c.accent, strokeWidth = 2.dp)
        else Icon(painterResource(icon), null, Modifier.size(24.dp), tint = tint)
        Spacer(Modifier.height(4.dp))
        Text(label, style = Type.SBold, color = tint)
    }
}

/** The recording: play / pause, its waveform (from the take's own levels) filling as it plays. */
@Composable
private fun Player(path: String) {
    val c = vx
    val player = remember { PcmPlayer() }
    DisposableEffect(Unit) { onDispose { player.stop() } }
    var levels by remember { mutableStateOf(emptyList<Float>()) }
    LaunchedEffect(path) { levels = withContext(Dispatchers.IO) { recordingLevels(path, BARS.toUInt()) } }
    val total = remember(path) { player.durationSecs(path) }
    var playing by remember { mutableStateOf(false) }
    var progress by remember { mutableFloatStateOf(0f) }
    LaunchedEffect(playing) {
        while (playing) {
            progress = player.progress()
            delay(50)
        }
    }
    val toggle = {
        if (playing) {
            progress = player.progress()
            player.stop()
            playing = false
        } else {
            player.play(path, if (progress >= 0.999f) 0f else progress) {
                playing = false
                progress = 0f
            }
            playing = true
        }
    }
    VCard(Modifier.padding(bottom = 24.dp), padding = PaddingValues(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(48.dp).clip(CircleShape).background(c.accent).clickable(role = Role.Button, onClick = toggle),
                contentAlignment = Alignment.Center,
            ) {
                Icon(painterResource(if (playing) R.drawable.ic_pause else R.drawable.ic_play), if (playing) "Pause" else "Play", Modifier.size(20.dp), tint = c.onAccent)
            }
            Spacer(Modifier.width(16.dp))
            Row(Modifier.weight(1f).height(40.dp), horizontalArrangement = Arrangement.spacedBy(3.dp), verticalAlignment = Alignment.CenterVertically) {
                val bars = if (levels.isEmpty()) List(BARS) { 0f } else levels
                bars.forEachIndexed { i, l ->
                    val played = (i + 0.5f) / bars.size <= progress
                    Box(
                        Modifier.weight(1f).fillMaxHeight((0.12f + 0.88f * l).coerceIn(0.12f, 1f))
                            .clip(RoundedCornerShape(2.dp)).background(if (played) c.accent else c.text16),
                    )
                }
            }
            Spacer(Modifier.width(16.dp))
            val secs = if (playing || progress > 0f) (progress * total).toInt() else total.toInt()
            Text("%d:%02d".format(secs / 60, secs % 60), style = Type.SBold.copy(fontFeatureSettings = "tnum"), color = c.text56)
        }
    }
}

private const val BARS = 24
