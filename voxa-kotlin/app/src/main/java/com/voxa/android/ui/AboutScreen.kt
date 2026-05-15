package com.voxa.android.ui

import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors

@Composable
fun AboutScreen(onBack: () -> Unit) {
    SubpageScaffold(title = "About", onBack = onBack) {

        SectionHeader("Build")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Hash,
                name = "Version",
                trailing = {
                    Text(
                        text = "0.1.4 · build 142",
                        fontFamily = MonoFamily,
                        fontSize = 11.sp,
                        color = VoxaColors.Muted,
                    )
                },
            )
        }

        SectionHeader("Privacy")
        Text(
            text = "Voxa records your voice only while you hold the mic. Audio is uploaded to ChatGPT for transcription using your signed-in session, then deleted from the device.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 26.dp, vertical = 4.dp),
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "Your ChatGPT session cookie is stored locally. Voxa never sends it anywhere except chatgpt.com.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 26.dp, vertical = 4.dp),
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "No analytics. No third-party servers.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 26.dp, vertical = 4.dp),
        )

        Spacer(Modifier.height(28.dp))
    }
}
