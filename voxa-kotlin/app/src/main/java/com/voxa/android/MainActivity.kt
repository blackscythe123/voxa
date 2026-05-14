package com.voxa.android

import android.Manifest
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
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
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.AuthScreen
import com.voxa.android.ui.HomeScreen
import com.voxa.android.ui.theme.VoxaColors
import com.voxa.android.ui.theme.VoxaTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
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
    }.toTypedArray()

    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { ready = true }   // continue even if some are denied — IME will show appropriate message

    LaunchedEffect(Unit) {
        launcher.launch(permissions)
    }

    if (ready) content()
}

private sealed class Screen {
    object Loading : Screen()
    object Auth : Screen()
    object Home : Screen()
}

@Composable
private fun VoxaNavHost() {
    val scope = rememberCoroutineScope()
    var screen by remember { mutableStateOf<Screen>(Screen.Loading) }

    LaunchedEffect(Unit) {
        val cookie = VoxaApp.prefs.getSessionCookie()
        screen = if (cookie == null) {
            Screen.Auth
        } else {
            if (TranscriptionApi.validateSession()) Screen.Home else Screen.Auth
        }
    }

    when (screen) {
        Screen.Loading -> Box(
            modifier = Modifier.fillMaxSize().background(VoxaColors.Bg),
            contentAlignment = Alignment.Center,
        ) {
            CircularProgressIndicator(color = VoxaColors.AccentAmber)
        }
        Screen.Auth -> AuthScreen(onLoginSuccess = { screen = Screen.Home })
        Screen.Home -> HomeScreen(onLogout = {
            scope.launch {
                VoxaApp.prefs.clearSession()
                screen = Screen.Auth
            }
        })
    }
}