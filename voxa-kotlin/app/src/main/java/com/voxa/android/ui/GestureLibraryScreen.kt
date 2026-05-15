package com.voxa.android.ui

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.ui.icons.VoxaIcons
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors
import helium314.keyboard.latin.utils.ChecksumCalculator
import helium314.keyboard.latin.utils.JniUtils
import helium314.keyboard.latin.utils.protectedPrefs
import java.io.File

private const val LIB_FILE_NAME = "libjni_latinime.so"
private const val PREF_LIBRARY_CHECKSUM = "lib_checksum"

// HeliBoard's README points users to this exact tag of erkserkserks/openboard
// for the glide typing native library. Files live under
// `app/src/main/jniLibs/<abi>/libjni_latinime.so`. We construct the raw URL
// per device ABI so the user gets the exact right file in one tap.
private const val LIB_BASE_URL =
    "https://github.com/erkserkserks/openboard/raw/46fdf2b550035ca69299ce312fa158e7ade36967/app/src/main/jniLibs/"

private fun downloadUrlForAbi(abi: String): String =
    "${LIB_BASE_URL}${abi}/libjni_latinime.so"

@Composable
fun GestureLibraryScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val libFile = remember { File(context.filesDir, LIB_FILE_NAME) }
    var installed by remember { mutableStateOf(libFile.exists()) }
    var statusMessage by remember { mutableStateOf<StatusKind?>(null) }
    val expectedChecksum = remember { JniUtils.expectedDefaultChecksum() }
    val abi = remember { Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown" }

    val pickLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val tmp = File(context.filesDir, "tmp_${LIB_FILE_NAME}")
        try {
            context.contentResolver.openInputStream(uri).use { input ->
                if (input == null) { statusMessage = StatusKind.OpenFailed; return@rememberLauncherForActivityResult }
                tmp.outputStream().use { input.copyTo(it) }
            }
            tmp.setReadOnly()
            val checksum = ChecksumCalculator.checksum(tmp) ?: ""
            if (checksum != expectedChecksum) {
                tmp.delete()
                statusMessage = StatusKind.ChecksumMismatch(expected = expectedChecksum, got = checksum)
                return@rememberLauncherForActivityResult
            }
            libFile.setWritable(true)
            libFile.delete()
            tmp.copyTo(libFile, overwrite = true)
            libFile.setReadOnly()
            tmp.delete()
            // JniUtils reads the stored checksum from protectedPrefs to verify on next launch.
            context.protectedPrefs().edit().putString(PREF_LIBRARY_CHECKSUM, checksum).commit()
            installed = true
            statusMessage = StatusKind.Installed
            Toast.makeText(context, "Swipe typing library installed. Restart Voxa to enable.", Toast.LENGTH_LONG).show()
        } catch (e: Exception) {
            tmp.delete()
            statusMessage = StatusKind.OpenFailed
        }
    }

    SubpageScaffold(title = "Swipe typing", onBack = onBack) {

        Text(
            text = "Slide your finger across the keys to type whole words at once.",
            fontSize = 13.sp,
            color = VoxaColors.Muted,
            lineHeight = 18.sp,
            modifier = Modifier.padding(horizontal = 22.dp, vertical = 4.dp),
        )

        SectionHeader("Status")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Pulse,
                name = if (installed) "Library installed" else "Library not installed",
                sub = if (installed) {
                    "Swipe typing is available across all Voxa keyboards once Voxa is restarted."
                } else {
                    "Tap below to install. The library is verified by SHA-256 before activation."
                },
            )
        }

        SectionHeader("Why this extra step?")
        Text(
            text = "Swipe typing needs a small native library that Voxa cannot legally bundle in the app: the binary is Google's, distributed with Gboard, and we do not have a license to redistribute it. HeliBoard, which powers Voxa's typing keyboard, supports loading the same library from a file you provide yourself.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier.padding(horizontal = 26.dp, vertical = 4.dp),
        )
        Spacer(Modifier.height(8.dp))
        Text(
            text = "Download the matching libjni_latinime.so for your device's ABI from a community mirror (search 'HeliBoard libjni_latinime.so' or check the HeliBoard issue tracker). Voxa verifies the file against the upstream checksum before installing — a wrong file is rejected.",
            fontSize = 13.sp,
            color = VoxaColors.InkSoft,
            lineHeight = 19.sp,
            modifier = Modifier.padding(horizontal = 26.dp, vertical = 4.dp),
        )

        SectionHeader("Your device")
        Panel {
            SettingsRow(
                icon = VoxaIcons.Chip,
                name = "ABI",
                trailing = {
                    Text(abi, fontFamily = MonoFamily, fontSize = 11.sp, color = VoxaColors.Muted)
                },
            )
            HairDivider()
            SettingsRow(
                icon = VoxaIcons.Hash,
                name = "Expected SHA-256",
                sub = expectedChecksum.take(32) + "…",
            )
        }

        Spacer(Modifier.height(16.dp))

        // Step 1 — open the community download page in a browser
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(VoxaColors.PrimarySoft)
                    .border(1.dp, VoxaColors.Primary, RoundedCornerShape(12.dp))
                    .clickable {
                        // Direct download link for the user's device ABI, per
                        // HeliBoard's README pointer to erkserkserks/openboard.
                        // We don't redistribute the file ourselves — we just hand
                        // off to the browser/DownloadManager.
                        context.startActivity(
                            Intent(
                                Intent.ACTION_VIEW,
                                Uri.parse(downloadUrlForAbi(abi)),
                            ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                        )
                    }
                    .padding(vertical = 13.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = "1. Download library for $abi",
                    color = VoxaColors.Primary,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
        Spacer(Modifier.height(10.dp))

        // Step 2 — pick the file you downloaded
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(VoxaColors.Primary)
                    .clickable { pickLauncher.launch(arrayOf("*/*")) }
                    .padding(vertical = 13.dp),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = if (installed) "Replace library file" else "2. Pick downloaded file (.so)",
                    color = Color.White,
                    fontSize = 14.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }

        if (installed) {
            Spacer(Modifier.height(10.dp))
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(12.dp))
                        .background(VoxaColors.DestructiveSoft)
                        .border(1.dp, VoxaColors.Destructive, RoundedCornerShape(12.dp))
                        .clickable {
                            libFile.delete()
                            context.protectedPrefs().edit().remove(PREF_LIBRARY_CHECKSUM).commit()
                            installed = false
                            statusMessage = StatusKind.Removed
                        }
                        .padding(vertical = 13.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = "Remove library",
                        color = VoxaColors.Destructive,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                    )
                }
            }
        }

        statusMessage?.let { msg ->
            Spacer(Modifier.height(14.dp))
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(
                        if (msg is StatusKind.Installed) VoxaColors.SuccessSoft else VoxaColors.DestructiveSoft,
                    )
                    .border(
                        1.dp,
                        if (msg is StatusKind.Installed) VoxaColors.Success else VoxaColors.Destructive,
                        RoundedCornerShape(12.dp),
                    )
                    .padding(14.dp),
            ) {
                Column {
                    Text(
                        text = when (msg) {
                            StatusKind.Installed -> "Installed. Tap below to restart Voxa now."
                            StatusKind.Removed -> "Library removed. Swipe typing disabled."
                            StatusKind.OpenFailed -> "Could not read that file."
                            is StatusKind.ChecksumMismatch -> "Checksum mismatch — this is the wrong file."
                        },
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        color = if (msg is StatusKind.Installed) VoxaColors.SuccessFg else VoxaColors.Destructive,
                    )
                    if (msg is StatusKind.ChecksumMismatch) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = "Expected SHA-256: ${msg.expected.take(24)}…\nFile SHA-256: ${msg.got.take(24)}…",
                            fontSize = 11.sp,
                            fontFamily = MonoFamily,
                            color = VoxaColors.InkSoft,
                            lineHeight = 16.sp,
                        )
                    }
                    if (msg is StatusKind.Installed) {
                        Spacer(Modifier.height(8.dp))
                        Box(
                            modifier = Modifier
                                .clip(RoundedCornerShape(8.dp))
                                .background(VoxaColors.Primary)
                                .clickable {
                                    // Hard-restart so JniUtils' static initializer re-runs and loads the lib.
                                    android.os.Process.killProcess(android.os.Process.myPid())
                                }
                                .padding(horizontal = 14.dp, vertical = 8.dp),
                        ) {
                            Text("Restart Voxa", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                }
            }
        }

        Spacer(Modifier.height(28.dp))
    }
}

private sealed class StatusKind {
    object Installed : StatusKind()
    object Removed : StatusKind()
    object OpenFailed : StatusKind()
    data class ChecksumMismatch(val expected: String, val got: String) : StatusKind()
}
