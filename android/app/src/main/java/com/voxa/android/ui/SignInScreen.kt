package com.voxa.android.ui

import android.annotation.SuppressLint
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.voxa.android.R
import com.voxa.android.data.hasSessionToken
import com.voxa.android.voxa
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val LOGIN_URL = "https://chatgpt.com/auth/login"

/**
 * ChatGPT's own sign-in page in a WebView (Voxa never sees the password). Once the page is on
 * chatgpt.com outside /auth/ and the session cookie is there, the full cookie header (including
 * cf_clearance) and this WebView's User-Agent are saved together. Carried over from the old app.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun SignInScreen(onDone: () -> Unit, onClose: () -> Unit) {
    val context = LocalContext.current
    val app = context.voxa
    val scope = rememberCoroutineScope()
    var loading by remember { mutableStateOf(true) }
    var linking by remember { mutableStateOf(false) }
    // The device's real WebView engine (a made-up Chrome version trips Cloudflare), minus "; wv":
    // Google sign-in refuses WebViews that say so.
    val userAgent = remember { WebSettings.getDefaultUserAgent(context).replace("; wv", "") }

    fun tryCapture(url: String) {
        if (linking || !url.startsWith("https://chatgpt.com") || url.contains("/auth/")) return
        linking = true
        scope.launch {
            repeat(30) {
                delay(500)
                val cookies = CookieManager.getInstance().getCookie("https://chatgpt.com") ?: return@repeat
                if (!hasSessionToken(cookies)) return@repeat
                CookieManager.getInstance().flush()
                app.signedIn(cookies, userAgent)
                onDone()
                return@launch
            }
            linking = false
        }
    }

    Column(Modifier.fillMaxSize().background(V.Page)) {
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Sign in to ChatGPT", fontSize = 18.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
                Muted("This is ChatGPT's own page. Voxa never sees your password.")
            }
            IconButton(onClose) { Icon(painterResource(R.drawable.ic_close), "Close", tint = V.Text) }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    WebView(ctx).apply {
                        // A real, fixed-size viewport; WRAP_CONTENT leaves ChatGPT's page blank.
                        layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        settings.setSupportZoom(false)
                        settings.userAgentString = userAgent
                        CookieManager.getInstance().setAcceptCookie(true)
                        CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)
                        webViewClient = object : WebViewClient() {
                            override fun onPageFinished(view: WebView?, url: String?) {
                                loading = false
                                url?.let(::tryCapture)
                            }

                            override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                                request?.url?.toString()?.let(::tryCapture)
                                return false
                            }
                        }
                        loadUrl(LOGIN_URL)
                    }
                },
            )
            if (loading || linking) {
                Column(Modifier.fillMaxSize().background(V.Page.copy(alpha = if (linking) 0.94f else 1f)), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                    CircularProgressIndicator(color = V.Ink, modifier = Modifier.size(36.dp))
                    Spacer(Modifier.height(12.dp))
                    Text(if (linking) "Linking your ChatGPT account" else "Opening ChatGPT", fontSize = 15.sp, color = V.Text)
                }
            }
        }
    }
}
