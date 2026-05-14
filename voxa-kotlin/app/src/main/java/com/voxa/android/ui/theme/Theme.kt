package com.voxa.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight

// ── Colours ──────────────────────────────────────────────────────────────────
object VoxaColors {
    val Bg = Color(0xFF0D0B08)
    val Surface = Color(0xFF171310)
    val Card = Color(0xFF1E1A14)
    val Border = Color(0xFF2A231B)
    val AccentAmber = Color(0xFFC97D2E)
    val AccentBright = Color(0xFFE09840)
    val AccentGlow = Color(0x2EC97D2E)
    val TextPrimary = Color(0xFFF2E4C8)
    val TextMuted = Color(0xFF8A7862)
    val TextDim = Color(0xFF453C2E)
    val RecordingRed = Color(0xFFD94030)
    val RecordingGlow = Color(0x38D94030)
    val Success = Color(0xFF4CAF50)
}

// ── Spacing ───────────────────────────────────────────────────────────────────
object Sp {
    val xs = 4
    val sm = 8
    val md = 16
    val lg = 24
    val xl = 32
    val xxl = 48
}

// ── Typography ────────────────────────────────────────────────────────────────
// Using system serif/sans as fallback; replace Font() refs with bundled assets if desired.
val FrauncesFamily = FontFamily(
    Font(resId = android.R.font.serif, weight = FontWeight.Bold, style = FontStyle.Italic),
    Font(resId = android.R.font.serif, weight = FontWeight.Normal, style = FontStyle.Italic),
)

private val colorScheme = darkColorScheme(
    background = VoxaColors.Bg,
    surface = VoxaColors.Surface,
    primary = VoxaColors.AccentAmber,
    secondary = VoxaColors.AccentBright,
    onBackground = VoxaColors.TextPrimary,
    onSurface = VoxaColors.TextPrimary,
    outline = VoxaColors.Border,
)

@Composable
fun VoxaTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = colorScheme,
        content = content,
    )
}
