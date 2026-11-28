package com.voxa.android.ime

import android.content.Context
import android.graphics.Color
import androidx.core.graphics.ColorUtils
import com.voxa.android.ui.ThemePref
import kotlin.math.pow

/**
 * The keyboard's colours for one look (see [KeyboardLook]). The Voxa look is the v6 palette
 * (docs/ui-mocks/v6-app-redesign.html), light or dark with Appearance; the others compute their
 * key and text colours from one base colour (or from the photo's brightness grid) so text always
 * reads. Three areas can differ over a photo: the status line, the mic stage and the keys.
 */
class KbColors(
    /** The background: the solid colour, or the wash over the photo. */
    val surface: Int,
    /** True when a photo is drawn under [surface] at [washAlpha]. */
    val photo: Boolean,
    val washAlpha: Float,
    /** Key fill; 0 means no key shapes. */
    val key: Int,
    /** The 1 dp key elevation: the Voxa light look only. */
    val keyElevation: Boolean,
    /** Status line: title and timer, and the secondary text (detail, close). */
    val text: Int,
    val muted: Int,
    /** Around the mic: "That's N seconds of typing saved". */
    val stageMuted: Int,
    /** Key icons and the space label. */
    val keyText: Int,
    /** The mic (and the action pill), and the icon on it. */
    val accent: Int,
    val onAccent: Int,
    val micRing: Int,
    val micGlow: Int,
    val bars: Int,
    val spinner: Int,
    val good: Int,
    /** The "Pasted in X" title: green, or the text colour when green would not read. */
    val goodTitle: Int,
    val recording: Int,
    val press: Int,
    /** The background is dark (status pair, check ring). */
    val dark: Boolean,
    /** The look this was built from (position and soften for the photo). */
    val look: KeyboardLook.Look,
) {
    fun alpha(color: Int, a: Float) = Color.argb((a * 255).toInt(), Color.red(color), Color.green(color), Color.blue(color))

    /** Everything the panel is built from except the wash's opacity: equal keys need no new panel. */
    val buildKey: List<Any> get() = listOf(
        surface, photo, key, keyElevation, text, muted, stageMuted, keyText, accent, onAccent,
        micRing, micGlow, bars, spinner, good, goodTitle, recording, press, dark,
    )

    companion object {
        const val LIGHT_FG = 0xFFF2F2F0.toInt()
        const val DARK_FG = 0xFF141414.toInt()
        private const val WHITE = 0xFFFFFFFF.toInt()
        private const val SEPIA = 0xFF8F4C2C.toInt()
        private const val SEPIA_DARK = 0xFFD9946D.toInt()
        /** Green that reads as text on the light surfaces (5.2:1 on #F3F3F1). */
        const val GOOD_TEXT_LIGHT = 0xFF17743F.toInt()

        /** The look saved in Settings; the Voxa look (and the saved choice cleared) on any failure. */
        fun of(context: Context): KbColors {
            val dark = runCatching { ThemePref.isDark(context) }.getOrDefault(false)
            return try {
                forLook(context, KeyboardLook.load(context), dark)
            } catch (t: Throwable) {
                KeyboardLook.fallback(context, "colours", t)
                voxa(dark, KeyboardLook.Look())
            }
        }

        /** Throws when the photo's files can't be read (the caller falls back). */
        fun forLook(context: Context, look: KeyboardLook.Look, dark: Boolean): KbColors = when (look.type) {
            KeyboardLook.PHONE -> if (KeyboardLook.phoneColorsAvailable) phone(context, dark, look) else voxa(dark, look)
            KeyboardLook.COLOR -> solid(look.color, look)
            KeyboardLook.PHOTO -> Photo.colors(KeyboardLook.readGrid(context), look, dark, Photo.aspect(context))
            else -> voxa(dark, look)
        }

        /** The approved v6 look, exactly as before, plus the Key shapes and Mic colour choices. */
        fun voxa(dark: Boolean, look: KeyboardLook.Look): KbColors {
            val surface = if (dark) 0xFF1A1A19.toInt() else 0xFFF3F3F1.toInt()
            val text = if (dark) LIGHT_FG else DARK_FG
            val custom = look.micColor != 0
            val accent = if (custom) look.micColor else if (dark) SEPIA_DARK else SEPIA
            val onAccent = if (custom) micIcon(accent) else if (dark) 0xFF24140C.toInt() else WHITE
            return KbColors(
                surface = surface, photo = false, washAlpha = 1f,
                key = if (!look.keyShapes) 0 else if (dark) 0xFF2E2E2C.toInt() else WHITE,
                keyElevation = !dark && look.keyShapes,
                // Secondary text: 56 % reads at 5.6:1 on the dark surface, but only 4.0:1 on the light
                // one, so light uses 62 % (4.95:1 on #F3F3F1, WCAG AA).
                text = text, muted = withAlpha(text, if (dark) 0.56f else 0.62f), stageMuted = withAlpha(text, if (dark) 0.56f else 0.62f), keyText = withAlpha(text, 0.72f),
                accent = accent, onAccent = onAccent,
                micRing = ring(accent, surface, text, dark), micGlow = withAlpha(accent, if (dark) 0.25f else 0.35f),
                bars = withAlpha(accent, 0.3f), spinner = accent,
                good = if (dark) 0xFF45C27C.toInt() else 0xFF1F9D55.toInt(),
                // The 16sp title needs 4.5:1: #1F9D55 is only 3.1:1 on #F3F3F1, #17743F is 5.2:1.
                goodTitle = if (dark) 0xFF45C27C.toInt() else GOOD_TEXT_LIGHT,
                recording = if (dark) 0xFFF2555A.toInt() else 0xFFE5484D.toInt(),
                press = if (dark) 0x33FFFFFF else 0x1F000000, dark = dark, look = look,
            )
        }

        /** Android 12+: the system's neutral and accent tones; the mic takes the accent. */
        private fun phone(c: Context, dark: Boolean, look: KeyboardLook.Look): KbColors {
            val surface = c.getColor(if (dark) android.R.color.system_neutral1_900 else android.R.color.system_neutral1_50)
            val key = c.getColor(if (dark) android.R.color.system_neutral1_800 else android.R.color.system_neutral1_10)
            val accent = if (look.micColor != 0) look.micColor else c.getColor(if (dark) android.R.color.system_accent1_200 else android.R.color.system_accent1_600)
            return fromBase(surface, key, accent, look)
        }

        /** A solid colour: keys 7 % lighter (darker on very light colours), text by contrast. */
        fun solid(base: Int, look: KeyboardLook.Look): KbColors {
            val hsl = FloatArray(3)
            ColorUtils.colorToHSL(base, hsl)
            hsl[2] = if (hsl[2] > 0.9f) hsl[2] - 0.07f else minOf(1f, hsl[2] + 0.07f)
            val key = ColorUtils.HSLToColor(hsl)
            return fromBase(base, key, look.micColor, look)
        }

        private fun fromBase(surface: Int, key: Int, mic: Int, look: KeyboardLook.Look): KbColors {
            // The text colour that reads best on both the background and the keys.
            val fg = listOf(LIGHT_FG, DARK_FG).maxBy { minOf(Contrast.ratio(it, surface), Contrast.ratio(it, key)) }
            val dark = fg == LIGHT_FG
            val accent = if (mic != 0) mic else if (dark) SEPIA_DARK else SEPIA
            val good = if (dark) 0xFF45C27C.toInt() else 0xFF1F9D55.toInt()
            return KbColors(
                surface = surface, photo = false, washAlpha = 1f,
                key = if (look.keyShapes) key else 0, keyElevation = false,
                text = fg, muted = withAlpha(fg, 0.8f), stageMuted = withAlpha(fg, 0.8f), keyText = withAlpha(fg, 0.8f),
                accent = accent, onAccent = micIcon(accent),
                micRing = ring(accent, surface, fg, dark), micGlow = withAlpha(accent, if (dark) 0.25f else 0.35f),
                bars = withAlpha(accent, 0.3f), spinner = accent,
                good = good, goodTitle = listOf(good, GOOD_TEXT_LIGHT).firstOrNull { Contrast.ratio(it, surface) >= 4.5 } ?: fg,
                recording = if (dark) 0xFFF2555A.toInt() else 0xFFE5484D.toInt(),
                press = if (dark) 0x33FFFFFF else 0x1F000000, dark = dark, look = look,
            )
        }

        /** White or near-black on the mic, whichever gives more contrast. */
        fun micIcon(mic: Int) = if (Contrast.ratio(WHITE, mic) >= Contrast.ratio(DARK_FG, mic)) WHITE else DARK_FG

        /** The soft accent ring, or a text-coloured ring when the mic would not stand out (below 3:1). */
        fun ring(mic: Int, around: Int, fg: Int, dark: Boolean) =
            if (Contrast.ratio(mic, around) < 3.0) withAlpha(fg, 0.85f) else withAlpha(mic, if (dark) 0.22f else 0.12f)

        fun withAlpha(color: Int, a: Float) = Color.argb((a * 255).toInt(), Color.red(color), Color.green(color), Color.blue(color))
    }
}

/** WCAG contrast on sRGB colours, and the grey with a colour's luminance. */
object Contrast {
    fun lin(v: Float) = if (v <= 0.04045f) v / 12.92f else ((v + 0.055f) / 1.055f).pow(2.4f)
    private fun srgb(y: Float) = if (y <= 0.0031308f) y * 12.92f else 1.055f * y.pow(1f / 2.4f) - 0.055f

    /** Relative luminance (alpha ignored). */
    fun lum(c: Int) = 0.2126f * lin(Color.red(c) / 255f) + 0.7152f * lin(Color.green(c) / 255f) + 0.0722f * lin(Color.blue(c) / 255f)

    /** The sRGB grey (0..1) with [c]'s luminance. */
    fun grey(c: Int) = srgb(lum(c))

    /** The sRGB grey (0..1) with relative luminance [y]. */
    fun greyOfLum(y: Float) = srgb(y)

    fun ratioLum(a: Float, b: Float) = (maxOf(a, b) + 0.05f) / (minOf(a, b) + 0.05f)
    fun ratio(a: Int, b: Int) = ratioLum(lum(a), lum(b))
    /** [c] against a grey given as sRGB 0..1. */
    fun ratioGrey(c: Int, grey: Float) = ratioLum(lum(c), lin(grey))
    fun greyRatio(a: Float, b: Float) = ratioLum(lin(a), lin(b))
}

/**
 * Readability over a photo. The panel is split in three bands (status line, around the mic, the
 * keys); each picks near-white or near-black against its worst cells, under a wash toward a dark
 * or light surface. Each cell keeps its darkest and brightest pixels (not an average, so small
 * bright details under a label count), a band is judged at its 2nd / 98th percentile cell, and
 * every minimum gets a 10 % margin. The Photo strength slider stops where any band would stop reading.
 */
object Photo {
    /** The panel's height used for the crop (it is 296 dp plus the navigation bar). */
    const val PANEL_DP = 320f
    private val STATUS = 16f to 92f
    private val STAGE = 84f to 224f
    private val KEYS = 224f to 272f
    const val DARK_WASH = 0xFF141414.toInt()
    const val LIGHT_WASH = 0xFFF2F2F0.toInt()

    /** Text at 100 % and the secondary text at 80 %: 4.5:1 (WCAG 1.4.3). */
    private val STATUS_NEEDS = listOf(1f to 4.5f, 0.8f to 4.5f)
    /** "That's N seconds of typing saved" (4.5:1) and the level bars at 50 % (3:1, WCAG 1.4.11). */
    private val STAGE_NEEDS = listOf(0.8f to 4.5f, 0.5f to 3f)
    /** The space label ("Pause", "Resume", "space") and the key icons, at 90 %. */
    private val KEY_NEEDS = listOf(0.9f to 4.5f)

    /** The panel's width over [PANEL_DP] (the crop's shape). */
    fun aspect(c: Context): Float {
        val m = c.resources.displayMetrics
        return m.widthPixels / (PANEL_DP * m.density)
    }

    /** The wash's opacity: the rest of Photo strength, 20 points stronger when dark. */
    fun washAlpha(strength: Float, dark: Boolean) = (1f - strength + if (dark) 0.2f else 0f).coerceIn(0f, 1f)

    /** The band's darkest and brightest cells (2nd / 98th percentile), before the wash. */
    class Band(val lo: Float, val hi: Float)

    private fun band(g: KeyboardLook.Grid, aspect: Float, position: Float, range: Pair<Float, Float>): Band {
        // Centre-crop the photo to the panel's shape: the visible part, in 0..1 photo units.
        val vw: Float
        val vh: Float
        if (g.aspect > aspect) { vw = aspect / g.aspect; vh = 1f } else { vw = 1f; vh = g.aspect / aspect }
        val x0 = (1f - vw) / 2f
        val y0 = (1f - vh) * position.coerceIn(0f, 1f)
        val top = y0 + vh * range.first / PANEL_DP
        val bottom = y0 + vh * range.second / PANEL_DP
        val r0 = (top * g.h).toInt().coerceIn(0, g.h - 1)
        val r1 = ((bottom * g.h).toInt()).coerceIn(r0 + 1, g.h)
        val c0 = (x0 * g.w).toInt().coerceIn(0, g.w - 1)
        val c1 = (((x0 + vw) * g.w).toInt()).coerceIn(c0 + 1, g.w)
        val n = (r1 - r0) * (c1 - c0)
        val lo = FloatArray(n)
        val hi = FloatArray(n)
        var i = 0
        for (r in r0 until r1) for (c in c0 until c1) {
            lo[i] = g.lo[r * g.w + c]
            hi[i++] = g.hi[r * g.w + c]
        }
        lo.sort()
        hi.sort()
        return Band(lo[((n - 1) * 0.02f).toInt()], hi[((n - 1) * 0.98f).toInt()])
    }

    private fun bands(g: KeyboardLook.Grid, aspect: Float, position: Float) =
        listOf(band(g, aspect, position, STATUS), band(g, aspect, position, STAGE), band(g, aspect, position, KEYS))

    /** Dark wash (light text) or light wash, from the whole visible photo. */
    private fun washDark(g: KeyboardLook.Grid, aspect: Float, position: Float): Boolean {
        val all = band(g, aspect, position, 0f to PANEL_DP)
        return Contrast.ratioGrey(LIGHT_FG, all.hi) >= Contrast.ratioGrey(DARK_FG, all.lo)
    }

    /** Headroom on every minimum: the grid is coarse and the photo is drawn scaled. */
    private const val MARGIN = 1.1f

    private const val LIGHT_FG = KbColors.LIGHT_FG
    private const val DARK_FG = KbColors.DARK_FG

    /** How well [fg] meets [needs] on the band under the wash: >= 1 passes. */
    private fun score(fg: Int, b: Band, wash: Float, a: Float, needs: List<Pair<Float, Float>>, keys: Boolean): Float {
        val f = Contrast.grey(fg)
        val light = f > 0.5f
        // The worst cells for this text colour: the brightest for light text, the darkest for dark.
        var bg = a * wash + (1f - a) * (if (light) b.hi else b.lo)
        // See-through keys: the text colour at 18 % over the photo.
        if (keys) bg = 0.18f * f + 0.82f * bg
        var worst = Float.MAX_VALUE
        for ((alpha, need) in needs) {
            val e = alpha * f + (1f - alpha) * bg
            worst = minOf(worst, Contrast.greyRatio(e, bg) / (need * MARGIN))
        }
        return worst
    }

    private fun pick(b: Band, wash: Float, a: Float, needs: List<Pair<Float, Float>>, keys: Boolean): Pair<Int, Float> {
        val l = score(LIGHT_FG, b, wash, a, needs, keys)
        val d = score(DARK_FG, b, wash, a, needs, keys)
        return if (l >= d) LIGHT_FG to l else DARK_FG to d
    }

    /** The highest Photo strength at which every band still reads (0..1, steps of 0.02). */
    fun maxStrength(g: KeyboardLook.Grid, position: Float, dark: Boolean, keyShapes: Boolean, aspect: Float): Float {
        val bs = bands(g, aspect, position)
        val wash = Contrast.grey(if (washDark(g, aspect, position)) DARK_WASH else LIGHT_WASH)
        for (step in 50 downTo 0) {
            val s = step / 50f
            val a = washAlpha(s, dark)
            val ok = pick(bs[0], wash, a, STATUS_NEEDS, false).second >= 1f &&
                pick(bs[1], wash, a, STAGE_NEEDS, false).second >= 1f &&
                pick(bs[2], wash, a, KEY_NEEDS, keyShapes).second >= 1f
            if (ok) return s
        }
        return 0f
    }

    fun colors(g: KeyboardLook.Grid, look: KeyboardLook.Look, dark: Boolean, aspect: Float): KbColors {
        val cap = maxStrength(g, look.position, dark, look.keyShapes, aspect)
        val a = washAlpha(minOf(look.strength, cap), dark)
        val washIsDark = washDark(g, aspect, look.position)
        val washColor = if (washIsDark) DARK_WASH else LIGHT_WASH
        val wash = Contrast.grey(washColor)
        val bs = bands(g, aspect, look.position)
        val status = pick(bs[0], wash, a, STATUS_NEEDS, false).first
        val stage = pick(bs[1], wash, a, STAGE_NEEDS, false).first
        val keyFg = pick(bs[2], wash, a, KEY_NEEDS, look.keyShapes).first
        val w = KbColors::withAlpha
        // Status colours are fixed pairs: the dark-palette pair over a dark wash, the light pair over a light one.
        val good = if (washIsDark) 0xFF45C27C.toInt() else 0xFF1F9D55.toInt()
        val goodText = if (washIsDark) good else KbColors.GOOD_TEXT_LIGHT
        val sb = bs[0]
        val goodReads = minOf(
            Contrast.ratioGrey(goodText, a * wash + (1f - a) * sb.lo),
            Contrast.ratioGrey(goodText, a * wash + (1f - a) * sb.hi),
        ) >= 4.5f
        val stageLight = stage == LIGHT_FG
        val mic = if (look.micColor != 0) look.micColor else if (stageLight) 0xFFD9946D.toInt() else 0xFF8F4C2C.toInt()
        val st = bs[1]
        val micReads = minOf(
            Contrast.ratioGrey(mic, a * wash + (1f - a) * st.lo),
            Contrast.ratioGrey(mic, a * wash + (1f - a) * st.hi),
        ) >= 3f
        return KbColors(
            surface = washColor, photo = true, washAlpha = a,
            key = if (look.keyShapes) w(keyFg, 0.18f) else 0, keyElevation = false,
            text = status, muted = w(status, 0.8f), stageMuted = w(stage, 0.8f), keyText = w(keyFg, 0.9f),
            accent = mic, onAccent = KbColors.micIcon(mic),
            micRing = if (micReads) w(mic, if (stageLight) 0.22f else 0.12f) else w(stage, 0.85f),
            micGlow = w(mic, if (stageLight) 0.25f else 0.35f),
            bars = w(stage, 0.5f), spinner = w(stage, 0.5f),
            good = good, goodTitle = if (goodReads) goodText else status,
            recording = if (washIsDark) 0xFFF2555A.toInt() else 0xFFE5484D.toInt(),
            press = if (keyFg == LIGHT_FG) 0x33FFFFFF else 0x1F000000, dark = washIsDark, look = look.copy(strength = minOf(look.strength, cap)),
        )
    }
}
