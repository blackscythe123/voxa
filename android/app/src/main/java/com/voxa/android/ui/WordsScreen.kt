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
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.voxa.android.R
import com.voxa.android.voxa
import uniffi.voxa_ffi.WordPair

private enum class WordsTab(val label: String) { Fixes("Fixes"), Snippets("Snippets"), Formatting("Formatting") }

/** What is being edited in the sheet: a list, and the index (null to add). */
private data class Editing(val tab: WordsTab, val index: Int?)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WordsScreen() {
    val c = vx
    val app = LocalContext.current.voxa
    var words by remember { mutableStateOf(app.core.words()) }
    var tab by remember { mutableIntStateOf(0) }
    var editing by remember { mutableStateOf<Editing?>(null) }
    val current = WordsTab.entries[tab]
    val save = { next: uniffi.voxa_ffi.Words ->
        app.core.setWords(next)
        words = app.core.words()
    }

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 24.dp, end = 24.dp, bottom = 104.dp)) {
            ScreenTitle("Words")
            Segmented(WordsTab.entries.map { it.label }, tab) { tab = it }
            Spacer(Modifier.height(24.dp))
            when (current) {
                WordsTab.Fixes -> {
                    if (words.fixes.isEmpty()) Empty("No fixes yet. When ChatGPT keeps writing a word or a name wrong, add it here and Voxa writes it your way.")
                    words.fixes.forEachIndexed { i, p -> FixCard(p) { editing = Editing(WordsTab.Fixes, i) } }
                }
                WordsTab.Snippets -> {
                    if (words.snippets.isEmpty()) Empty("No snippets yet. Say a short phrase, and Voxa types the longer text you saved for it.")
                    words.snippets.forEachIndexed { i, p -> SnippetCard(p) { editing = Editing(WordsTab.Snippets, i) } }
                }
                WordsTab.Formatting -> FormattingCard(words.spokenFormatting) { save(words.copy(spokenFormatting = it)) }
            }
        }
        if (current != WordsTab.Formatting) {
            Fab(if (current == WordsTab.Fixes) "Add a fix" else "Add a snippet", R.drawable.ic_add, Modifier.align(Alignment.BottomCenter).padding(bottom = 16.dp)) {
                editing = Editing(current, null)
            }
        }
    }

    editing?.let { ed ->
        val list = if (ed.tab == WordsTab.Fixes) words.fixes else words.snippets
        val existing = ed.index?.let { list.getOrNull(it) }
        val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
        ModalBottomSheet(onDismissRequest = { editing = null }, sheetState = sheet, containerColor = c.base, shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)) {
            PairEditor(
                fix = ed.tab == WordsTab.Fixes,
                initial = existing,
                onSave = { p ->
                    val next = list.toMutableList().apply { if (ed.index != null) set(ed.index, p) else add(p) }
                    save(if (ed.tab == WordsTab.Fixes) words.copy(fixes = next) else words.copy(snippets = next))
                    editing = null
                },
                onDelete = ed.index?.let { i ->
                    {
                        val next = list.toMutableList().apply { removeAt(i) }
                        save(if (ed.tab == WordsTab.Fixes) words.copy(fixes = next) else words.copy(snippets = next))
                        editing = null
                    }
                },
            )
        }
    }
}

@Composable
fun Segmented(options: List<String>, selected: Int, onSelect: (Int) -> Unit) {
    val c = vx
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(c.surface).padding(4.dp)) {
        options.forEachIndexed { i, label ->
            val on = i == selected
            Box(
                Modifier.weight(1f).height(40.dp)
                    .then(if (on) Modifier.shadow(if (c.dark) 0.dp else 2.dp, RoundedCornerShape(12.dp)) else Modifier)
                    .clip(RoundedCornerShape(12.dp)).background(if (on) (if (c.dark) c.card else c.base) else c.surface)
                    .clickable(role = Role.Tab) { onSelect(i) },
                contentAlignment = Alignment.Center,
            ) {
                Text(label, style = Type.SBold, color = if (on) c.text else c.text56)
            }
        }
    }
}

@Composable
private fun Empty(text: String) {
    Text(text, Modifier.padding(vertical = 16.dp), style = Type.M, color = vx.text56)
}

/** "Heard ~~clawed code~~" then a green check and "Claude Code". The core keeps no use count, so
 *  there is no "Fixed N times" line. */
@Composable
private fun FixCard(p: WordPair, onClick: () -> Unit) {
    val c = vx
    VCard(Modifier.padding(bottom = 12.dp), padding = PaddingValues(16.dp), onClick = onClick) {
        Text(
            buildAnnotatedString {
                append("Heard ")
                withStyle(SpanStyle(textDecoration = TextDecoration.LineThrough)) { append(p.from) }
            },
            style = Type.S, color = c.text56,
        )
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(painterResource(R.drawable.ic_check), null, Modifier.size(18.dp), tint = c.good)
            Spacer(Modifier.width(8.dp))
            Text(p.to, style = Type.MBold, color = c.text)
        }
    }
}

@Composable
private fun SnippetCard(p: WordPair, onClick: () -> Unit) {
    val c = vx
    VCard(Modifier.padding(bottom = 12.dp), padding = PaddingValues(16.dp), onClick = onClick) {
        Text("Say “${p.from}”", style = Type.S, color = c.text56)
        Spacer(Modifier.height(8.dp))
        Text(p.to, style = Type.MBold, color = c.text, maxLines = 3, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun FormattingCard(on: Boolean, onChange: (Boolean) -> Unit) {
    val c = vx
    VCard(padding = PaddingValues(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Spoken formatting", Modifier.weight(1f), style = Type.MBold, color = c.text)
            VSwitch(on, onChange)
        }
        Spacer(Modifier.height(8.dp))
        Text(
            "Say a command and Voxa formats your text instead of writing the words. Off, they are written as you said them.",
            style = Type.S, color = c.text56,
        )
        Spacer(Modifier.height(16.dp))
        listOf("new line" to "Starts a new line", "new paragraph" to "Leaves an empty line", "bullet point" to "Starts a list item").forEachIndexed { i, (say, does) ->
            if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
            Row(Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("“$say”", Modifier.weight(1f), style = Type.M, color = if (on) c.text else c.text56)
                Text(does, style = Type.S, color = c.text56)
            }
        }
    }
}

@Composable
private fun PairEditor(fix: Boolean, initial: WordPair?, onSave: (WordPair) -> Unit, onDelete: (() -> Unit)?) {
    val c = vx
    var from by remember { mutableStateOf(initial?.from ?: "") }
    var to by remember { mutableStateOf(initial?.to ?: "") }
    Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 24.dp).navigationBarsPadding().imePadding()) {
        Text(
            when {
                fix && initial == null -> "Add a fix"
                fix -> "Edit fix"
                initial == null -> "Add a snippet"
                else -> "Edit snippet"
            },
            style = Type.L, color = c.text,
        )
        Spacer(Modifier.height(24.dp))
        FieldLabel(if (fix) "Voxa heard" else "When you say")
        VTextField(from, { from = it }, if (fix) "clawed code" else "my address")
        Spacer(Modifier.height(16.dp))
        FieldLabel(if (fix) "Write instead" else "Voxa types")
        VTextField(to, { to = it }, if (fix) "Claude Code" else "12 Park Road, Chennai", singleLine = fix, minHeight = if (fix) 56.dp else 96.dp)
        Spacer(Modifier.height(24.dp))
        PrimaryButton("Save", enabled = from.isNotBlank() && to.isNotBlank()) { onSave(WordPair(from.trim(), to.trim())) }
        if (onDelete != null) {
            Spacer(Modifier.height(12.dp))
            Box(
                Modifier.fillMaxWidth().height(48.dp).clip(RoundedCornerShape(16.dp)).clickable(role = Role.Button, onClick = onDelete),
                contentAlignment = Alignment.Center,
            ) {
                Text(if (fix) "Delete this fix" else "Delete this snippet", style = Type.MBold, color = c.danger)
            }
        }
    }
}
