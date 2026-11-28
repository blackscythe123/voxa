package com.voxa.android.ui

import android.content.Context
import android.content.res.Configuration
import androidx.compose.runtime.mutableStateOf

/** Appearance chosen in Settings: follow the phone, or always light, or always dark. Used by the
 *  app and by the keyboard. */
object ThemePref {
    const val SYSTEM = "system"
    const val LIGHT = "light"
    const val DARK = "dark"
    private const val KEY = "theme"

    /** Observed by the app's theme so a change applies at once. */
    val current = mutableStateOf(SYSTEM)

    fun load(c: Context): String =
        c.getSharedPreferences("ui", Context.MODE_PRIVATE).getString(KEY, SYSTEM)!!.also { current.value = it }

    fun set(c: Context, value: String) {
        c.getSharedPreferences("ui", Context.MODE_PRIVATE).edit().putString(KEY, value).apply()
        current.value = value
    }

    fun label(value: String) = when (value) {
        LIGHT -> "Light"
        DARK -> "Dark"
        else -> "Same as phone"
    }

    fun next(value: String) = when (value) {
        SYSTEM -> LIGHT
        LIGHT -> DARK
        else -> SYSTEM
    }

    /** Dark or not, given what the phone says. */
    fun isDark(value: String, systemDark: Boolean) = when (value) {
        LIGHT -> false
        DARK -> true
        else -> systemDark
    }

    /** For the keyboard (plain Views): reads the saved choice and the phone's mode. */
    fun isDark(c: Context): Boolean {
        val system = (c.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        return isDark(load(c), system)
    }
}
