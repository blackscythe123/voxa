package com.voxa.android

import android.Manifest
import android.content.Intent
import android.database.ContentObserver
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.content.FileProvider
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import com.voxa.android.diag.Diag
import com.voxa.android.ui.BottomSheet
import com.voxa.android.ui.DetailScreen
import com.voxa.android.ui.HistoryScreen
import com.voxa.android.ui.HomeScreen
import com.voxa.android.ui.LaunchOverlay
import com.voxa.android.ui.SettingsScreen
import com.voxa.android.ui.SetupScreen
import com.voxa.android.ui.SignInScreen
import com.voxa.android.ui.Tab
import com.voxa.android.ui.TabBar
import com.voxa.android.ui.VoxaTheme
import com.voxa.android.ui.WordsScreen
import com.voxa.android.ui.vx
import java.io.File

/** Where the app is. Tabs sit at the bottom of the stack; a dictation, sign-in and setup on top. */
sealed interface Route {
    data class Tabbed(val tab: Tab) : Route
    data class Detail(val id: ULong) : Route
    data object SignIn : Route
    data object Setup : Route
}

/** The companion app: setup, Home, History, Words and Settings. */
class MainActivity : ComponentActivity() {
    /** Bumped on resume and when keyboard settings change, so setup state is re-read live. */
    val tick = mutableIntStateOf(0)
    private var openRequest by mutableStateOf<String?>(null)
    private val main = Handler(Looper.getMainLooper())
    private val imeObserver = object : ContentObserver(main) {
        override fun onChange(selfChange: Boolean) {
            tick.intValue++
        }
    }
    private val micPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        Diag.log("permission", "microphone ${if (granted) "granted" else "denied"}")
        tick.intValue++
    }
    private val stack = mutableStateListOf<Route>()
    /** The written splash is on screen; Home's doodle waits for it. */
    var splashShowing by mutableStateOf(false)
        private set
    /** The open bottom sheet, drawn over everything (tabs included). */
    private var sheet by mutableStateOf<(@Composable ColumnScope.() -> Unit)?>(null)

    fun showSheet(content: @Composable ColumnScope.() -> Unit) {
        sheet = content
    }

    fun hideSheet() {
        sheet = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen()
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        openRequest = intent.getStringExtra(EXTRA_OPEN)
        stack.add(if (voxa.core.settings().onboarded) Route.Tabbed(Tab.Home) else Route.Setup)
        debugSeams(intent)
        for (key in listOf(Settings.Secure.ENABLED_INPUT_METHODS, Settings.Secure.DEFAULT_INPUT_METHOD)) {
            contentResolver.registerContentObserver(Settings.Secure.getUriFor(key), false, imeObserver)
        }
        // The written splash: every time the app is opened (not when the keyboard opens it for
        // sign-in or the mic, not on rotation). With animations off it is the still end frame.
        // The keyboard keeps the app's process alive, so "once per process" would mean almost never.
        val animationsOff = Settings.Global.getFloat(contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
        val freeze = if (BuildConfig.DEBUG && intent.hasExtra("debug_launch_at")) intent.getFloatExtra("debug_launch_at", 0.5f) else null
        val showLaunch = freeze != null || (savedInstanceState == null && openRequest == null)
        splashShowing = showLaunch && freeze == null
        com.voxa.android.ui.ThemePref.load(this)
        setContent {
            val dark = com.voxa.android.ui.ThemePref.isDark(com.voxa.android.ui.ThemePref.current.value, androidx.compose.foundation.isSystemInDarkTheme())
            VoxaTheme(dark = dark) { Root(showLaunch, freeze, animationsOff) }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        openRequest = intent.getStringExtra(EXTRA_OPEN)
        debugSeams(intent)
    }

    /** Debug builds only, for testing without a ChatGPT account:
     *  `adb shell am start -n com.voxa.android/.MainActivity --ez debug_fake_session true`
     *  stores a dummy session (ChatGPT rejects it, so nothing is ever transcribed with it). */
    private fun debugSeams(intent: Intent) {
        if (!BuildConfig.DEBUG) return
        if (intent.getBooleanExtra("debug_fake_session", false)) {
            Diag.log("debug", "dummy session stored for testing")
            voxa.signedIn("voxa-debug=1", android.webkit.WebSettings.getDefaultUserAgent(this))
        }
        // --ez debug_ready true|false: treat the session as good (no ChatGPT check).
        // --es debug_transcript "text": the keyboard pastes this instead of transcribing ("" to stop).
        val prefs = getSharedPreferences("debug", MODE_PRIVATE)
        if (intent.hasExtra("debug_ready")) {
            prefs.edit().putBoolean("ready", intent.getBooleanExtra("debug_ready", false)).apply()
            voxa.checkSession(force = true)
        }
        // --es debug_now 2026-12-02T13:05: Home greets as if it were then ("" to use the clock).
        if (intent.hasExtra("debug_now")) prefs.edit().putString("now", intent.getStringExtra("debug_now").orEmpty()).apply()
        // --es debug_name Alexandria: greet with this first name when no account is known ("" clears).
        if (intent.hasExtra("debug_name")) prefs.edit().putString("name", intent.getStringExtra("debug_name").orEmpty()).apply()
        if (intent.hasExtra("debug_levels")) prefs.edit().putBoolean("levels", intent.getBooleanExtra("debug_levels", false)).apply()
        if (intent.hasExtra("debug_transcript")) {
            val t = intent.getStringExtra("debug_transcript").orEmpty()
            prefs.edit().apply { if (t.isEmpty()) remove("transcript") else putString("transcript", t) }.apply()
        }
        intent.getStringExtra("debug_route")?.let { r ->
            stack.clear()
            stack.add(
                when {
                    r == "setup" -> Route.Setup.also { getSharedPreferences("ui", MODE_PRIVATE).edit().putInt("setup_step", 0).apply() }
                    r.startsWith("detail:") -> Route.Detail(r.removePrefix("detail:").toULong())
                    else -> Route.Tabbed(Tab.entries.firstOrNull { it.name.equals(r, true) } ?: Tab.Home)
                },
            )
        }
    }

    override fun onResume() {
        super.onResume()
        Diag.log("app", "screen shown")
        tick.intValue++
        voxa.checkSession()
    }

    override fun onDestroy() {
        contentResolver.unregisterContentObserver(imeObserver)
        super.onDestroy()
    }

    // ── navigation ───────────────────────────────────────────────────────────

    fun go(route: Route) {
        sheet = null
        if (route is Route.Tabbed) {
            // A fresh visit to History starts without the last search.
            if (route.tab == Tab.History) getSharedPreferences("ui", MODE_PRIVATE).edit().remove("history_query").apply()
            stack.clear()
            if (route.tab != Tab.Home) stack.add(Route.Tabbed(Tab.Home))
            stack.add(route)
        } else {
            stack.add(route)
        }
    }

    fun back() {
        if (stack.size > 1) stack.removeAt(stack.lastIndex)
    }

    /** Setup finished (or skipped): Home. */
    fun finishSetup() {
        voxa.core.setSettings(voxa.core.settings().copy(onboarded = true))
        getSharedPreferences("ui", MODE_PRIVATE).edit().putBoolean("greet_first_open", true).apply()
        stack.clear()
        stack.add(Route.Tabbed(Tab.Home))
    }

    fun restartSetup() {
        getSharedPreferences("ui", MODE_PRIVATE).edit().putInt("setup_step", 0).apply()
        stack.clear()
        stack.add(Route.Setup)
    }

    // ── system actions ───────────────────────────────────────────────────────

    /** Ask for the mic; after a permanent "Don't allow", open Voxa's page in Android settings. */
    fun requestMic() {
        val prefs = getSharedPreferences("ui", MODE_PRIVATE)
        val askedBefore = prefs.getBoolean("mic_asked", false)
        if (askedBefore && !shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO) && !voxa.micGranted()) {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
        } else {
            prefs.edit().putBoolean("mic_asked", true).apply()
            micPermission.launch(Manifest.permission.RECORD_AUDIO)
        }
    }

    /** Zip files/diag and open the share sheet. */
    fun shareDiagnostics() {
        Diag.log("app", "share diagnostics")
        val zip = Diag.zip(File(cacheDir, "share"))
        val uri = FileProvider.getUriForFile(this, "$packageName.files", zip)
        val send = Intent(Intent.ACTION_SEND)
            .setType("application/zip")
            .putExtra(Intent.EXTRA_STREAM, uri)
            .putExtra(Intent.EXTRA_SUBJECT, "Voxa diagnostics")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        startActivity(Intent.createChooser(send, "Share diagnostics"))
    }

    fun shareText(text: String) {
        startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
    }

    fun openKeyboardSettings() {
        startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
    }

    fun showKeyboardPicker() {
        getSystemService(InputMethodManager::class.java).showInputMethodPicker()
    }

    @Composable
    private fun Root(showLaunch: Boolean, freeze: Float?, reduced: Boolean) {
        var launching by remember { mutableStateOf(showLaunch) }
        openRequest?.let {
            openRequest = null
            when (it) {
                OPEN_SIGN_IN -> if (stack.lastOrNull() != Route.SignIn) go(Route.SignIn)
                OPEN_MIC -> {
                    if (stack.lastOrNull() == Route.SignIn) back()
                    requestMic()
                }
                OPEN_STORAGE -> {
                    stack.clear()
                    stack.add(Route.Tabbed(Tab.Settings))
                }
            }
        }
        val route = stack.lastOrNull() ?: Route.Tabbed(Tab.Home)
        BackHandler(stack.size > 1) { back() }
        val c = vx
        Box(Modifier.fillMaxSize().background(c.base)) {
            Column(Modifier.fillMaxSize()) {
                // Each place starts fresh (a second dictation must not inherit the first one's player).
                Box(Modifier.weight(1f).fillMaxWidth().statusBarsPadding()) { key(route) {
                    when (route) {
                        is Route.Tabbed -> when (route.tab) {
                            Tab.Home -> HomeScreen(this@MainActivity)
                            Tab.History -> HistoryScreen(this@MainActivity)
                            Tab.Words -> WordsScreen()
                            Tab.Settings -> SettingsScreen(this@MainActivity)
                        }
                        is Route.Detail -> DetailScreen(this@MainActivity, route.id)
                        Route.SignIn -> SignInScreen(onDone = { back() }, onClose = { back() })
                        Route.Setup -> SetupScreen(this@MainActivity)
                    }
                } }
                if (route is Route.Tabbed) TabBar(route.tab) { go(Route.Tabbed(it)) }
            }
            sheet?.let { s -> BottomSheet(onDismiss = { sheet = null }, content = s) }
            if (launching) LaunchOverlay(freeze, reduced) {
                launching = false
                splashShowing = false
            }
        }
    }

    companion object {
        const val EXTRA_OPEN = "open"
        const val OPEN_SIGN_IN = "sign-in"
        const val OPEN_MIC = "mic"
        const val OPEN_STORAGE = "storage"

        /** The launch animation plays once per process (a cold start). */
    }
}
