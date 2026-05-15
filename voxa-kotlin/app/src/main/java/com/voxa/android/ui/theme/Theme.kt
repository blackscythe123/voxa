package com.voxa.android.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

// Quiet Sheet palette. Neutralised — pure-white surface with very light neutral chrome.
object VoxaColors {
    val Bg          = Color(0xFFFAFAFA)
    val Surface     = Color(0xFFFFFFFF)
    val SurfaceAlt  = Color(0xFFF5F5F5)
    val Ink         = Color(0xFF16161A)
    val InkSoft     = Color(0xFF2A2A30)
    val Muted       = Color(0xFF7A7A82)
    val MutedSoft   = Color(0xFFA5A29F)
    val Hair        = Color(0xFFE5E5E5)
    val HairSoft    = Color(0xFFEEEEEE)
    val IconChip    = Color(0xFFF2F2F2)
    val Primary     = Color(0xFF1F4FE0)
    val PrimarySoft = Color(0x141F4FE0)
    val Destructive = Color(0xFFE5484D)
    val DestructiveSoft = Color(0xFFFCEAEA)
    val Success     = Color(0xFF7FB069)
    val SuccessSoft = Color(0x217FB069)
    val SuccessFg   = Color(0xFF3E7B30)
    val Recording   = Color(0xFFE5484D)
    val RecordingSoft = Color(0x33E5484D)

    // Backward-compat aliases for code paths that may still reference old names.
    val Card           = Surface
    val Border         = Hair
    val AccentAmber    = Primary
    val AccentBright   = Primary
    val AccentGlow     = PrimarySoft
    val TextPrimary    = Ink
    val TextMuted      = Muted
    val TextDim        = MutedSoft
    val RecordingRed   = Recording
    val RecordingGlow  = RecordingSoft
}

object Sp {
    val xs = 4
    val sm = 8
    val md = 16
    val lg = 24
    val xl = 32
    val xxl = 48
}

val DisplayFamily: FontFamily = FontFamily.SansSerif
val BodyFamily: FontFamily    = FontFamily.SansSerif
val MonoFamily: FontFamily    = FontFamily.Monospace
// kept for source compatibility with any code that imports it
val FrauncesFamily: FontFamily = FontFamily.SansSerif

private val voxaTypography = Typography(
    displayLarge = TextStyle(
        fontFamily = DisplayFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 30.sp,
        letterSpacing = (-0.9).sp,
    ),
    headlineMedium = TextStyle(
        fontFamily = DisplayFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 22.sp,
        letterSpacing = (-0.5).sp,
    ),
    titleLarge = TextStyle(
        fontFamily = DisplayFamily,
        fontWeight = FontWeight.SemiBold,
        fontSize = 17.sp,
    ),
    bodyLarge = TextStyle(
        fontFamily = BodyFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 14.sp,
        lineHeight = 20.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = BodyFamily,
        fontWeight = FontWeight.Normal,
        fontSize = 13.sp,
        lineHeight = 18.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = MonoFamily,
        fontSize = 11.sp,
    ),
)

private val voxaColorScheme = lightColorScheme(
    background     = VoxaColors.Bg,
    surface        = VoxaColors.Surface,
    surfaceVariant = VoxaColors.SurfaceAlt,
    primary        = VoxaColors.Primary,
    secondary      = VoxaColors.Primary,
    onBackground   = VoxaColors.Ink,
    onSurface      = VoxaColors.Ink,
    onPrimary      = VoxaColors.Surface,
    outline        = VoxaColors.Hair,
    error          = VoxaColors.Destructive,
)

@Composable
fun VoxaTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = voxaColorScheme,
        typography = voxaTypography,
        content = content,
    )
}
