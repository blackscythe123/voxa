// Fixture for tools/lint (contrast): not compiled.

class KbColors {
    companion object {
        const val LIGHT_FG = 0xFFF2F2F0.toInt()
        const val DARK_FG = 0xFF141414.toInt()
        private const val WHITE = 0xFFFFFFFF.toInt()
        private const val SEPIA = 0xFF7A3F22.toInt()
        // Bad: the light sepia kept in dark mode (2.3:1 on the dark keyboard, mic/surface).
        private const val SEPIA_DARK = 0xFF7A3F22.toInt()
        // Bad: light secondary text #808080 is 3.5:1 on #F3F3F1 (muted/surface).
        private fun mutedFor(dark: Boolean) = if (dark) 0xFFB8B8B6.toInt() else 0xFF808080.toInt()
        const val GOOD_TEXT_LIGHT = 0xFF17743F.toInt()

        fun voxa(dark: Boolean, look: KeyboardLook.Look): KbColors {
            val surface = if (dark) 0xFF1A1A19.toInt() else 0xFFF3F3F1.toInt()
            val text = if (dark) LIGHT_FG else DARK_FG
            val custom = look.micColor != 0
            val accent = if (custom) look.micColor else if (dark) SEPIA_DARK else SEPIA
            // Bad (dark): near-black icon on the dark-mode sepia above (mic icon/mic).
            val onAccent = if (custom) micIcon(accent) else if (dark) 0xFF24140C.toInt() else WHITE
            return KbColors(
                surface = surface,
                key = if (!look.keyShapes) 0 else if (dark) 0xFF2E2E2C.toInt() else WHITE,
                text = text, muted = mutedFor(dark),
                accent = accent, onAccent = onAccent,
                good = if (dark) 0xFF45C27C.toInt() else 0xFF1F9D55.toInt(),
                goodTitle = if (dark) 0xFF45C27C.toInt() else GOOD_TEXT_LIGHT,
                recording = if (dark) 0xFFF2555A.toInt() else 0xFFE5484D.toInt(),
            )
        }
    }
}
