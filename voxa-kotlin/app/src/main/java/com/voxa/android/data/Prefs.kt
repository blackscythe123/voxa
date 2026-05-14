package com.voxa.android.data

import android.content.Context
import android.content.SharedPreferences
import java.util.UUID

class Prefs(context: Context) {

    private val sp: SharedPreferences =
        context.getSharedPreferences("voxa_prefs", Context.MODE_PRIVATE)

    /** Legacy single-cookie storage (kept for compatibility) */
    fun getSessionCookie(): String? = sp.getString(KEY_SESSION_COOKIE, null)
    fun setSessionCookie(value: String) = sp.edit().putString(KEY_SESSION_COOKIE, value).apply()

    /** Full Cookie header captured from the WebView — includes cf_clearance, session-token, etc. */
    fun getCookieHeader(): String? = sp.getString(KEY_COOKIE_HEADER, null)
    fun setCookieHeader(value: String) = sp.edit().putString(KEY_COOKIE_HEADER, value).apply()

    fun clearSession() = sp.edit()
        .remove(KEY_SESSION_COOKIE)
        .remove(KEY_COOKIE_HEADER)
        .remove(KEY_ACCESS_TOKEN)
        .apply()

    fun getDeviceId(): String {
        var id = sp.getString(KEY_DEVICE_ID, null)
        if (id == null) {
            id = UUID.randomUUID().toString()
            sp.edit().putString(KEY_DEVICE_ID, id).apply()
        }
        return id
    }

    fun getOaiSessionId(): String {
        var id = sp.getString(KEY_OAI_SESSION_ID, null)
        if (id == null) {
            id = UUID.randomUUID().toString()
            sp.edit().putString(KEY_OAI_SESSION_ID, id).apply()
        }
        return id
    }

    fun getMaxDuration(): Int = sp.getInt(KEY_MAX_DURATION, 120)
    fun setMaxDuration(seconds: Int) = sp.edit().putInt(KEY_MAX_DURATION, seconds).apply()

    fun getAutoStart(): Boolean = sp.getBoolean(KEY_AUTO_START, false)
    fun setAutoStart(enabled: Boolean) = sp.edit().putBoolean(KEY_AUTO_START, enabled).apply()

    companion object {
        private const val KEY_SESSION_COOKIE = "voxa_session_cookie"
        private const val KEY_COOKIE_HEADER = "voxa_cookie_header"
        private const val KEY_ACCESS_TOKEN = "voxa_access_token"
        private const val KEY_DEVICE_ID = "voxa_device_id"
        private const val KEY_OAI_SESSION_ID = "voxa_oai_session_id"
        private const val KEY_MAX_DURATION = "voxa_max_duration"
        private const val KEY_AUTO_START = "voxa_auto_start"
    }
}