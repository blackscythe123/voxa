package com.voxa.android.ui

import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.voxa.android.VoxaApp
import com.voxa.android.ui.theme.VoxaColors
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val LOGIN_URL = "https://chatgpt.com/auth/login"

@Composable
fun AuthScreen(onLoginSuccess: () -> Unit) {
    var loading by remember { mutableStateOf(true) }
    var capturing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    var captured by remember { mutableStateOf(false) }

    fun tryCaptureSession(url: String) {
        if (captured) return
        if (!url.startsWith("https://chatgpt.com") || url.contains("/auth/")) return
        captured = true
        capturing = true
        scope.launch {
            repeat(20) {
                delay(500)
                val cookies = CookieManager.getInstance().getCookie("https://chatgpt.com") ?: return@repeat
                val token = cookies.split(";")
                    .map { it.trim() }
                    .find { it.startsWith("__Secure-next-auth.session-token") }
                    ?.substringAfter("=")
                    ?.trim()
                if (!token.isNullOrEmpty()) {
                    VoxaApp.prefs.setSessionCookie(token)
                    onLoginSuccess()
                    return@launch
                }
            }
            capturing = false
            captured = false
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(VoxaColors.Bg)
            .systemBarsPadding()
    ) {
        // Header
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp)
                .padding(top = 32.dp, bottom = 16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = "voxa",
                fontSize = 36.sp,
                fontWeight = FontWeight.Bold,
                fontStyle = FontStyle.Italic,
                color = VoxaColors.AccentAmber,
                letterSpacing = 2.sp,
            )
            Spacer(modifier = Modifier.height(4.dp))
            Text(
                text = "Sign in to ChatGPT to continue",
                fontSize = 14.sp,
                color = VoxaColors.TextMuted,
            )
        }

        // WebView
        Box(
            modifier = Modifier
                .weight(1f)
                .padding(16.dp)
                .clip(RoundedCornerShape(16.dp))
                .border(1.dp, VoxaColors.Border, RoundedCornerShape(16.dp))
        ) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    val wv = WebView(ctx)
                    wv.settings.apply {
                        javaScriptEnabled = true
                        domStorageEnabled = true
                        @Suppress("DEPRECATION")
                        allowFileAccessFromFileURLs = false
                    }
                    CookieManager.getInstance().apply {
                        setAcceptCookie(true)
                        setAcceptThirdPartyCookies(wv, true)
                    }
                    wv.webViewClient = object : WebViewClient() {
                        override fun onPageFinished(view: WebView?, url: String?) {
                            loading = false
                            url?.let { tryCaptureSession(it) }
                        }
                        override fun shouldOverrideUrlLoading(
                            view: WebView?,
                            request: WebResourceRequest?
                        ): Boolean {
                            request?.url?.toString()?.let { tryCaptureSession(it) }
                            return false
                        }
                    }
                    wv.loadUrl(LOGIN_URL)
                    wv
                }
            )

            if (loading || capturing) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(VoxaColors.Surface),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    CircularProgressIndicator(color = VoxaColors.AccentAmber)
                    Spacer(modifier = Modifier.height(12.dp))
                    Text(
                        text = if (capturing) "Capturing session…" else "Loading…",
                        fontSize = 14.sp,
                        color = VoxaColors.TextMuted,
                    )
                }
            }
        }

        // Retry hint
        Text(
            text = "Having trouble? Tap to retry",
            fontSize = 13.sp,
            color = VoxaColors.TextDim,
            textAlign = TextAlign.Center,
            modifier = Modifier
                .fillMaxWidth()
                .clickable { captured = false }
                .padding(vertical = 16.dp),
        )
    }
}
