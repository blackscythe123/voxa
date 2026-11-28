package com.voxa.android.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The ChatGPT session (full cookie header and the User-Agent it was issued to), encrypted with an
 * AES key that never leaves the Android Keystore. Values are never logged.
 */
class SessionVault(context: Context) {
    data class Saved(val cookieHeader: String, val userAgent: String)

    private val file = File(context.filesDir, "session.bin")

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    /** Atomic: written to a temporary file of its own, synced to disk, then renamed over the vault,
     *  so a reader never sees half of one save and half of another. Callers serialise saves (VoxaApp's lock). */
    @Synchronized
    fun save(s: Saved) {
        val plain = JSONObject().put("cookie_header", s.cookieHeader).put("user_agent", s.userAgent).toString().toByteArray()
        val c = Cipher.getInstance(TRANSFORM).apply { init(Cipher.ENCRYPT_MODE, key()) }
        val sealed = c.iv + c.doFinal(plain)
        val tmp = File.createTempFile("session.bin.", ".tmp", file.parentFile)
        try {
            FileOutputStream(tmp).use { out ->
                out.write(sealed)
                out.flush()
                out.fd.sync()
            }
            check(tmp.renameTo(file)) { "rename failed" }
        } finally {
            tmp.delete()
        }
    }

    fun load(): Saved? = runCatching {
        if (!file.exists()) return null
        val bytes = file.readBytes()
        val c = Cipher.getInstance(TRANSFORM).apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12)) }
        val json = JSONObject(String(c.doFinal(bytes, 12, bytes.size - 12)))
        Saved(json.getString("cookie_header"), json.getString("user_agent"))
    }.getOrNull()

    @Synchronized
    fun clear() {
        file.delete()
    }

    private companion object {
        const val ALIAS = "voxa-session"
        const val TRANSFORM = "AES/GCM/NoPadding"
    }
}

/**
 * NextAuth splits large session tokens across `__Secure-next-auth.session-token.0`, `.1`, ...
 * Whether the cookie header holds a complete session token (single or every chunk from 0).
 * From the old app's AuthScreen.
 */
fun hasSessionToken(cookieHeader: String): Boolean {
    val base = "__Secure-next-auth.session-token"
    val pairs = cookieHeader.split(";").mapNotNull {
        val kv = it.trim()
        val i = kv.indexOf('=')
        if (i <= 0) null else kv.substring(0, i) to kv.substring(i + 1).trim()
    }
    if (pairs.any { it.first == base && it.second.isNotEmpty() }) return true
    val chunks = pairs.mapNotNull { (k, v) ->
        if (!k.startsWith("$base.") || v.isEmpty()) null else k.removePrefix("$base.").toIntOrNull()
    }.sorted()
    return chunks.isNotEmpty() && chunks == chunks.indices.toList()
}
