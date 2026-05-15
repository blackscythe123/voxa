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
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun HomeScreen(
    onOpenTutorial: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenAccount: () -> Unit,
    onOpenAbout: () -> Unit,
) {
    val context = LocalContext.current

    var sessionValid by remember { mutableStateOf(true) }
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
            StatusPill(connected = sessionValid)
        }

        Text(
            text = "Dictate anywhere on your phone. Tap a text field, switch to Voxa, hold to talk.",
            fontSize = 13.sp,
            color = VoxaColors.Muted,
            lineHeight = 18.sp,
            modifier = Modifier
                .padding(horizontal = 22.dp)
                .widthIn(max = 320.dp),
        )

        SetupHero(
            sessionValid = sessionValid,
            imeEnabled = imeEnabled.value,
            onEnableKeyboard = {
                context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
            },
            onOpenTutorial = onOpenTutorial,
        )

        // Nav tiles
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            NavTile(
                modifier = Modifier.weight(1f),
                icon = VoxaIcons.Book,
                title = "How to use",
                sub = "Step-by-step setup and dictation tour",
                onClick = onOpenTutorial,
            )
            NavTile(
                modifier = Modifier.weight(1f),
                icon = VoxaIcons.Settings,
                title = "Settings",
                sub = "Length, hold, trim, haptics",
                onClick = onOpenSettings,
            )
        }
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            NavTile(
                modifier = Modifier.weight(1f),
                icon = VoxaIcons.User,
                title = "Account",
                sub = "ChatGPT session, Sign out",
                onClick = onOpenAccount,
            )
            NavTile(
                modifier = Modifier.weight(1f),
                icon = VoxaIcons.Info,
                title = "About",
                sub = "Version, Privacy",
                onClick = onOpenAbout,
            )
        }

        Spacer(Modifier.height(24.dp))
        Text(
            text = "voxa · dictate anywhere",
            fontFamily = MonoFamily,
            fontSize = 10.sp,
            color = VoxaColors.MutedSoft,
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 30.dp),
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun SetupHero(
    sessionValid: Boolean,
    imeEnabled: Boolean,
    onEnableKeyboard: () -> Unit,
    onOpenTutorial: () -> Unit,
) {
    val accountDone = sessionValid
    val keyboardDone = imeEnabled
    val done = (if (accountDone) 1 else 0) + (if (keyboardDone) 1 else 0)
    val allDone = done == 2

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(Brush.verticalGradient(listOf(VoxaColors.Surface, VoxaColors.Bg)))
            .border(1.dp, VoxaColors.Hair, RoundedCornerShape(16.dp))
            .padding(18.dp),
    ) {
        Text(
            text = if (allDone) "All set" else "Setup · $done of 2",
            fontFamily = MonoFamily,
            fontSize = 10.sp,
            color = VoxaColors.Muted,
            letterSpacing = 0.6.sp,
        )
        Text(
            text = if (allDone) "Voxa is ready" else "Almost ready",
            fontFamily = DisplayFamily,
            fontWeight = FontWeight.SemiBold,
            fontSize = 18.sp,
            letterSpacing = (-0.3).sp,
            color = VoxaColors.Ink,
            modifier = Modifier.padding(top = 6.dp),
        )
        Text(
            text = if (allDone) {
                "Tap a text field in any app, switch to Voxa, and hold to talk."
            } else {
                "${2 - done} step left before you can dictate in other apps."
            },
            fontSize = 13.sp,
            color = VoxaColors.Muted,
            lineHeight = 18.sp,
            modifier = Modifier.padding(top = 6.dp),
        )

        Spacer(Modifier.height(14.dp))
        ChecklistItem(done = accountDone, label = "ChatGPT account linked")
        Spacer(Modifier.height(10.dp))
        ChecklistItem(done = keyboardDone, label = "Enable Voxa keyboard in system settings")

        Spacer(Modifier.height(14.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (!keyboardDone) {
                PrimaryButton(text = "Enable keyboard", onClick = onEnableKeyboard)
                Spacer(Modifier.width(10.dp))
                GhostButton(text = "How to use →", onClick = onOpenTutorial)
            } else {
                GhostButton(text = "How to use →", onClick = onOpenTutorial)
            }
        }
    }
}

@Composable
private fun ChecklistItem(done: Boolean, label: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            modifier = Modifier
                .size(18.dp)
                .clip(CircleShape)
                .background(if (done) VoxaColors.Success else VoxaColors.Surface)
                .then(if (!done) Modifier.border(1.4.dp, Color(0xFFD9D6CF), CircleShape) else Modifier),
            contentAlignment = Alignment.Center,
        ) {
            if (done) {
                Text("✓", color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            }
        }
        Spacer(Modifier.width(10.dp))
        Text(
            text = label,
            fontSize = 12.5.sp,
            color = if (done) VoxaColors.Muted else VoxaColors.InkSoft,
        )
    }
}

@Composable
private fun PrimaryButton(text: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .background(VoxaColors.Primary)
            .clickable { onClick() }
            .padding(horizontal = 14.dp, vertical = 9.dp),
    ) {
        Text(text, color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun GhostButton(text: String, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .clickable { onClick() }
            .padding(horizontal = 12.dp, vertical = 9.dp),
    ) {
        Text(text, color = VoxaColors.Primary, fontSize = 13.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun NavTile(
    modifier: Modifier = Modifier,
    icon: ImageVector,
    title: String,
    sub: String,
    onClick: () -> Unit,
) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(14.dp))
            .background(VoxaColors.Surface)
            .border(1.dp, VoxaColors.Hair, RoundedCornerShape(14.dp))
            .clickable { onClick() }
            .padding(16.dp, 16.dp, 14.dp, 14.dp)
            .heightIn(min = 108.dp),
    ) {
        Box(
            modifier = Modifier
                .size(36.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(VoxaColors.IconChip),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = VoxaColors.Ink,
                modifier = Modifier.size(20.dp),
            )
        }
        Spacer(Modifier.height(12.dp))
        Text(
            text = title,
            fontWeight = FontWeight.SemiBold,
            fontSize = 14.5.sp,
            color = VoxaColors.Ink,
            letterSpacing = (-0.1).sp,
        )
        Text(
            text = sub,
            fontSize = 11.5.sp,
            color = VoxaColors.Muted,
            lineHeight = 15.sp,
            modifier = Modifier.padding(top = 2.dp),
        )
    }
}
