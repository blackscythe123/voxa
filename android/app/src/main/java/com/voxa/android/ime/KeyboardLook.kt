package com.voxa.android.ime

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.Rect
import android.net.Uri
import android.os.Build
import com.voxa.android.diag.Diag
import java.io.File
import kotlin.math.roundToInt

/**
 * What the keyboard looks like (Settings, Keyboard look): the Voxa look, the phone's own colours,
 * one of the solid colours, or the user's photo under a wash. Stored in the "kb_look" prefs; the
 * app and the keyboard share one process, so a change is seen at once. Every change bumps
 * [version], and the keyboard rebuilds its view on the next show when the version moved.
 *
 * Every read of the photo or its files is guarded: on any failure the look falls back to Voxa
 * and the saved choice is cleared, because a crashing keyboard gets dropped by Android.
 */
object KeyboardLook {
    const val VOXA = "voxa"
    const val PHONE = "phone"
    const val COLOR = "color"
    const val PHOTO = "photo"

    private const val PREFS = "kb_look"

    data class Look(
        val type: String = VOXA,
        /** The solid colour (type [COLOR]). */
        val color: Int = 0,
        /** How much of the photo shows through the wash, 0..1 (capped by [Photo.maxStrength]). */
        val strength: Float = 0.6f,
        val soften: Boolean = false,
        /** The photo's up/down position, 0 (top) .. 1 (bottom). */
        val position: Float = 0.5f,
        val keyShapes: Boolean = true,
        /** 0: the default (Voxa sepia, or the phone's accent with Phone colours). */
        val micColor: Int = 0,
    )

    class Preset(val name: String, val color: Int)

    /** The solid colours (Voxa itself is the first swatch, not a preset). */
    val presets = listOf(
        Preset("Black", 0xFF000000.toInt()),
        Preset("Graphite", 0xFF2B2D31.toInt()),
        Preset("Paper", 0xFFF7F5F0.toInt()),
        Preset("Navy", 0xFF1B2A41.toInt()),
        Preset("Forest", 0xFF1F3A2E.toInt()),
        Preset("Plum", 0xFF3B2438.toInt()),
        Preset("Sand", 0xFFE6D8C3.toInt()),
    )

    class MicSwatch(val name: String, val color: Int)

    /** Mic colours after the default: the icon on it is white or near-black, whichever reads. */
    /** Each gives its icon (and the action pill's text) at least 4.5:1 ([KbColors.micIcon]). */
    val micColors = listOf(
        MicSwatch("Ocean", 0xFF2F6FDB.toInt()),
        MicSwatch("Teal", 0xFF0F7A6F.toInt()),
        MicSwatch("Rose", 0xFFB83A60.toInt()),
        MicSwatch("Violet", 0xFF7457E0.toInt()),
        MicSwatch("Amber", 0xFFD08A1E.toInt()),
    )

    /** Phone colours need Android 12 (the system's dynamic palette). */
    val phoneColorsAvailable get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

    private fun prefs(c: Context) = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun load(c: Context): Look {
        val p = prefs(c)
        val type = p.getString("type", VOXA) ?: VOXA
        return Look(
            type = if (type == PHONE && !phoneColorsAvailable) VOXA else type,
            color = p.getInt("color", 0),
            strength = p.getFloat("strength", 0.6f),
            soften = p.getBoolean("soften", false),
            position = p.getFloat("position", 0.5f),
            keyShapes = p.getBoolean("key_shapes", true),
            micColor = newerMic(p.getInt("mic_color", 0)),
        )
    }

    /** Teal and Rose were darkened to reach 4.5:1 (#138A7E and #D04870 gave only 4.4:1 and 4.3:1). */
    private fun newerMic(c: Int): Int = when (c) {
        0xFF138A7E.toInt() -> 0xFF0F7A6F.toInt()
        0xFFD04870.toInt() -> 0xFFB83A60.toInt()
        else -> c
    }

    fun save(c: Context, look: Look) {
        prefs(c).edit()
            .putString("type", look.type)
            .putInt("color", look.color)
            .putFloat("strength", look.strength)
            .putBoolean("soften", look.soften)
            .putFloat("position", look.position)
            .putBoolean("key_shapes", look.keyShapes)
            .putInt("mic_color", look.micColor)
            .putInt("version", version(c) + 1)
            .apply()
    }

    fun version(c: Context) = prefs(c).getInt("version", 0)

    /** Something the keyboard draws with changed (Appearance, too). */
    fun bump(c: Context) {
        prefs(c).edit().putInt("version", version(c) + 1).apply()
    }

    /**
     * What the keyboard's view was built from: the look's version, light or dark, and the phone's
     * accent (Phone colours follow a wallpaper change). A different stamp on show rebuilds the view.
     */
    fun stamp(c: Context): String {
        val look = runCatching { load(c) }.getOrNull()
        val phone = if (look?.type == PHONE && phoneColorsAvailable) {
            runCatching { c.getColor(android.R.color.system_accent1_500) }.getOrDefault(0)
        } else 0
        return "${version(c)}/${com.voxa.android.ui.ThemePref.isDark(c)}/$phone"
    }

    fun label(look: Look): String = when (look.type) {
        PHONE -> "Phone colours"
        COLOR -> presets.firstOrNull { it.color == look.color }?.name ?: "Colour"
        PHOTO -> "Your photo"
        else -> "Voxa"
    }

    /** Back to the Voxa look: deletes the photo copy and every saved choice. */
    fun reset(c: Context) {
        val v = version(c)
        photoFile(c).delete()
        gridFile(c).delete()
        prefs(c).edit().clear().putInt("version", v + 1).apply()
    }

    /** Deletes the photo copy; a photo look goes back to Voxa (the other choices stay). */
    fun removePhoto(c: Context) {
        photoFile(c).delete()
        gridFile(c).delete()
        val look = load(c)
        save(c, if (look.type == PHOTO) look.copy(type = VOXA) else look)
    }

    fun hasPhoto(c: Context) = photoFile(c).isFile

    /** Something about the look could not be read: the Voxa look, and say why in the log. */
    fun fallback(c: Context, why: String, t: Throwable? = null) {
        Diag.log("look", "fell back to the Voxa look: $why${t?.let { " (${it.javaClass.simpleName})" } ?: ""}")
        runCatching { reset(c) }
    }

    // ── the photo ────────────────────────────────────────────────────────────

    fun photoFile(c: Context) = File(c.filesDir, "kb_bg.webp")
    private fun gridFile(c: Context) = File(c.filesDir, "kb_bg.grid")

    /** The longest side of the private copy. */
    private const val COPY_MAX = 1440
    /** Columns of the brightness grid (rows follow the photo's shape: 24 for 4:3). */
    private const val GRID_W = 32

    /**
     * The photo's brightness, [w] x [h] cells, each the sRGB grey with the cell's luminance
     * (0..1). Kept for the whole photo, so any crop and position can be checked.
     */
    class Grid(val w: Int, val h: Int, val cells: FloatArray) {
        val aspect get() = w.toFloat() / h
    }

    /**
     * Copies the picked photo into private storage (lossy WebP, longest side at most 1440 px) and
     * keeps its brightness grid. Decodes at the target size (never full size; HEIC and EXIF
     * rotation are handled by ImageDecoder). Call off the main thread. False on any failure.
     */
    fun importPhoto(c: Context, uri: Uri): Boolean {
        var bmp: Bitmap? = null
        return try {
            val src = ImageDecoder.createSource(c.contentResolver, uri)
            bmp = ImageDecoder.decodeBitmap(src) { d, info, _ ->
                val w = info.size.width
                val h = info.size.height
                val s = minOf(1f, COPY_MAX.toFloat() / maxOf(w, h))
                d.setTargetSize(maxOf(1, (w * s).roundToInt()), maxOf(1, (h * s).roundToInt()))
                d.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            }
            val tmp = File(c.filesDir, "kb_bg.tmp")
            tmp.outputStream().use { out ->
                @Suppress("DEPRECATION")
                val fmt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Bitmap.CompressFormat.WEBP_LOSSY else Bitmap.CompressFormat.WEBP
                if (!bmp.compress(fmt, 80, out)) error("compress failed")
            }
            val grid = gridOf(bmp)
            writeGrid(c, grid)
            if (!tmp.renameTo(photoFile(c))) {
                tmp.copyTo(photoFile(c), overwrite = true)
                tmp.delete()
            }
            val dark = com.voxa.android.ui.ThemePref.isDark(c)
            val old = load(c)
            val cap = Photo.maxStrength(grid, 0.5f, dark, old.keyShapes, Photo.aspect(c))
            save(c, old.copy(type = PHOTO, position = 0.5f, strength = minOf(0.6f, cap)))
            Diag.log("look", "photo saved: ${bmp.width} x ${bmp.height}, ${photoFile(c).length() / 1024} KB")
            true
        } catch (t: Throwable) {
            Diag.log("look", "photo could not be used: ${t.javaClass.simpleName}")
            false
        } finally {
            bmp?.recycle()
        }
    }

    private fun gridOf(bmp: Bitmap): Grid {
        val gh = (GRID_W * bmp.height.toFloat() / bmp.width).roundToInt().coerceIn(8, 96)
        val small = Bitmap.createScaledBitmap(bmp, GRID_W, gh, true)
        val px = IntArray(GRID_W * gh)
        small.getPixels(px, 0, GRID_W, 0, 0, GRID_W, gh)
        if (small != bmp) small.recycle()
        return Grid(GRID_W, gh, FloatArray(px.size) { Contrast.grey(px[it]) })
    }

    private fun writeGrid(c: Context, g: Grid) {
        val bytes = ByteArray(2 + g.cells.size)
        bytes[0] = g.w.toByte()
        bytes[1] = g.h.toByte()
        for (i in g.cells.indices) bytes[2 + i] = (g.cells[i] * 255f).roundToInt().coerceIn(0, 255).toByte()
        gridFile(c).writeBytes(bytes)
    }

    /** The saved brightness grid; throws when it is missing or damaged (the caller falls back). */
    fun readGrid(c: Context): Grid {
        val b = gridFile(c).readBytes()
        val w = b[0].toInt() and 0xFF
        val h = b[1].toInt() and 0xFF
        require(w > 0 && h > 0 && b.size == 2 + w * h) { "bad grid" }
        if (!photoFile(c).isFile) error("photo copy missing")
        return Grid(w, h, FloatArray(w * h) { (b[2 + it].toInt() and 0xFF) / 255f })
    }

    /**
     * Decodes the private copy for a [w] x [h] px panel, centre-cropped at [position]; [soften]
     * keeps only a 1/8-size bitmap that the panel scales up (bilinear filtering is the blur).
     * [crop] false keeps the photo's full height (the app's preview, where it can be dragged).
     * Uses the default (hardware) allocator so the pixels stay off the Java heap. Throws on failure.
     */
    fun decodePhoto(c: Context, w: Int, h: Int, position: Float, soften: Boolean, crop: Boolean): Bitmap {
        val file = photoFile(c)
        if (!file.isFile) error("photo copy missing")
        val src = ImageDecoder.createSource(file)
        val div = if (soften) 8f else 1f
        val decode = { hardware: Boolean ->
            ImageDecoder.decodeBitmap(src) { d, info, _ ->
                val pw = info.size.width.toFloat()
                val ph = info.size.height.toFloat()
                val s = minOf(1f, maxOf(w / pw, h / ph)) / div
                val tw = maxOf(1, (pw * s).roundToInt())
                val th = maxOf(1, (ph * s).roundToInt())
                d.setTargetSize(tw, th)
                if (crop) {
                    val cw = minOf(tw, maxOf(1, (w / div).roundToInt()))
                    val ch = minOf(th, maxOf(1, (h / div).roundToInt()))
                    val x = (tw - cw) / 2
                    val y = ((th - ch) * position.coerceIn(0f, 1f)).roundToInt()
                    d.crop = Rect(x, y, x + cw, y + ch)
                }
                if (!hardware) d.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            }
        }
        return try {
            decode(true)
        } catch (e: OutOfMemoryError) {
            throw e
        } catch (e: Exception) {
            // Some GPUs refuse hardware bitmaps; the photo is small enough for the heap.
            decode(false)
        }
    }

    /** A small thumbnail of the private copy for the app's swatch (software, ~144 px). */
    fun thumbnail(c: Context, size: Int): Bitmap? = runCatching {
        val file = photoFile(c)
        if (!file.isFile) return null
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { d, info, _ ->
            val s = minOf(1f, size.toFloat() / minOf(info.size.width, info.size.height))
            d.setTargetSize(maxOf(1, (info.size.width * s).roundToInt()), maxOf(1, (info.size.height * s).roundToInt()))
            d.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
    }.getOrNull()
}
