// SPDX-License-Identifier: GPL-3.0-only
package helium314.keyboard.latin.utils

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight

// Voxa fork: replaced HeliBoard's dynamic Material You theme with the Voxa
// "Quiet Sheet" palette so the settings activity matches the rest of the app.
// VoxaColors values copied here so the :keyboard library doesn't need to depend
// on :app (which would create a dependency cycle).
private val VoxaPrimary       = Color(0xFF1F4FE0)
private val VoxaSurface       = Color(0xFFFFFFFF)
private val VoxaSurfaceAlt    = Color(0xFFF5F5F5)
private val VoxaBg            = Color(0xFFFAFAFA)
private val VoxaInk           = Color(0xFF16161A)
private val VoxaMuted         = Color(0xFF7A7A82)
private val VoxaHair          = Color(0xFFE5E5E5)
private val VoxaDestructive   = Color(0xFFE5484D)

private val VoxaLightScheme = lightColorScheme(
    primary = VoxaPrimary,
    onPrimary = VoxaSurface,
    primaryContainer = Color(0xFFE6ECFB),
    onPrimaryContainer = VoxaPrimary,
    secondary = VoxaPrimary,
    background = VoxaBg,
    onBackground = VoxaInk,
    surface = VoxaSurface,
    onSurface = VoxaInk,
    surfaceVariant = VoxaSurfaceAlt,
    onSurfaceVariant = VoxaMuted,
    outline = VoxaHair,
    error = VoxaDestructive,
)

private val VoxaDarkScheme = darkColorScheme(
    primary = Color(0xFF8FAEF7),
    onPrimary = Color(0xFF0B1B45),
    background = Color(0xFF101114),
    onBackground = Color(0xFFE9EAEC),
    surface = Color(0xFF18191D),
    onSurface = Color(0xFFE9EAEC),
    surfaceVariant = Color(0xFF24262B),
    onSurfaceVariant = Color(0xFFA9ABB1),
    outline = Color(0xFF2A2C32),
    error = VoxaDestructive,
)

@Composable
fun Theme(dark: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val material3 = Typography()
    val colorScheme = if (dark) VoxaDarkScheme else VoxaLightScheme
    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography(
            titleLarge = material3.titleLarge.copy(fontWeight = FontWeight.SemiBold),
            titleMedium = material3.titleMedium.copy(fontWeight = FontWeight.SemiBold),
            titleSmall = material3.titleSmall.copy(fontWeight = FontWeight.SemiBold)
        ),
        content = content
    )
}

const val previewDark = true
