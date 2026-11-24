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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.AccountInfo
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.ReadinessInfo
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.VoxaCore
import uniffi.voxa_ffi.coreVersion

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
        Log.i(TAG, "voxa core ${coreVersion()} loaded")
        val saved = vault.load()
        if (saved != null) core.useSession(saved.cookieHeader, saved.userAgent)
        session.value = if (saved != null) SessionState.UNKNOWN else SessionState.EXPIRED
        account.value = core.cachedAccount()
        watchNetwork()
        checkSession(force = true)
    }

    fun micGranted(): Boolean = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    fun readiness() = ReadinessInfo(
        session = session.value,
        network = if (online.value) NetworkState.UP else NetworkState.DOWN,
        mic = if (micGranted()) MicState.GRANTED else MicState.DENIED,
    )

    /** Ask ChatGPT whether the session is good, at most every 10 minutes unless [force]. */
    fun checkSession(force: Boolean = false) {
        val now = System.currentTimeMillis()
        if (!force && now - lastCheckMs < 10 * 60_000) return
        lastCheckMs = now
        scope.launch {
            val r = withContext(Dispatchers.IO) { core.checkSession() }
            Log.i(TAG, "session check: ${r.session}, network ${r.network}, blocked ${r.blocked}")
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
        checkSession(force = true)
    }

    fun signOut() {
        vault.clear()
        core.signOut()
        CookieManager.getInstance().removeAllCookies(null)
        session.value = SessionState.EXPIRED
        account.value = null
    }

    fun historyChanged() {
        historyVersion.value++
    }

    private fun watchNetwork() {
        val cm = getSystemService(ConnectivityManager::class.java)
        val validated = { n: Network? ->
            cm.getNetworkCapabilities(n)?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) == true
        }
        online.value = validated(cm.activeNetwork)
        cm.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                val up = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                scope.launch {
                    val was = online.value
                    online.value = up
                    if (up && !was) checkSession(force = session.value != SessionState.VALID)
                }
            }

            override fun onLost(network: Network) {
                scope.launch { online.value = false }
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
