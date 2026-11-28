package com.voxa.android.ui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** The ChatGPT profile picture, downloaded once and kept small (192 px) in the app's storage. */
object AvatarCache {
    private const val PX = 192
    private var memo: Pair<String, ImageBitmap>? = null

    private fun file(c: Context) = File(c.filesDir, "avatar.png")
    private fun urlFile(c: Context) = File(c.filesDir, "avatar.url")

    /** Already downloaded for this URL (memory, then disk), else null. Cheap: no network. */
    fun cached(c: Context, url: String): ImageBitmap? {
        if (url.isEmpty()) return null
        memo?.let { if (it.first == url) return it.second }
        if (urlFile(c).takeIf { it.exists() }?.readText() != url) return null
        val bmp = BitmapFactory.decodeFile(file(c).path) ?: return null
        return bmp.asImageBitmap().also { memo = url to it }
    }

    /** Download, shrink and save the picture. Run off the main thread. Null on any failure. */
    fun load(c: Context, url: String): ImageBitmap? = runCatching {
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 8000; readTimeout = 8000
        }
        val bytes = conn.inputStream.use { it.readBytes() }
        conn.disconnect()
        val full = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null
        val small = Bitmap.createScaledBitmap(full, PX, PX * full.height / full.width.coerceAtLeast(1), true)
        file(c).outputStream().use { small.compress(Bitmap.CompressFormat.PNG, 100, it) }
        urlFile(c).writeText(url)
        if (small !== full) full.recycle()
        small.asImageBitmap().also { memo = url to it }
    }.getOrNull()

    /** Forget the picture (sign out). */
    fun clear(c: Context) {
        memo = null
        file(c).delete()
        urlFile(c).delete()
    }
}
