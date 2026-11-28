package com.voxa.android.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.voxa.android.R

/**
 * The v5 tokens (docs/ui-mocks/v5-app-redesign.html): paper, wash, card, sepia ink, night text.
 * Dark follows the mockup's dark keyboard: warm near-black paper and the pale ink #E2B48F.
 */
@Immutable
data class VxColors(
    val paper: Color,
    val wash: Color,
    val card: Color,
    val ink: Color,
    val onInk: Color,
    val inkSoft: Color,
    val night: Color,
    val graphite: Color,
    val hair: Color,
    val leaf: Color,
    val amber: Color,
    val readyBg: Color,
    val readyText: Color,
    val warnBg: Color,
    val warnText: Color,
    val notText: Color,
    val danger: Color,
    val dangerBg: Color,
    val hintText: Color,
    val switchOff: Color,
    // The practice pad (dark in both themes).
    val pad: Color,
    val padText: Color,
    val padMuted: Color,
    val padInk: Color,
    val dark: Boolean,
)

val LightVx = VxColors(
    paper = Color(0xFFF7F3EC), wash = Color(0xFFEFE6DA), card = Color(0xFFFFFDF9),
    ink = Color(0xFF8F4C2C), onInk = Color.White, inkSoft = Color(0xFFE9D5C6),
    night = Color(0xFF1E1A17), graphite = Color(0xFF6B625A), hair = Color(0xFFE2D8CB),
    leaf = Color(0xFF3F7A55), amber = Color(0xFFA9701B),
    readyBg = Color(0xFFE4EFE6), readyText = Color(0xFF24503A),
    warnBg = Color(0xFFF6E7CC), warnText = Color(0xFF7A4F0E), notText = Color(0xFF5B4636),
    danger = Color(0xFF9B2C1F), dangerBg = Color(0xFFF6E1DC), hintText = Color(0xFF4A3D33),
    switchOff = Color(0xFFD6CCBF),
    pad = Color(0xFF1E1A17), padText = Color(0xFFFBF5EC), padMuted = Color(0xFFC9B9A8), padInk = Color(0xFFE2B48F),
    dark = false,
)

val DarkVx = VxColors(
    paper = Color(0xFF141110), wash = Color(0xFF24201C), card = Color(0xFF1C1815),
    ink = Color(0xFFE2B48F), onInk = Color(0xFF1C1815), inkSoft = Color(0xFF3B2E25),
    night = Color(0xFFEFE6DA), graphite = Color(0xFFA99886), hair = Color(0xFF2C2621),
    leaf = Color(0xFF7DBF94), amber = Color(0xFFDDA75A),
    readyBg = Color(0xFF1F3326), readyText = Color(0xFFA9D8B7),
    warnBg = Color(0xFF3A2C14), warnText = Color(0xFFE8C58A), notText = Color(0xFFC9B9A8),
    danger = Color(0xFFEC9A8B), dangerBg = Color(0xFF3A1F1A), hintText = Color(0xFFC9B9A8),
    switchOff = Color(0xFF4A4038),
    pad = Color(0xFF26211D), padText = Color(0xFFFBF5EC), padMuted = Color(0xFFA99886), padInk = Color(0xFFE2B48F),
    dark = true,
)

val LocalVx = staticCompositionLocalOf { LightVx }

object Fonts {
    /** Wordmark, screen titles, the dictation text. Only the 600 weight is bundled. */
    val Fraunces = FontFamily(Font(R.font.fraunces, FontWeight.SemiBold))
    val Figtree = FontFamily(
        Font(R.font.figtree_regular, FontWeight.Normal),
        Font(R.font.figtree_medium, FontWeight.Medium),
        Font(R.font.figtree_semibold, FontWeight.SemiBold),
    )
}

/** Shorthand for the current palette. */
val vx: VxColors @Composable get() = LocalVx.current

@Composable
fun VoxaTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val c = if (dark) DarkVx else LightVx
    val scheme = if (dark) {
        darkColorScheme(
            primary = c.ink, onPrimary = c.onInk, primaryContainer = c.inkSoft, onPrimaryContainer = c.night,
            secondaryContainer = c.inkSoft, onSecondaryContainer = c.night,
            background = c.paper, onBackground = c.night, surface = c.paper, onSurface = c.night,
            surfaceVariant = c.wash, onSurfaceVariant = c.graphite,
            surfaceContainerLowest = c.card, surfaceContainerLow = c.card, surfaceContainer = c.card,
            surfaceContainerHigh = c.card, surfaceContainerHighest = c.wash,
            outline = c.graphite, outlineVariant = c.hair, error = c.danger, errorContainer = c.dangerBg,
        )
    } else {
        lightColorScheme(
            primary = c.ink, onPrimary = c.onInk, primaryContainer = c.inkSoft, onPrimaryContainer = c.night,
            secondaryContainer = c.inkSoft, onSecondaryContainer = c.night,
            background = c.paper, onBackground = c.night, surface = c.paper, onSurface = c.night,
            surfaceVariant = c.wash, onSurfaceVariant = c.graphite,
            surfaceContainerLowest = c.card, surfaceContainerLow = c.card, surfaceContainer = c.card,
            surfaceContainerHigh = c.card, surfaceContainerHighest = c.wash,
            outline = c.graphite, outlineVariant = c.hair, error = c.danger, errorContainer = c.dangerBg,
        )
    }
    val base = Typography()
    val f = Fonts.Figtree
    val type = Typography(
        displayLarge = base.displayLarge.copy(fontFamily = f), displayMedium = base.displayMedium.copy(fontFamily = f),
        displaySmall = base.displaySmall.copy(fontFamily = f), headlineLarge = base.headlineLarge.copy(fontFamily = f),
        headlineMedium = base.headlineMedium.copy(fontFamily = f), headlineSmall = base.headlineSmall.copy(fontFamily = Fonts.Fraunces, fontWeight = FontWeight.SemiBold),
        titleLarge = base.titleLarge.copy(fontFamily = f, fontWeight = FontWeight.SemiBold), titleMedium = base.titleMedium.copy(fontFamily = f, fontWeight = FontWeight.SemiBold),
        titleSmall = base.titleSmall.copy(fontFamily = f, fontWeight = FontWeight.SemiBold),
        bodyLarge = base.bodyLarge.copy(fontFamily = f, fontSize = 15.sp, lineHeight = 21.sp),
        bodyMedium = base.bodyMedium.copy(fontFamily = f, fontSize = 14.sp, lineHeight = 20.sp),
        bodySmall = base.bodySmall.copy(fontFamily = f),
        labelLarge = base.labelLarge.copy(fontFamily = f, fontWeight = FontWeight.SemiBold),
        labelMedium = base.labelMedium.copy(fontFamily = f), labelSmall = base.labelSmall.copy(fontFamily = f),
    )
    CompositionLocalProvider(LocalVx provides c) {
        MaterialTheme(colorScheme = scheme, typography = type) {
            CompositionLocalProvider(LocalTextStyle provides TextStyle(fontFamily = f, color = c.night, fontSize = 15.sp), content = content)
        }
    }
}
