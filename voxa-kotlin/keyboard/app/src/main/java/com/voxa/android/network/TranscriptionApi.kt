package com.voxa.android.network

import com.voxa.android.data.Prefs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.asRequestBody
import org.json.JSONObject
import java.io.File
import java.util.concurrent.TimeUnit

object TranscriptionApi {

    private const val SESSION_URL = "https://chatgpt.com/api/auth/session"
    private const val TRANSCRIBE_URL = "https://chatgpt.com/backend-api/transcribe"
    private const val UA =
        "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36"

    private val client = OkHttpClient.Builder()
        .connectTimeout(30, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .build()

    /** Pull the full cookie header (preferred — passes cf_clearance for Cloudflare) or fall back to bare session-token. */
    private fun buildCookieHeader(): String {
        val prefs = Prefs.get()
        prefs.getCookieHeader()?.takeIf { it.isNotEmpty() }?.let { return it }
        val token = prefs.getSessionCookie() ?: throw Exception("NOT_LOGGED_IN")
        return "__Secure-next-auth.session-token=$token"
    }

    private suspend fun getAccessToken(): String = withContext(Dispatchers.IO) {
        val prefs = Prefs.get()
        val cookieHeader = buildCookieHeader()
        val deviceId = prefs.getDeviceId()
        val sessionId = prefs.getOaiSessionId()

        val request = Request.Builder()
            .url(SESSION_URL)
            .get()
            .header("Cookie", cookieHeader)
            .header("User-Agent", UA)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Cache-Control", "no-cache")
            .header("Origin", "https://chatgpt.com")
            .header("Referer", "https://chatgpt.com/")
            .header("oai-language", "en-US")
            .header("oai-device-id", deviceId)
            .header("oai-session-id", sessionId)
            .build()

        val response = client.newCall(request).execute()
        if (response.code == 401 || response.code == 403) throw Exception("SESSION_EXPIRED")

        val body = response.body?.string() ?: throw Exception("Empty response")
        // Cloudflare challenge returns HTML, not JSON
        if (body.trimStart().startsWith("<")) throw Exception("CLOUDFLARE_CHALLENGE")

        val json = try { JSONObject(body) } catch (e: Exception) { throw Exception("Invalid response") }
        json.optString("accessToken").takeIf { it.isNotEmpty() }
            ?: throw Exception("NO_ACCESS_TOKEN")
    }

    suspend fun validateSession(): Boolean = try {
        getAccessToken()
        true
    } catch (_: Exception) {
        false
    }

    suspend fun transcribeAudio(filePath: String): String = withContext(Dispatchers.IO) {
        val accessToken = getAccessToken()
        val prefs = Prefs.get()
        val cookieHeader = buildCookieHeader()
        val deviceId = prefs.getDeviceId()
        val sessionId = prefs.getOaiSessionId()

        val audioFile = File(filePath)
        val body = MultipartBody.Builder()
            .setType(MultipartBody.FORM)
            .addFormDataPart(
                "file",
                "recording.webm",
                audioFile.asRequestBody("audio/webm".toMediaType())
            )
            .addFormDataPart("model", "whisper-1")
            .build()

        val request = Request.Builder()
            .url(TRANSCRIBE_URL)
            .post(body)
            .header("Authorization", "Bearer $accessToken")
            .header("Cookie", cookieHeader)
            .header("User-Agent", UA)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("oai-language", "en-US")
            .header("oai-device-id", deviceId)
            .header("oai-session-id", sessionId)
            .header("oai-client-build-number", "5503767")
            .header("x-openai-target-path", "/backend-api/transcribe")
            .header("x-openai-target-route", "/backend-api/transcribe")
            .build()

        val response = client.newCall(request).execute()
        val text = response.body?.string() ?: ""
        if (!response.isSuccessful) throw Exception("Transcription failed (${response.code}): ${text.take(200)}")

        val json = JSONObject(text)
        json.optString("text").takeIf { it.isNotEmpty() }
            ?: json.optString("transcript").takeIf { it.isNotEmpty() }
            ?: json.optString("message")
            ?: ""
    }
}