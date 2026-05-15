package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors
import kotlinx.coroutines.launch

@Composable
fun AccountScreen(onBack: () -> Unit, onLogout: () -> Unit) {
    val scope = rememberCoroutineScope()
    var sessionValid by remember { mutableStateOf<Boolean?>(null) }
    var refreshing by remember { mutableStateOf(false) }
    var lastResultMessage by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        sessionValid = TranscriptionApi.validateSession()
    }

    SubpageScaffold(title = "Account", onBack = onBack) {

        // Hero card
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 6.dp)
                .clip(RoundedCornerShape(16.dp))
                .background(
                    Brush.verticalGradient(listOf(VoxaColors.Surface, VoxaColors.Bg))
                )
                .border(1.dp, VoxaColors.Hair, RoundedCornerShape(16.dp))
                .padding(18.dp),
        ) {
            Text(
                text = "ChatGPT session",
                fontFamily = MonoFamily,
                fontSize = 10.sp,
                color = VoxaColors.Muted,
                letterSpacing = 0.6.sp,
            )
            Row(
                modifier = Modifier.padding(top = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = when (sessionValid) {
                        true -> "Connected"
                        false -> "Expired"
                        null -> "Checking…"
                    },
                    fontFamily = DisplayFamily,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 18.sp,
                    letterSpacing = (-0.3).sp,
                    color = VoxaColors.Ink,
                )
                Spacer(Modifier.width(10.dp))
                if (sessionValid != null) StatusPill(connected = sessionValid == true)
            }
            Text(
                text = "Voxa transcribes through your ChatGPT account. If transcription starts failing, refresh the session here.",
                fontSize = 13.sp,
                color = VoxaColors.Muted,
                lineHeight = 18.sp,
                modifier = Modifier.padding(top = 6.dp),
            )
            Row(
                modifier = Modifier.padding(top = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .background(VoxaColors.Primary)
                        .clickable(enabled = !refreshing) {
                            refreshing = true
                            lastResultMessage = null
                            scope.launch {
                                val ok = TranscriptionApi.validateSession()
                                sessionValid = ok
                                lastResultMessage = if (ok) "Session is live." else "Session expired. Sign in again."
                                refreshing = false
                            }
                        }
                        .padding(horizontal = 14.dp, vertical = 9.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (refreshing) {
                            CircularProgressIndicator(
                                color = Color.White,
                                strokeWidth = 2.dp,
                                modifier = Modifier.size(14.dp),
                            )
                            Spacer(Modifier.width(8.dp))
                        }
                        Text(
                            text = if (refreshing) "Checking…" else "Refresh session",
                            color = Color.White,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
                if (lastResultMessage != null) {
                    Spacer(Modifier.width(12.dp))
                    Text(
                        text = lastResultMessage!!,
                        fontSize = 12.sp,
                        color = if (sessionValid == true) VoxaColors.SuccessFg else VoxaColors.Destructive,
                    )
                }
            }
        }

        SectionHeader("Linked account")
        Panel {
            SettingsRow(
                icon = VoxaIcons.SignOut,
                name = "Sign out",
                sub = "Clear session and re-link.",
                destructive = true,
                trailing = { Chevron(tint = VoxaColors.Destructive) },
                onClick = onLogout,
            )
        }

        Spacer(Modifier.height(28.dp))
    }
}
