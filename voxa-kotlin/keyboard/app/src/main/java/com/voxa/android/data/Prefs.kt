package com.voxa.android.data

import android.content.Context
import android.content.SharedPreferences
import java.util.UUID

class Prefs(context: Context) {

    init { register(this) }

    private val sp: SharedPreferences =
        context.getSharedPreferences("voxa_prefs", Context.MODE_PRIVATE)

    fun getSessionCookie(): String? = sp.getString(KEY_SESSION_COOKIE, null)
    fun setSessionCookie(value: String) = sp.edit().putString(KEY_SESSION_COOKIE, value).apply()

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

    fun getHoldToTalk(): Boolean = sp.getBoolean(KEY_HOLD_TO_TALK, false)
    fun setHoldToTalk(enabled: Boolean) = sp.edit().putBoolean(KEY_HOLD_TO_TALK, enabled).apply()

    fun getAutoPunctuation(): Boolean = sp.getBoolean(KEY_AUTO_PUNCT, true)
    fun setAutoPunctuation(enabled: Boolean) = sp.edit().putBoolean(KEY_AUTO_PUNCT, enabled).apply()

    fun getHaptics(): Boolean = sp.getBoolean(KEY_HAPTICS, true)
    fun setHaptics(enabled: Boolean) = sp.edit().putBoolean(KEY_HAPTICS, enabled).apply()

    fun getTrimSilence(): Boolean = sp.getBoolean(KEY_TRIM_SILENCE, false)
    fun setTrimSilence(enabled: Boolean) = sp.edit().putBoolean(KEY_TRIM_SILENCE, enabled).apply()

    fun getLanguage(): String = sp.getString(KEY_LANGUAGE, "en-US") ?: "en-US"
    fun setLanguage(value: String) = sp.edit().putString(KEY_LANGUAGE, value).apply()

    fun getSampleRate(): Int = sp.getInt(KEY_SAMPLE_RATE, 16000)
    fun setSampleRate(hz: Int) = sp.edit().putInt(KEY_SAMPLE_RATE, hz).apply()

    fun getModel(): String = sp.getString(KEY_MODEL, "whisper-1") ?: "whisper-1"
    fun setModel(value: String) = sp.edit().putString(KEY_MODEL, value).apply()

    companion object {
        // Single-process singleton — the first Prefs(context) call (made from
        // VoxaApp.onCreate) installs itself here so code that has no app reference
        // (TranscriptionApi, IME services in the :keyboard library) can still get it.
        @Volatile private var instance: Prefs? = null
        private fun register(p: Prefs) { instance = p }
        fun get(): Prefs = instance ?: error("Prefs.get() before Prefs(context) was constructed")

        private const val KEY_SESSION_COOKIE = "voxa_session_cookie"
        private const val KEY_COOKIE_HEADER = "voxa_cookie_header"
        private const val KEY_ACCESS_TOKEN = "voxa_access_token"
        private const val KEY_DEVICE_ID = "voxa_device_id"
        private const val KEY_OAI_SESSION_ID = "voxa_oai_session_id"
        private const val KEY_MAX_DURATION = "voxa_max_duration"
        private const val KEY_AUTO_START = "voxa_auto_start"
        private const val KEY_HOLD_TO_TALK = "voxa_hold_to_talk"
        private const val KEY_AUTO_PUNCT = "voxa_auto_punctuation"
        private const val KEY_HAPTICS = "voxa_haptics"
        private const val KEY_TRIM_SILENCE = "voxa_trim_silence"
        private const val KEY_LANGUAGE = "voxa_language"
        private const val KEY_SAMPLE_RATE = "voxa_sample_rate"
        private const val KEY_MODEL = "voxa_model"
    }
}
