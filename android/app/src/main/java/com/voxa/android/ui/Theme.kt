package com.voxa.android.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.voxa.android.R

/**
 * The v6 tokens (docs/ui-mocks/v6-app-redesign.html). 60/30/10: a neutral base, near-black text at
 * 100 / 72 / 56 %, and Voxa's sepia only for the mic and the primary actions. Dark mode keeps the
 * same roles on a neutral near-black, with the sepia lightened for contrast.
 */
@Immutable
data class VxColors(
    /** 60 %: the page. */
    val base: Color,
    /** Inputs, segmented control, action tiles, the keyboard. */
    val surface: Color,
    /** Cards: the base with a soft tinted shadow in light, a lifted grey in dark. */
    val card: Color,
    val text: Color,
    val accent: Color,
    val onAccent: Color,
    val good: Color,
    val warn: Color,
    val danger: Color,
    /** Soft shadow tint (light only; dark cards are lifted by colour instead). */
    val shadow: Color,
    val avatar: Color,
    val dark: Boolean,
) {
    val text72 get() = text.copy(alpha = 0.72f)
    val text56 get() = text.copy(alpha = 0.56f)
    val text40 get() = text.copy(alpha = 0.40f)
    val text16 get() = text.copy(alpha = 0.16f)
    val hair get() = text.copy(alpha = if (dark) 0.10f else 0.06f)
    val accent5 get() = accent.copy(alpha = if (dark) 0.12f else 0.06f)
    val accent12 get() = accent.copy(alpha = if (dark) 0.22f else 0.12f)
    val good12 get() = good.copy(alpha = if (dark) 0.18f else 0.10f)
    val warn10 get() = warn.copy(alpha = if (dark) 0.18f else 0.10f)
    val danger8 get() = danger.copy(alpha = if (dark) 0.16f else 0.08f)
}

val LightVx = VxColors(
    base = Color(0xFFFFFFFF),
    surface = Color(0xFFF3F3F1),
    card = Color(0xFFFFFFFF),
    text = Color(0xFF141414),
    accent = Color(0xFF8F4C2C),
    onAccent = Color(0xFFFFFFFF),
    good = Color(0xFF1F9D55),
    warn = Color(0xFFB26B00),
    danger = Color(0xFFC8281E),
    shadow = Color(0xFF3C2819),
    avatar = Color(0xFF384656),
    dark = false,
)

val DarkVx = VxColors(
    base = Color(0xFF121212),
    surface = Color(0xFF232321),
    card = Color(0xFF1C1C1B),
    text = Color(0xFFF2F2F0),
    accent = Color(0xFFD9946D),
    onAccent = Color(0xFF24140C),
    good = Color(0xFF45C27C),
    warn = Color(0xFFE3A44A),
    danger = Color(0xFFF07A70),
    shadow = Color(0xFF000000),
    avatar = Color(0xFF4A5868),
    dark = true,
)

val LocalVx = staticCompositionLocalOf { LightVx }

/** Shorthand for the current palette. */
val vx: VxColors @Composable get() = LocalVx.current

/** One typeface, Manrope, two weights (500 and 700, Latin subset). */
val Manrope = FontFamily(
    Font(R.font.manrope_medium, FontWeight.Medium),
    Font(R.font.manrope_bold, FontWeight.Bold),
)

/** Four sizes, 32 / 20 / 16 / 13, plus 12 for tiny captions and 40 for the one big number. */
object Type {
    private val base = TextStyle(fontFamily = Manrope, fontWeight = FontWeight.Medium)
    val Title = base.copy(fontSize = 32.sp, lineHeight = 40.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.025).em)
    val Big = base.copy(fontSize = 40.sp, lineHeight = 44.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.03).em, fontFeatureSettings = "tnum")
    val L = base.copy(fontSize = 20.sp, lineHeight = 28.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.01).em)
    val Said = base.copy(fontSize = 20.sp, lineHeight = 30.sp, letterSpacing = (-0.005).em)
    val M = base.copy(fontSize = 16.sp, lineHeight = 24.sp)
    val MBold = M.copy(fontWeight = FontWeight.Bold)
    val S = base.copy(fontSize = 13.sp, lineHeight = 18.sp)
    val SBold = S.copy(fontWeight = FontWeight.Bold)
    val XS = base.copy(fontSize = 12.sp, lineHeight = 16.sp)
    val XSBold = XS.copy(fontWeight = FontWeight.Bold)
}

@Composable
fun VoxaTheme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val c = if (dark) DarkVx else LightVx
    val scheme = (if (dark) darkColorScheme() else lightColorScheme()).copy(
        primary = c.accent, onPrimary = c.onAccent,
        background = c.base, onBackground = c.text, surface = c.base, onSurface = c.text,
        surfaceVariant = c.surface, onSurfaceVariant = c.text56,
        surfaceContainerLowest = c.card, surfaceContainerLow = c.card, surfaceContainer = c.card,
        surfaceContainerHigh = c.card, surfaceContainerHighest = c.surface,
        outline = c.text40, outlineVariant = c.hair, error = c.danger,
    )
    CompositionLocalProvider(LocalVx provides c) {
        MaterialTheme(colorScheme = scheme) {
            CompositionLocalProvider(LocalTextStyle provides Type.M.copy(color = c.text), content = content)
        }
    }
}
