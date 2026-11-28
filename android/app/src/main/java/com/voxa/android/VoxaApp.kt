package com.voxa.android

import android.Manifest
import android.app.Application
import android.content.Context
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.provider.Settings
import android.util.Log
import android.view.inputmethod.InputMethodManager
import android.webkit.CookieManager
import com.voxa.android.data.SessionVault
import com.voxa.android.diag.Diag
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.AccountInfo
import uniffi.voxa_ffi.LogSink
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.ReadinessInfo
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.VoxaCore
import uniffi.voxa_ffi.coreVersion
import uniffi.voxa_ffi.setLogSink

/**
 * App-wide state shared by the keyboard and the app screens (same process): the Rust core, the
 * session, and what readiness says. Never logs cookie, token or transcript values.
 */
class VoxaApp : Application() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val core: VoxaCore by lazy { VoxaCore(filesDir.path) }
    private val vault by lazy { SessionVault(this) }

    val session = MutableStateFlow(SessionState.UNKNOWN)
    val account = MutableStateFlow<AccountInfo?>(null)
    val online = MutableStateFlow(true)
    /** ChatGPT answered the last check with Cloudflare's challenge. */
    val blocked = MutableStateFlow(false)
    /** Bumped whenever History changes, so open screens reload. */
    val historyVersion = MutableStateFlow(0)
    private var lastCheckMs = 0L

    override fun onCreate() {
        super.onCreate()
        instance = this
        // Diagnostics are on in debug builds (the owner's test APK), off in release.
        Diag.init(this, BuildConfig.DEBUG)
        if (Diag.enabled) {
            setLogSink(object : LogSink {
                override fun log(line: String) = Diag.log("core", line)
            }, BuildConfig.DEBUG)
        }
        Log.i(TAG, "voxa core ${coreVersion()} loaded")
        Diag.log("app", "start: Voxa ${BuildConfig.VERSION_NAME} (${BuildConfig.BUILD_TYPE}), core ${coreVersion()}, Android ${android.os.Build.VERSION.RELEASE} on ${android.os.Build.MODEL}")
        Diag.memory("app start")
        val saved = vault.load()
        if (saved != null) core.useSession(saved.cookieHeader, saved.userAgent)
        Diag.log("app", "saved session: ${if (saved != null) "yes" else "none"}; mic ${if (micGranted()) "granted" else "not granted"}")
        session.value = if (saved != null) SessionState.UNKNOWN else SessionState.EXPIRED
        account.value = core.cachedAccount()
        holdToTalkOnByDefault()
        watchNetwork()
        checkSession(force = true)
    }

    /** On a phone, hold to talk has always been on (hold the mic, let go to stop); the shared
     *  settings default it off (the desktop's hotkey mode). Turn it on once, then the user decides. */
    private fun holdToTalkOnByDefault() {
        val prefs = getSharedPreferences("ui", MODE_PRIVATE)
        if (prefs.getBoolean("hold_default_set", false)) return
        val s = core.settings()
        if (!s.holdToTalk) core.setSettings(s.copy(holdToTalk = true))
        prefs.edit().putBoolean("hold_default_set", true).apply()
    }

    fun micGranted(): Boolean = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    fun readiness() = ReadinessInfo(
        session = session.value,
        network = if (online.value) NetworkState.UP else NetworkState.DOWN,
        mic = if (micGranted()) MicState.GRANTED else MicState.DENIED,
    )

    /** Debug builds only: the session counts as good without asking ChatGPT (screenshots, tests). */
    val debugReady get() = BuildConfig.DEBUG && getSharedPreferences("debug", MODE_PRIVATE).getBoolean("ready", false)

    /** Debug builds only: the level bars move to a made-up voice (emulator mics hear silence). */
    val debugLevels get() = BuildConfig.DEBUG && getSharedPreferences("debug", MODE_PRIVATE).getBoolean("levels", false)

    /** Debug builds only: what "transcription" returns instead of calling ChatGPT, or null. */
    val debugTranscript: String? get() = if (BuildConfig.DEBUG) getSharedPreferences("debug", MODE_PRIVATE).getString("transcript", null) else null

    /** Ask ChatGPT whether the session is good, at most every 10 minutes unless [force]. */
    fun checkSession(force: Boolean = false) {
        if (debugReady) {
            session.value = SessionState.VALID
            blocked.value = false
            online.value = true
            if (account.value == null) account.value = core.cachedAccount()
            return
        }
        val now = System.currentTimeMillis()
        if (!force && now - lastCheckMs < 10 * 60_000) return
        lastCheckMs = now
        scope.launch {
            val r = withContext(Dispatchers.IO) { core.checkSession() }
            // ChatGPT re-issues the sign-in (90 days from now) as it's used: save it, or the saved
            // sign-in expires 90 days after the original sign-in however much Voxa is used.
            core.takeRefreshedCookies()?.let { header ->
                vault.load()?.let { old -> withContext(Dispatchers.IO) { vault.save(SessionVault.Saved(header, old.userAgent)) } }
                Diag.log("signin", "sign-in refreshed by ChatGPT: saved")
            }
            Log.i(TAG, "session check: ${r.session}, network ${r.network}, blocked ${r.blocked}")
            Diag.log("readiness", "session ${r.session}, network ${r.network}, blocked ${r.blocked}")
            session.value = r.session
            blocked.value = r.blocked
            if (r.network == NetworkState.DOWN) online.value = false
            if (r.account != null) account.value = r.account
            if (r.session == SessionState.EXPIRED) account.value = null
        }
    }

    /** The sign-in WebView captured a session. */
    fun signedIn(cookieHeader: String, userAgent: String) {
        vault.save(SessionVault.Saved(cookieHeader, userAgent))
        core.useSession(cookieHeader, userAgent)
        Log.i(TAG, "session captured (${cookieHeader.length} bytes of cookies)")
        Diag.log("signin", "success: session captured")
        checkSession(force = true)
    }

    fun signOut() {
        Diag.log("signin", "signed out")
        vault.clear()
        core.signOut()
        CookieManager.getInstance().removeAllCookies(null)
        session.value = SessionState.EXPIRED
        account.value = null
        com.voxa.android.ui.AvatarCache.clear(this)
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        Diag.log("memory", "onTrimMemory level $level")
        Diag.memory("trim $level")
    }

    override fun onLowMemory() {
        super.onLowMemory()
        Diag.log("memory", "onLowMemory")
        Diag.memory("low memory")
    }

    fun historyChanged() {
        historyVersion.value++
    }

    private fun watchNetwork() {
        val cm = getSystemService(ConnectivityManager::class.java)
        val validated = { n: Network? ->
            cm.getNetworkCapabilities(n)?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
        }
        online.value = validated(cm.activeNetwork) || debugReady
        cm.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                val up = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                scope.launch {
                    val was = online.value
                    if (was != up) Diag.log("network", if (up) "online (validated)" else "connected, not validated")
                    online.value = up || debugReady
                    if (up && !was) checkSession(force = session.value != SessionState.VALID)
                }
            }

            override fun onLost(network: Network) {
                Diag.log("network", "lost")
                scope.launch { online.value = debugReady }
            }
        })
    }

    // ── keyboard setup state ─────────────────────────────────────────────────

    fun keyboardEnabled(): Boolean =
        getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.packageName == packageName }

    fun keyboardSelected(): Boolean =
        Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)?.startsWith("$packageName/") == true

    companion object {
        const val TAG = "Voxa"
        lateinit var instance: VoxaApp
            private set
    }
}

val Context.voxa: VoxaApp get() = applicationContext as VoxaApp
