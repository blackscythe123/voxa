package com.voxa.android.ui.keyboard

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

enum class RecState { Idle, Recording, Transcribing }

sealed class KeyAction {
    data class Char(val c: String) : KeyAction()
    object Backspace : KeyAction()
    object Space : KeyAction()
    object Enter : KeyAction()
    object Shift : KeyAction()
    object SymbolsToggle : KeyAction()
    object EmojiToggle : KeyAction()
    object Mic : KeyAction()
}

private enum class Layout { Letters, Symbols, Emoji }
private enum class ShiftState { Off, On, Caps }

private val ROW_LETTERS_1 = listOf("q","w","e","r","t","y","u","i","o","p")
private val ROW_LETTERS_2 = listOf("a","s","d","f","g","h","j","k","l")
private val ROW_LETTERS_3 = listOf("z","x","c","v","b","n","m")
private val ROW_SYMS_1   = listOf("1","2","3","4","5","6","7","8","9","0")
private val ROW_SYMS_2   = listOf("@","#","$","_","&","-","+","(",")","/")
private val ROW_SYMS_3   = listOf("*","\"","'",":",";","!","?")

private val EMOJIS = listOf(
    "😀","😃","😄","😁","😆","😅","🤣","😂","🙂","🙃",
    "😉","😊","😇","🥰","😍","🤩","😘","😋","😛","😜",
    "🤪","😝","🤑","🤗","🤔","🤐","🤨","😐","😑","😶",
    "😏","😒","🙄","😬","🤥","😌","😔","😪","🤤","😴",
    "😷","🤒","🤕","🤢","🤮","🥵","🥶","😵","🤯","🤠",
    "👍","👎","👏","🙏","💪","🫶","❤️","🧡","💛","💚",
    "💙","💜","🔥","✨","🎉","🎂","☕","🌧️","☀️","🌙",
)

@Composable
fun VoxaKeyboard(
    recState: RecState,
    onAction: (KeyAction) -> Unit,
    recordingTimerLabel: String? = null,
) {
    var layout by remember { mutableStateOf(Layout.Letters) }
    var shift by remember { mutableStateOf(ShiftState.On) }

    val kbBg = Color(0xFFF0F1F2)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(kbBg)
            .padding(top = 6.dp, bottom = 8.dp),
    ) {
        TopBar(
            recState = recState,
            timerLabel = recordingTimerLabel,
            onMicTap = { onAction(KeyAction.Mic) },
        )

        when (layout) {
            Layout.Letters -> LettersLayout(
                shift = shift,
                onChar = { c ->
                    val out = if (shift != ShiftState.Off) c.uppercase() else c
                    onAction(KeyAction.Char(out))
                    if (shift == ShiftState.On) shift = ShiftState.Off
                },
                onShift = {
                    shift = when (shift) {
                        ShiftState.Off  -> ShiftState.On
                        ShiftState.On   -> ShiftState.Caps
                        ShiftState.Caps -> ShiftState.Off
                    }
                },
                onBackspace = { onAction(KeyAction.Backspace) },
                onSymbols = { layout = Layout.Symbols },
                onEmoji = { layout = Layout.Emoji },
                onSpace = { onAction(KeyAction.Space) },
                onEnter = { onAction(KeyAction.Enter) },
                onPunct = { onAction(KeyAction.Char(".")) },
            )
            Layout.Symbols -> SymbolsLayout(
                onChar = { c -> onAction(KeyAction.Char(c)) },
                onBackspace = { onAction(KeyAction.Backspace) },
                onLetters = { layout = Layout.Letters },
                onEmoji = { layout = Layout.Emoji },
                onSpace = { onAction(KeyAction.Space) },
                onEnter = { onAction(KeyAction.Enter) },
            )
            Layout.Emoji -> EmojiLayout(
                onChar = { c -> onAction(KeyAction.Char(c)) },
                onBackspace = { onAction(KeyAction.Backspace) },
                onLetters = { layout = Layout.Letters },
            )
        }
    }
}

@Composable
private fun TopBar(
    recState: RecState,
    timerLabel: String?,
    onMicTap: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(modifier = Modifier.weight(1f)) {
            when (recState) {
                RecState.Idle -> SuggestionRowPlaceholder()
                RecState.Recording -> RecordingBanner(timerLabel)
                RecState.Transcribing -> TranscribingBanner(timerLabel)
            }
        }
        Spacer(Modifier.width(4.dp))
        MicButton(state = recState, onClick = onMicTap)
    }
}

@Composable
private fun SuggestionRowPlaceholder() {
    // We don't have real word-suggestions yet (would need a dictionary).
    // Keeping the row empty for now so the bar height stays stable.
    Box(modifier = Modifier.fillMaxWidth().height(32.dp))
}

@Composable
private fun RecordingBanner(timer: String?) {
    val transition = rememberInfiniteTransition(label = "rec-blink")
    val dotAlpha by transition.animateFloat(
        initialValue = 1f,
        targetValue = 0.2f,
        animationSpec = infiniteRepeatable(
            animation = tween(700, easing = LinearEasing),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "alpha",
    )
    Row(
        modifier = Modifier.fillMaxWidth().height(32.dp).padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .alpha(dotAlpha)
                .clip(CircleShape)
                .background(VoxaColors.Recording),
        )
        Spacer(Modifier.width(8.dp))
        Text(
            text = "Recording",
            color = VoxaColors.Recording,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
        )
        if (timer != null) {
            Spacer(Modifier.weight(1f))
            Text(
                text = timer,
                color = VoxaColors.InkSoft,
                fontSize = 11.5.sp,
                fontFamily = MonoFamily,
            )
        }
    }
}

@Composable
private fun TranscribingBanner(timer: String?) {
    Row(
        modifier = Modifier.fillMaxWidth().height(32.dp).padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "Transcribing…",
            color = VoxaColors.Primary,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
        )
        if (timer != null) {
            Spacer(Modifier.weight(1f))
            Text(
                text = "$timer captured",
                color = VoxaColors.Muted,
                fontSize = 11.5.sp,
                fontFamily = MonoFamily,
            )
        }
    }
}

@Composable
private fun MicButton(state: RecState, onClick: () -> Unit) {
    val bg = when (state) {
        RecState.Idle -> VoxaColors.PrimarySoft
        RecState.Recording -> VoxaColors.Recording
        RecState.Transcribing -> VoxaColors.PrimarySoft
    }
    val border = when (state) {
        RecState.Idle -> VoxaColors.Primary.copy(alpha = 0.25f)
        RecState.Recording -> VoxaColors.Recording
        RecState.Transcribing -> VoxaColors.Primary.copy(alpha = 0.25f)
    }
    val iconTint = if (state == RecState.Recording) Color.White else VoxaColors.Primary

    Box(
        modifier = Modifier
            .size(36.dp)
            .clip(CircleShape)
            .background(bg)
            .border(1.dp, border, CircleShape)
            .clickable(enabled = state != RecState.Transcribing) { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        when (state) {
            RecState.Idle -> Icon(VoxaIcons.Mic, contentDescription = "Start dictation", tint = iconTint, modifier = Modifier.size(18.dp))
            RecState.Recording -> Icon(VoxaIcons.Stop, contentDescription = "Stop and transcribe", tint = iconTint, modifier = Modifier.size(14.dp))
            RecState.Transcribing -> CircularProgressIndicator(
                color = VoxaColors.Primary,
                strokeWidth = 2.dp,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

// ───────── Letters layout ─────────

@Composable
private fun LettersLayout(
    shift: ShiftState,
    onChar: (String) -> Unit,
    onShift: () -> Unit,
    onBackspace: () -> Unit,
    onSymbols: () -> Unit,
    onEmoji: () -> Unit,
    onSpace: () -> Unit,
    onEnter: () -> Unit,
    onPunct: () -> Unit,
) {
    LetterRow(letters = ROW_LETTERS_1, shift = shift, onChar = onChar)
    LetterRow(letters = ROW_LETTERS_2, shift = shift, onChar = onChar, sidePad = true)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FnKey(modifier = Modifier.weight(1.4f), onClick = onShift) {
            Icon(
                imageVector = if (shift == ShiftState.Caps) VoxaIcons.ShiftFilled else VoxaIcons.Shift,
                contentDescription = "Shift",
                tint = if (shift != ShiftState.Off) VoxaColors.Primary else VoxaColors.Ink,
                modifier = Modifier.size(18.dp),
            )
        }
        ROW_LETTERS_3.forEach { c ->
            LetterKey(modifier = Modifier.weight(1f), char = c, shift = shift, onChar = onChar)
        }
        FnKey(modifier = Modifier.weight(1.4f), onClick = onBackspace) {
            Icon(VoxaIcons.Backspace, contentDescription = "Backspace", tint = VoxaColors.Ink, modifier = Modifier.size(18.dp))
        }
    }
    BottomRow(
        leftLabel = "?123",
        onLeft = onSymbols,
        onEmoji = onEmoji,
        onSpace = onSpace,
        onPunct = onPunct,
        onEnter = onEnter,
    )
}

@Composable
private fun LetterRow(
    letters: List<String>,
    shift: ShiftState,
    onChar: (String) -> Unit,
    sidePad: Boolean = false,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (sidePad) Spacer(Modifier.weight(0.5f))
        letters.forEach { c ->
            LetterKey(modifier = Modifier.weight(1f), char = c, shift = shift, onChar = onChar)
        }
        if (sidePad) Spacer(Modifier.weight(0.5f))
    }
}

// ───────── Symbols layout ─────────

@Composable
private fun SymbolsLayout(
    onChar: (String) -> Unit,
    onBackspace: () -> Unit,
    onLetters: () -> Unit,
    onEmoji: () -> Unit,
    onSpace: () -> Unit,
    onEnter: () -> Unit,
) {
    SymRow(ROW_SYMS_1, onChar)
    SymRow(ROW_SYMS_2, onChar)
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FnKey(modifier = Modifier.weight(1.4f), onClick = {}) {
            Text("=\\<", color = VoxaColors.Ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }
        ROW_SYMS_3.forEach { c ->
            LetterKey(modifier = Modifier.weight(1f), char = c, shift = ShiftState.Off, onChar = onChar)
        }
        FnKey(modifier = Modifier.weight(1.4f), onClick = onBackspace) {
            Icon(VoxaIcons.Backspace, contentDescription = "Backspace", tint = VoxaColors.Ink, modifier = Modifier.size(18.dp))
        }
    }
    BottomRow(
        leftLabel = "ABC",
        onLeft = onLetters,
        onEmoji = onEmoji,
        onSpace = onSpace,
        onPunct = { onChar(",") },
        onEnter = onEnter,
    )
}

@Composable
private fun SymRow(syms: List<String>, onChar: (String) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        syms.forEach { c ->
            LetterKey(modifier = Modifier.weight(1f), char = c, shift = ShiftState.Off, onChar = onChar)
        }
    }
}

// ───────── Emoji layout ─────────

@Composable
private fun EmojiLayout(
    onChar: (String) -> Unit,
    onBackspace: () -> Unit,
    onLetters: () -> Unit,
) {
    Box(modifier = Modifier.fillMaxWidth().heightIn(min = 200.dp, max = 240.dp)) {
        LazyVerticalGrid(
            columns = GridCells.Fixed(8),
            modifier = Modifier.fillMaxSize().padding(horizontal = 6.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            items(EMOJIS) { e ->
                Box(
                    modifier = Modifier
                        .aspectRatio(1f)
                        .clickable { onChar(e) },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(e, fontSize = 22.sp)
                }
            }
        }
    }
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        FnKey(modifier = Modifier.weight(1.4f), onClick = onLetters) {
            Text("ABC", color = VoxaColors.Ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }
        Spacer(Modifier.weight(4f))
        FnKey(modifier = Modifier.weight(1.4f), onClick = onBackspace) {
            Icon(VoxaIcons.Backspace, contentDescription = "Backspace", tint = VoxaColors.Ink, modifier = Modifier.size(18.dp))
        }
    }
}

// ───────── Bottom utility row ─────────

@Composable
private fun BottomRow(
    leftLabel: String,
    onLeft: () -> Unit,
    onEmoji: () -> Unit,
    onSpace: () -> Unit,
    onPunct: () -> Unit,
    onEnter: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp).padding(top = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FnKey(modifier = Modifier.weight(1.4f), onClick = onLeft) {
            Text(leftLabel, color = VoxaColors.Ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
        }
        FnKey(modifier = Modifier.weight(1f), onClick = onEmoji) {
            Icon(VoxaIcons.Smiley, contentDescription = "Emoji", tint = VoxaColors.Ink, modifier = Modifier.size(18.dp))
        }
        SpaceKey(modifier = Modifier.weight(5f), onClick = onSpace)
        FnKey(modifier = Modifier.weight(1f), onClick = onPunct) {
            Text(".", color = VoxaColors.Ink, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
        SendKey(modifier = Modifier.weight(1.4f), onClick = onEnter)
    }
}

// ───────── Key primitives ─────────

@Composable
private fun LetterKey(
    modifier: Modifier,
    char: String,
    shift: ShiftState,
    onChar: (String) -> Unit,
) {
    val label = if (char.length == 1 && char[0].isLetter() && shift != ShiftState.Off) char.uppercase() else char
    Box(
        modifier = modifier
            .height(44.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(VoxaColors.Surface)
            .clickable { onChar(char) },
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = VoxaColors.Ink, fontSize = 15.sp)
    }
}

@Composable
private fun FnKey(
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = modifier
            .height(44.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(Color(0xFFDCDEE0))
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

@Composable
private fun SpaceKey(modifier: Modifier, onClick: () -> Unit) {
    Box(
        modifier = modifier
            .height(44.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(VoxaColors.Surface)
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Text("space", color = VoxaColors.Muted, fontSize = 12.sp)
    }
}

@Composable
private fun SendKey(modifier: Modifier, onClick: () -> Unit) {
    Box(
        modifier = modifier
            .height(44.dp)
            .clip(RoundedCornerShape(5.dp))
            .background(VoxaColors.Primary)
            .clickable { onClick() },
        contentAlignment = Alignment.Center,
    ) {
        Icon(VoxaIcons.Return, contentDescription = "Enter", tint = Color.White, modifier = Modifier.size(18.dp))
    }
}
