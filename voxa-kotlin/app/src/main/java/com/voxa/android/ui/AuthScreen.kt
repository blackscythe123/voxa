package com.voxa.android.ui

import android.app.Activity
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.voxa.android.VoxaApp
import com.voxa.android.ui.theme.DisplayFamily
import com.voxa.android.ui.theme.MonoFamily
import com.voxa.android.ui.theme.VoxaColors
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val LOGIN_URL = "https://chatgpt.com/auth/login"
private const val CHROME_UA =
    "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 " +
    "(KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36"

// CSS injected into the ChatGPT auth WebView to vertically center the login form
// (fixes "content jammed top 75%" complaint). Marketing-hiding is intentionally
// narrow: only anchors pointing at /pricing or marketing.openai are hidden — the
// earlier text-phrase scan nuked the whole login subtree, so it has been removed.
private const val AUTH_CLEANUP_PAYLOAD = """
(function() {
    var STYLE_ID = '__voxa_auth_cleanup_style';
    var css = ''
        + 'html, body { background: #FFFFFF !important; min-height: 100vh !important; overflow-x: hidden !important; }'
        + 'body { display: flex !important; flex-direction: column !important; justify-content: center !important; }'
        + '.__voxa_hidden { display: none !important; }';
    if (!document.getElementById(STYLE_ID)) {
        var s = document.createElement('style');
        s.id = STYLE_ID;
        s.textContent = css;
        (document.head || document.documentElement).appendChild(s);
    }

    var BAD_HREF = /\/pricing|marketing\.openai|openai\.com\/chatgpt\/(plus|pro|team|enterprise)/i;
    function hideMarketingLinks() {
        var anchors = document.querySelectorAll('a[href]');
        for (var i = 0; i < anchors.length; i++) {
            var href = anchors[i].getAttribute('href') || '';
            if (BAD_HREF.test(href)) anchors[i].classList.add('__voxa_hidden');
        }
    }
    hideMarketingLinks();
})();
"""

@Composable
fun AuthScreen(onLoginSuccess: () -> Unit) {
    val context = LocalContext.current
    var loading by remember { mutableStateOf(true) }
    var linking by remember { mutableStateOf(false) }
    var captured by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    fun tryCaptureSession(url: String) {
        if (captured) return
        if (!url.startsWith("https://chatgpt.com") || url.contains("/auth/")) return
        captured = true
        linking = true
        scope.launch {
            repeat(30) {
                delay(500)
                val cookieHeader = CookieManager.getInstance().getCookie("https://chatgpt.com") ?: return@repeat
                if (!cookieHeader.contains("__Secure-next-auth.session-token")) return@repeat

                val sessionToken = cookieHeader.split(";")
                    .map { it.trim() }
                    .find { it.startsWith("__Secure-next-auth.session-token") }
                    ?.substringAfter("=")
                    ?.trim()
                    ?: return@repeat

                VoxaApp.prefs.setSessionCookie(sessionToken)
                VoxaApp.prefs.setCookieHeader(cookieHeader)
                onLoginSuccess()
                return@launch
            }
            linking = false
            captured = false
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(VoxaColors.Surface)
    ) {
        Column(modifier = Modifier.fillMaxSize().systemBarsPadding()) {

            // Thin native top strip: Voxa wordmark on the left, screen title in the middle, close button on the right.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(VoxaColors.Surface)
                    .padding(horizontal = 16.dp, vertical = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Voxa",
                        fontFamily = DisplayFamily,
                        fontWeight = FontWeight.Bold,
                        fontSize = 18.sp,
                        letterSpacing = (-0.5).sp,
                        color = VoxaColors.Ink,
                    )
                    Text(
                        text = "Sign in to ChatGPT to use Voxa",
                        fontFamily = DisplayFamily,
                        fontSize = 11.5.sp,
                        color = VoxaColors.Muted,
                        modifier = Modifier.padding(top = 1.dp),
                    )
                }
                Box(
                    modifier = Modifier
                        .size(32.dp)
                        .clip(CircleShape)
                        .background(VoxaColors.IconChip)
                        .clickable {
                            (context as? Activity)?.finish()
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = "✕",
                        fontSize = 14.sp,
                        color = VoxaColors.Ink,
                    )
                }
            }

            // Hairline divider
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(1.dp)
                    .background(VoxaColors.HairSoft)
            )

            // Native helper banner — explains the embedded ChatGPT page.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(VoxaColors.SurfaceAlt)
                    .padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .size(18.dp)
                        .clip(CircleShape)
                        .background(VoxaColors.Primary),
                    contentAlignment = Alignment.Center,
                ) {
                    Text("i", color = androidx.compose.ui.graphics.Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
                Spacer(Modifier.width(10.dp))
                Text(
                    text = "ChatGPT's sign-in page. Sign in normally. Voxa closes this view automatically once your session is captured.",
                    fontSize = 12.sp,
                    color = VoxaColors.InkSoft,
                    lineHeight = 16.sp,
                )
            }
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(1.dp)
                    .background(VoxaColors.HairSoft)
            )

            // WebView (edge-to-edge, fills the rest)
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                AndroidView(
                    modifier = Modifier.fillMaxSize(),
                    factory = { ctx ->
                        val wv = WebView(ctx)
                        wv.settings.apply {
                            javaScriptEnabled = true
                            domStorageEnabled = true
                            databaseEnabled = true
                            useWideViewPort = true
                            loadWithOverviewMode = true
                            builtInZoomControls = false
                            displayZoomControls = false
                            setSupportZoom(false)
                            mediaPlaybackRequiresUserGesture = false
                            mixedContentMode = android.webkit.WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
                            @Suppress("DEPRECATION")
                            allowFileAccessFromFileURLs = false
                            userAgentString = CHROME_UA
                        }
                        CookieManager.getInstance().apply {
                            setAcceptCookie(true)
                            setAcceptThirdPartyCookies(wv, true)
                        }
                        wv.webViewClient = object : WebViewClient() {
                            override fun onPageFinished(view: WebView?, url: String?) {
                                loading = false
                                if (url != null && url.contains("chatgpt.com/auth", ignoreCase = true)) {
                                    view?.evaluateJavascript(AUTH_CLEANUP_PAYLOAD, null)
                                }
                                url?.let { tryCaptureSession(it) }
                            }
                            override fun shouldOverrideUrlLoading(
                                view: WebView?,
                                request: WebResourceRequest?,
                            ): Boolean {
                                request?.url?.toString()?.let { tryCaptureSession(it) }
                                return false
                            }
                        }
                        wv.loadUrl(LOGIN_URL)
                        wv
                    }
                )

                if (loading) {
                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(VoxaColors.Surface),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                    ) {
                        CircularProgressIndicator(color = VoxaColors.Primary)
                        Spacer(Modifier.height(12.dp))
                        Text(
                            text = "Loading sign-in",
                            fontSize = 13.sp,
                            color = VoxaColors.Muted,
                        )
                    }
                }

                // Center "linking" overlay while we capture the session cookie.
                if (linking) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(VoxaColors.Surface.copy(alpha = 0.92f)),
                        contentAlignment = Alignment.Center,
                    ) {
                        Column(
                            modifier = Modifier
                                .padding(24.dp)
                                .clip(RoundedCornerShape(14.dp))
                                .background(VoxaColors.Surface)
                                .border(1.dp, VoxaColors.Hair, RoundedCornerShape(14.dp))
                                .padding(horizontal = 22.dp, vertical = 20.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            CircularProgressIndicator(
                                color = VoxaColors.Primary,
                                strokeWidth = 3.dp,
                                modifier = Modifier.size(40.dp),
                            )
                            Spacer(Modifier.height(12.dp))
                            Text(
                                text = "Linking your ChatGPT account",
                                fontSize = 15.sp,
                                fontWeight = FontWeight.SemiBold,
                                color = VoxaColors.Ink,
                                textAlign = TextAlign.Center,
                            )
                            Spacer(Modifier.height(4.dp))
                            Text(
                                text = "Hang on a moment. Don't close the app.",
                                fontSize = 12.5.sp,
                                color = VoxaColors.Muted,
                                textAlign = TextAlign.Center,
                            )
                        }
                    }
                }
            }

            // Bottom progress strip
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(VoxaColors.SurfaceAlt)
                    .padding(horizontal = 14.dp, vertical = 11.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(
                    color = VoxaColors.Primary,
                    strokeWidth = 2.dp,
                    modifier = Modifier.size(14.dp),
                )
                Spacer(Modifier.width(10.dp))
                Text(
                    text = if (linking) "Linking your ChatGPT account" else "Waiting for sign-in",
                    fontSize = 12.sp,
                    color = VoxaColors.Muted,
                )
            }
        }
    }
}
