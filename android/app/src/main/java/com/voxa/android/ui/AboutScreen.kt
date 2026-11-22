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

        SectionHeader("Credits")
        Text(
            text = "The QWERTY keyboard inside Voxa is built on HeliBoard, an open-source Android keyboard maintained by Helium314 and community contributors. HeliBoard is itself a modernized fork of OpenBoard and AOSP LatinIME. Voxa adds the voice-dictation mic, the Whisper transcription bridge, and the surrounding app shell.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 26.dp, vertical = 4.dp),
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "HeliBoard is licensed GPL-3.0. Source: github.com/Helium314/HeliBoard",
            fontSize = 12.sp,
            color = VoxaColors.Muted,
            lineHeight = 17.sp,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 26.dp, vertical = 4.dp),
        )

        Spacer(Modifier.height(28.dp))
    }
}
