package com.voxa.android

import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.AboutScreen
import com.voxa.android.ui.AccountScreen
import com.voxa.android.ui.AuthScreen
import com.voxa.android.ui.GestureLibraryScreen
import com.voxa.android.ui.HomeScreen
import com.voxa.android.ui.SettingsScreen
import com.voxa.android.ui.SplashScreen
import com.voxa.android.ui.TutorialScreen
import com.voxa.android.ui.theme.VoxaColors
import com.voxa.android.ui.theme.VoxaTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        var systemSplashReady = false
        splash.setKeepOnScreenCondition { !systemSplashReady }
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        systemSplashReady = true
        setContent {
            VoxaTheme {
                PermissionGate {
                    VoxaNavHost()
                }
            }
        }
    }
}

@Composable
private fun PermissionGate(content: @Composable () -> Unit) {
    var ready by remember { mutableStateOf(false) }

    val permissions = buildList {
        add(Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            add(Manifest.permission.POST_NOTIFICATIONS)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            add(Manifest.permission.BLUETOOTH_CONNECT)
        }
    }.toTypedArray()

    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { ready = true }

    LaunchedEffect(Unit) {
        launcher.launch(permissions)
    }

    if (ready) content()
}

private sealed class Screen {
    object Splash : Screen()
    object Loading : Screen()
    object Auth : Screen()
    object Home : Screen()
    object Tutorial : Screen()
    object Settings : Screen()
    object Account : Screen()
    object About : Screen()
    object GestureLibrary : Screen()
}

@Composable
private fun VoxaNavHost() {
    val scope = rememberCoroutineScope()
    var screen by remember { mutableStateOf<Screen>(Screen.Splash) }
    var sessionResolved by remember { mutableStateOf<Screen?>(null) }

    LaunchedEffect(Unit) {
        val cookie = VoxaApp.prefs.getSessionCookie()
        sessionResolved = if (cookie == null) {
            Screen.Auth
        } else {
            if (TranscriptionApi.validateSession()) Screen.Home else Screen.Auth
        }
    }

    // Back button maps each subpage back to Home; Home keeps system default.
    BackHandler(enabled = screen is Screen.Tutorial || screen is Screen.Settings
            || screen is Screen.Account || screen is Screen.About
            || screen is Screen.GestureLibrary) {
        screen = if (screen is Screen.GestureLibrary) Screen.Settings else Screen.Home
    }

    when (screen) {
        Screen.Splash -> SplashScreen(onFinished = {
            screen = sessionResolved ?: Screen.Loading
        })
        Screen.Loading -> Box(
            modifier = Modifier.fillMaxSize().background(VoxaColors.Bg),
            contentAlignment = Alignment.Center,
        ) {
            CircularProgressIndicator(color = VoxaColors.Primary)
            LaunchedEffect(sessionResolved) {
                val resolved = sessionResolved
                if (resolved != null) screen = resolved
            }
        }
        Screen.Auth -> AuthScreen(onLoginSuccess = { screen = Screen.Home })
        Screen.Home -> HomeScreen(
            onOpenTutorial = { screen = Screen.Tutorial },
            onOpenSettings = { screen = Screen.Settings },
            onOpenAccount = { screen = Screen.Account },
            onOpenAbout = { screen = Screen.About },
        )
        Screen.Tutorial -> TutorialScreen(onBack = { screen = Screen.Home })
        Screen.Settings -> SettingsScreen(
            onBack = { screen = Screen.Home },
            onOpenGestureLibrary = { screen = Screen.GestureLibrary },
        )
        Screen.GestureLibrary -> GestureLibraryScreen(onBack = { screen = Screen.Settings })
        Screen.Account -> AccountScreen(
            onBack = { screen = Screen.Home },
            onLogout = {
                scope.launch {
                    VoxaApp.prefs.clearSession()
                    screen = Screen.Auth
                }
            },
        )
        Screen.About -> AboutScreen(onBack = { screen = Screen.Home })
    }
}
