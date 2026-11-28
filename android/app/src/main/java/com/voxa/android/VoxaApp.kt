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
import com.voxa.android.data.hasSessionToken
import com.voxa.android.diag.Diag
import com.voxa.android.notify.Notifier
import com.voxa.android.notify.VoxaJobs
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.AccountInfo
import uniffi.voxa_ffi.LogSink
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.NetworkState
import uniffi.voxa_ffi.ReadinessInfo
import uniffi.voxa_ffi.SessionCheck
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.VoxaFlow
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
    /** Held around every read and write of the vault, so a take, load and save is one step and
     *  an older sign-in is never saved over a newer one or over a sign-out. */
    private val vaultLock = Mutex()
    /** The saved session was read (off the main thread) and handed to the core. */
    private val sessionLoaded = CompletableDeferred<Unit>()

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
        holdToTalkOnByDefault()
        watchNetwork()
        // The vault is decrypted with the Keystore: off the main thread.
        scope.launch {
            val saved = withContext(Dispatchers.IO) {
                vaultLock.withLock {
                    val s = vault.load()
                    if (s != null) core.useSession(s.cookieHeader, s.userAgent)
                    s
                }
            }
            Diag.log("app", "saved session: ${if (saved != null) "yes" else "none"}; mic ${if (micGranted()) "granted" else "not granted"}")
            if (session.value == SessionState.UNKNOWN) session.value = if (saved != null) SessionState.UNKNOWN else SessionState.EXPIRED
            account.value = withContext(Dispatchers.IO) { core.cachedAccount() }
            sessionLoaded.complete(Unit)
            checkSession(force = true)
        }
        // Notifications (docs/notifications.md): channels, the 12 h session check, and the
        // background send of dictations recorded offline, checked whenever History changes.
        Notifier.createChannels(this)
        VoxaJobs.schedulePeriodic(this)
        scope.launch {
            historyVersion.collectLatest {
                delay(1500)
                // Every send changes History: any sign-in ChatGPT refreshed on the way is saved
                // here too, so no caller can forget it.
                keepRefreshedCookies()
                withContext(Dispatchers.IO) { VoxaJobs.scheduleSendIfWaiting(this@VoxaApp) }
            }
        }
    }

    /** The live keyboard's flow, so background work never sends a dictation the keyboard owns. */
    @Volatile var keyboardFlow: VoxaFlow? = null

    /** The keyboard will send this entry itself (recording, being written out, or waiting for the
     *  network in its live flow). A failed or ready take is not the keyboard's any more. */
    fun keyboardOwns(entryId: ULong): Boolean = runCatching { keyboardFlow?.owns(entryId) == true }.getOrDefault(false)

    /** Wait until the saved session is in the core (background jobs on a cold start). */
    suspend fun awaitSession() = sessionLoaded.await()

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

    /** Debug builds only: what session checks answer instead of asking ChatGPT ("valid" or "expired"). */
    val debugCheck: SessionState? get() = if (!BuildConfig.DEBUG) null else when (getSharedPreferences("debug", MODE_PRIVATE).getString("check", null)) {
        "valid" -> SessionState.VALID
        "expired" -> SessionState.EXPIRED
        else -> null
    }

    /** A check that may lead to "You're signed out": ChatGPT's session call answered with no token
     *  for a session we had (not a 401 or 403, not "nothing saved", not Cloudflare). Never a
     *  failed transcription. */
    fun definiteExpiry(r: SessionCheck) = r.session == SessionState.EXPIRED && !r.blocked && (r.definite || debugCheck != null)

    /** ChatGPT re-issues the sign-in (90 days from now) as it's used: save it, or the saved sign-in
     *  expires 90 days after the original sign-in however much Voxa is used. Take, load and save
     *  are one step under [vaultLock], off the main thread; the core hands out only cookies
     *  refreshed from the live session, and nothing after a sign-out or a new sign-in. Logs no values. */
    suspend fun keepRefreshedCookies() = withContext(Dispatchers.IO) {
        vaultLock.withLock {
            val generation = core.sessionGeneration()
            val header = core.takeRefreshedCookies() ?: return@withLock
            val old = vault.load()
            if (old == null || generation != core.sessionGeneration() || !hasSessionToken(header)) {
                Diag.log("signin", "sign-in refreshed by ChatGPT: not saved (signed out or changed meanwhile)")
                return@withLock
            }
            vault.save(SessionVault.Saved(header, old.userAgent))
            Diag.log("signin", "sign-in refreshed by ChatGPT: saved")
        }
    }

    /** Check the session now and wait for the answer (network; for the background jobs too). */
    suspend fun checkSessionNow(): SessionCheck {
        awaitSession()
        lastCheckMs = System.currentTimeMillis()
        val generation = core.sessionGeneration()
        val fake = debugCheck
        val r = if (fake != null) SessionCheck(fake, NetworkState.UP, withContext(Dispatchers.IO) { core.cachedAccount() }, false, fake == SessionState.EXPIRED) else withContext(Dispatchers.IO) { core.checkSession() }
        keepRefreshedCookies()
        // Signed out or in while this check ran: its answer is about a session that is gone.
        if (generation != core.sessionGeneration()) {
            Diag.log("readiness", "session check outlived a sign-in or sign-out: ignored")
            return r.copy(session = SessionState.UNKNOWN, definite = false)
        }
        withContext(Dispatchers.Main) {
            Log.i(TAG, "session check: ${r.session}, network ${r.network}, blocked ${r.blocked}")
            Diag.log("readiness", "session ${r.session}, network ${r.network}, blocked ${r.blocked}${if (fake != null) " (debug answer)" else ""}")
            session.value = r.session
            blocked.value = r.blocked
            if (r.network == NetworkState.DOWN) online.value = false
            if (r.account != null) account.value = r.account
            if (r.session == SessionState.EXPIRED) account.value = null
        }
        return r
    }

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
            val r = checkSessionNow()
            // Known good, now definitely signed out: say so only if a check 2 minutes later agrees.
            if (Notifier.afterCheck(this@VoxaApp, r, definiteExpiry(r))) VoxaJobs.scheduleConfirm(this@VoxaApp, Notifier.confirmDelayMs(this@VoxaApp))
        }
    }

    /** The sign-in WebView captured a session. */
    fun signedIn(cookieHeader: String, userAgent: String) {
        // In memory now (this also drops cookies refreshed for an older session); saved off the
        // main thread, under the same lock as every refresh.
        core.useSession(cookieHeader, userAgent)
        Log.i(TAG, "session captured (${cookieHeader.length} bytes of cookies)")
        Diag.log("signin", "success: session captured")
        scope.launch {
            withContext(Dispatchers.IO) { vaultLock.withLock { vault.save(SessionVault.Saved(cookieHeader, userAgent)) } }
            checkSession(force = true)
        }
    }

    fun signOut() {
        Diag.log("signin", "signed out")
        Notifier.signedOutOnPurpose(this)
        // First the core (a new generation: nothing still running may save this session again),
        // then the file, under the lock every save holds.
        core.signOut()
        scope.launch(Dispatchers.IO) { vaultLock.withLock { vault.clear() } }
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
