package com.voxa.android.ui

import android.content.Context
import android.content.Intent
import android.util.LruCache
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.graphics.drawable.toBitmap
import com.voxa.android.R

/**
 * App badges for History. History keeps the app's label ("WhatsApp"), so the label is matched to a
 * launchable app on the phone and its real icon used; apps the phone doesn't have get a brand tile
 * (the mockup's colours) or their initial.
 */
object AppIcons {
    class Brand(val color: Long, val glyph: Int)

    private val brands = mapOf(
        "whatsapp" to Brand(0xFF25D366, R.drawable.app_whatsapp),
        "gmail" to Brand(0xFFEA4335, R.drawable.app_gmail),
        "chatgpt" to Brand(0xFF0F0F0F, R.drawable.app_chatgpt),
        "slack" to Brand(0xFF4A154B, R.drawable.app_slack),
        "telegram" to Brand(0xFF2AABEE, R.drawable.app_telegram),
    )

    fun brand(label: String): Brand? = brands[label.trim().lowercase()]

    private val icons = LruCache<String, LoadedIcon>(48)
    @Volatile private var packages: Map<String, String>? = null

    fun cached(label: String): LoadedIcon? = icons.get(label)

    /** The real icon for an app label, or a holder with null when the phone has no such app.
     *  Call off the main thread. */
    fun load(context: Context, label: String): LoadedIcon {
        icons.get(label)?.let { return it }
        val pm = context.packageManager
        val pkg = labels(context)[label.trim().lowercase()]
        val px = (40 * context.resources.displayMetrics.density).toInt() * 14 / 10
        val bitmap = pkg?.let { runCatching { pm.getApplicationIcon(it).toBitmap(px, px).asImageBitmap() }.getOrNull() }
        return LoadedIcon(bitmap).also { icons.put(label, it) }
    }

    /** Launchable apps by lower-cased label (the manifest's <queries> makes them visible). */
    private fun labels(context: Context): Map<String, String> {
        packages?.let { return it }
        val pm = context.packageManager
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val map = HashMap<String, String>()
        for (ri in runCatching { pm.queryIntentActivities(launcher, 0) }.getOrDefault(emptyList())) {
            val ai = ri.activityInfo?.applicationInfo ?: continue
            map.putIfAbsent(pm.getApplicationLabel(ai).toString().trim().lowercase(), ai.packageName)
            map.putIfAbsent(ri.loadLabel(pm).toString().trim().lowercase(), ai.packageName)
        }
        return map.also { packages = it }
    }
}
