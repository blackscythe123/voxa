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
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.core.content.FileProvider
import com.voxa.android.diag.Diag
import java.io.File
import com.voxa.android.ui.HistoryScreen
import com.voxa.android.ui.HomeScreen
import com.voxa.android.ui.SettingsScreen
import com.voxa.android.ui.SignInScreen
import com.voxa.android.ui.TourScreen
import com.voxa.android.ui.VoxaTheme
import com.voxa.android.ui.WordsScreen

enum class Screen { Home, History, Settings, Words, SignIn, Tour }

/** The companion app: sign in, set up the keyboard and the mic, History, Settings, Words. */
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        openRequest = intent.getStringExtra(EXTRA_OPEN)
        debugSeams(intent)
        for (key in listOf(Settings.Secure.ENABLED_INPUT_METHODS, Settings.Secure.DEFAULT_INPUT_METHOD)) {
            contentResolver.registerContentObserver(Settings.Secure.getUriFor(key), false, imeObserver)
        }
        setContent { VoxaTheme { Root() } }
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
        if (!BuildConfig.DEBUG || !intent.getBooleanExtra("debug_fake_session", false)) return
        Diag.log("debug", "dummy session stored for testing")
        voxa.signedIn("voxa-debug=1", android.webkit.WebSettings.getDefaultUserAgent(this))
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

    fun openKeyboardSettings() {

        startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
    }

    fun showKeyboardPicker() {
        getSystemService(InputMethodManager::class.java).showInputMethodPicker()
    }

    @Composable
    private fun Root() {
        val first = remember { if (voxa.core.settings().onboarded) Screen.Home else Screen.Tour }
        var screen by remember { mutableStateOf(first) }
        var back by remember { mutableStateOf(Screen.Home) }
        val go = { to: Screen ->
            back = if (to == Screen.SignIn || to == Screen.Words) screen else Screen.Home
            screen = to
        }
        openRequest?.let {
            openRequest = null
            when (it) {
                OPEN_SIGN_IN -> go(Screen.SignIn)
                OPEN_MIC -> {
                    if (screen == Screen.SignIn) screen = Screen.Home
                    requestMic()
                }
            }
        }
        BackHandler(screen != Screen.Home && screen != Screen.Tour) { screen = back.takeIf { it != screen } ?: Screen.Home }
        Box(Modifier.fillMaxSize().systemBarsPadding()) {
            when (screen) {
                Screen.Home -> HomeScreen(this@MainActivity, go)
                Screen.History -> HistoryScreen(this@MainActivity, go)
                Screen.Settings -> SettingsScreen(this@MainActivity, go)
                Screen.Words -> WordsScreen { screen = Screen.Settings }
                Screen.SignIn -> SignInScreen(onDone = { screen = back }, onClose = { screen = back })
                Screen.Tour -> TourScreen(this@MainActivity, onSignIn = { go(Screen.SignIn); back = Screen.Tour }, onDone = { screen = Screen.Home })
            }
        }
    }

    companion object {
        const val EXTRA_OPEN = "open"
        const val OPEN_SIGN_IN = "sign-in"
        const val OPEN_MIC = "mic"
    }
}
