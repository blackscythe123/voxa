package com.voxa.android.notify

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.VoxaApp
import com.voxa.android.diag.Diag
import uniffi.voxa_ffi.NoticeDecision
import uniffi.voxa_ffi.NoticeKind
import uniffi.voxa_ffi.SessionCheck
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.noticeDecide
import uniffi.voxa_ffi.noticeNotSent
import uniffi.voxa_ffi.noticePublicTitle
import uniffi.voxa_ffi.noticeQuietHour
import uniffi.voxa_ffi.noticeReady
import uniffi.voxa_ffi.noticeRecord
import uniffi.voxa_ffi.noticeSignedOut
import java.time.LocalTime

/**
 * Voxa's notifications (docs/notifications.md). Two kinds, each one notification updated in place:
 * A1 "signed out" (ID 1, channel Sign-in) and A2/A3 dictations ready or not sent (ID 2, channel
 * Dictations). Words and anti-spam rules come from the core (`voxa_core::notices`). Never shows
 * transcript text, cookies or the account email; the lock screen shows only "Voxa needs you".
 */
object Notifier {
    const val ID_SIGN_IN = 1
    const val ID_DICTATIONS = 2

    /** A notification channel. A third, "Reminders", can be added to [CHANNELS] later. */
    data class Channel(val id: String, val name: String, val description: String, val importance: Int)

    val SIGN_IN = Channel("sign-in", "Sign-in", "When you need to sign in to ChatGPT again", NotificationManager.IMPORTANCE_DEFAULT)
    val DICTATIONS = Channel("dictations", "Dictations", "Dictations sent in the background, or that couldn't be sent", NotificationManager.IMPORTANCE_LOW)
    private val CHANNELS = listOf(SIGN_IN, DICTATIONS)

    /** The Voxa keyboard is on screen (set by the keyboard). */
    @Volatile var keyboardShowing = false

    /** One of the app's screens is in front (set by MainActivity). */
    @Volatile var activityResumed = false

    /** Never post while the user can see Voxa: they already see what the notification would say. */
    private val looking get() = keyboardShowing || activityResumed

    private const val CONFIRM_MS = 2 * 60_000L

    fun createChannels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        for (c in CHANNELS) nm.createNotificationChannel(NotificationChannel(c.id, c.name, c.importance).apply { description = c.description })
    }

    private fun prefs(ctx: Context) = ctx.getSharedPreferences("notify", Context.MODE_PRIVATE)

    fun allowed(ctx: Context) = NotificationManagerCompat.from(ctx).areNotificationsEnabled()

    private fun showing(ctx: Context, id: Int) =
        ctx.getSystemService(NotificationManager::class.java).activeNotifications.any { it.id == id }

    // ── A1: signed out ───────────────────────────────────────────────────────

    /** How long to wait before the confirming check (debug builds can shorten it for tests). */
    fun confirmDelayMs(app: VoxaApp): Long =
        if (app.debugCheck != null) app.getSharedPreferences("debug", Context.MODE_PRIVATE).getLong("confirm_ms", CONFIRM_MS) else CONFIRM_MS

    /**
     * After any session check. Returns true when the session just went from known good to a
     * definite "signed out": the caller confirms with a second check about 2 minutes later
     * ([confirm]) before anything is posted. `definite`: ChatGPT's session call answered with no
     * token (not Cloudflare, not offline, not a failed transcription).
     */
    fun afterCheck(ctx: Context, r: SessionCheck, definite: Boolean): Boolean {
        val p = prefs(ctx)
        val known = p.getString("known", "")
        return when {
            r.session == SessionState.VALID -> {
                if (known != "valid") p.edit().putString("known", "valid").apply()
                clearSignIn(ctx)
                false
            }
            r.session == SessionState.EXPIRED && definite -> {
                p.edit().putString("known", "expired").apply()
                if (known == "valid") Diag.log("notify", "session ended after being good: confirming before saying so")
                known == "valid"
            }
            else -> false
        }
    }

    /** The second check. Posts A1 only if it agrees and nothing changed meanwhile. */
    fun confirm(app: VoxaApp, r: SessionCheck, definite: Boolean) {
        val known = prefs(app).getString("known", "")
        Diag.log("notify", "confirming check: ${r.session}, definite $definite, known $known")
        if (r.session == SessionState.EXPIRED && definite && known == "expired") signedOut(app)
        else afterCheck(app, r, definite)
    }

    private fun signedOut(app: VoxaApp) {
        val waiting = app.core.waitingForSignIn()
        post(app, NoticeKind.SIGN_IN, ID_SIGN_IN) { silent -> signInNotice(app, waiting, silent) }
    }

    /** Update the waiting count on a sign-in notification already showing (silently). */
    fun refreshSignIn(app: VoxaApp) {
        if (!showing(app, ID_SIGN_IN)) return
        NotificationManagerCompat.from(app).safeNotify(app, ID_SIGN_IN, signInNotice(app, app.core.waitingForSignIn(), true))
    }

    private fun signInNotice(ctx: Context, waiting: UInt, silent: Boolean): android.app.Notification {
        val t = noticeSignedOut(waiting)
        val open = PendingIntent.getActivity(
            ctx, ID_SIGN_IN,
            Intent(ctx, MainActivity::class.java).putExtra(MainActivity.EXTRA_OPEN, MainActivity.OPEN_SIGN_IN).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return builder(ctx, SIGN_IN, t.title, t.body, silent)
            .setContentIntent(open)
            .addAction(0, t.action ?: "Sign in", open)
            .build()
    }

    /** The session is good again: the sign-in notification goes. */
    fun clearSignIn(ctx: Context) {
        NotificationManagerCompat.from(ctx).cancel(ID_SIGN_IN)
    }

    /** VoxaApp.signOut: on purpose, so never a "signed out" notification for it. */
    fun signedOutOnPurpose(ctx: Context) {
        prefs(ctx).edit().remove("known").apply()
        clearSignIn(ctx)
    }

    // ── A2 / A3: dictations ──────────────────────────────────────────────────

    /** A2: dictations that weren't on screen were written out and wait in History. */
    fun ready(ctx: Context, entries: List<ULong>, app: String) {
        if (entries.isEmpty()) return
        val p = prefs(ctx)
        // Still showing "ready": add to its count; otherwise (gone, or a "couldn't send") start over.
        val before = if (showing(ctx, ID_DICTATIONS) && p.getString("dict_kind", "") == "ready") ids(p.getString("ready_ids", "")) else emptyList()
        val all = (before + entries).distinct()
        val posted = post(ctx, NoticeKind.DICTATIONS, ID_DICTATIONS) { silent -> readyNotice(ctx, all, app, silent) }
        if (posted) p.edit().putString("dict_kind", "ready").putString("ready_ids", all.joinToString(",")).apply()
    }

    private fun readyNotice(ctx: Context, ids: List<ULong>, app: String, silent: Boolean): android.app.Notification {
        val t = noticeReady(ids.size.toUInt(), app)
        val one = ids.singleOrNull()
        val open = activityIntent(ctx, if (one != null) "${MainActivity.OPEN_ENTRY}${one}" else MainActivity.OPEN_HISTORY)
        val b = builder(ctx, DICTATIONS, t.title, t.body, silent).setContentIntent(open)
        if (one != null && t.action != null) b.addAction(0, t.action, NotifyReceiver.copy(ctx, one))
        return b.build()
    }

    /** A3: background sends that gave up. Most recent wins over a "ready" notice. */
    fun notSent(ctx: Context, count: Int) {
        if (count <= 0) return
        val p = prefs(ctx)
        val before = if (showing(ctx, ID_DICTATIONS) && p.getString("dict_kind", "") == "failed") p.getInt("failed_count", 0) else 0
        val n = before + count
        val posted = post(ctx, NoticeKind.DICTATIONS, ID_DICTATIONS) { silent ->
            val t = noticeNotSent(n.toUInt())
            builder(ctx, DICTATIONS, t.title, t.body, silent)
                .setContentIntent(activityIntent(ctx, MainActivity.OPEN_HISTORY))
                .addAction(0, t.action ?: "Retry", NotifyReceiver.retry(ctx))
                .build()
        }
        if (posted) p.edit().putString("dict_kind", "failed").putInt("failed_count", n).apply()
    }

    /** History or the entry was opened, it was copied, deleted or sent: the dictations notice goes. */
    fun clearDictations(ctx: Context) {
        NotificationManagerCompat.from(ctx).cancel(ID_DICTATIONS)
        prefs(ctx).edit().remove("dict_kind").remove("ready_ids").remove("failed_count").apply()
    }

    // ── posting ──────────────────────────────────────────────────────────────

    /** The anti-spam rules, then post or update in place. True when it was posted. */
    private fun post(ctx: Context, kind: NoticeKind, id: Int, build: (silent: Boolean) -> android.app.Notification): Boolean {
        if (looking) {
            Diag.log("notify", "$kind not posted: Voxa is on screen")
            return false
        }
        if (!allowed(ctx)) {
            Diag.log("notify", "$kind not posted: notifications are off for Voxa")
            return false
        }
        val p = prefs(ctx)
        val now = System.currentTimeMillis()
        val posted = ids(p.getString("posted", ""))
        val lastDictations = p.getLong("last_dictations", 0L).toULong()
        val decision = noticeDecide(kind, now.toULong(), showing(ctx, id), posted, lastDictations)
        Diag.log("notify", "$kind: $decision")
        when (decision) {
            NoticeDecision.SKIP -> return false
            NoticeDecision.UPDATE -> NotificationManagerCompat.from(ctx).safeNotify(ctx, id, build(true))
            NoticeDecision.POST -> {
                // Sign-in at night arrives silently; dictations are a quiet channel anyway.
                val quiet = kind == NoticeKind.SIGN_IN && noticeQuietHour(LocalTime.now().hour.toUInt())
                if (!NotificationManagerCompat.from(ctx).safeNotify(ctx, id, build(quiet))) return false
                val e = p.edit().putString("posted", noticeRecord(posted, now.toULong()).joinToString(","))
                if (kind == NoticeKind.DICTATIONS) e.putLong("last_dictations", now)
                e.apply()
            }
        }
        return true
    }

    private fun builder(ctx: Context, ch: Channel, title: String, body: String, silent: Boolean): NotificationCompat.Builder {
        val public = NotificationCompat.Builder(ctx, ch.id)
            .setSmallIcon(R.drawable.ic_notify)
            .setContentTitle(noticePublicTitle())
            .build()
        return NotificationCompat.Builder(ctx, ch.id)
            .setSmallIcon(R.drawable.ic_notify)
            .setColor(0xFF8F4C2C.toInt())
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setSilent(silent)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setPublicVersion(public)
    }

    private fun activityIntent(ctx: Context, open: String): PendingIntent = PendingIntent.getActivity(
        ctx, ID_DICTATIONS,
        Intent(ctx, MainActivity::class.java).putExtra(MainActivity.EXTRA_OPEN, open).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun NotificationManagerCompat.safeNotify(ctx: Context, id: Int, n: android.app.Notification): Boolean {
        if (Build.VERSION.SDK_INT >= 33 && ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return false
        return runCatching { notify(id, n) }.onFailure { Diag.log("notify", "post failed: ${it.javaClass.simpleName}") }.isSuccess
    }

    private fun ids(s: String?): List<ULong> = s.orEmpty().split(',').mapNotNull { it.trim().toULongOrNull() }

    // ── asking for the permission (Android 13+) ──────────────────────────────

    /** The app may offer the "notify me" card: Android 13+, not allowed yet, asked fewer than twice. */
    fun shouldAsk(ctx: Context): Boolean =
        Build.VERSION.SDK_INT >= 33 && !allowed(ctx) && prefs(ctx).getInt("asks", 0) < 2

    /** The user answered a card (Allow or Not now). */
    fun asked(ctx: Context) {
        val p = prefs(ctx)
        p.edit().putInt("asks", p.getInt("asks", 0) + 1).apply()
    }

    fun homeCardDismissed(ctx: Context) = prefs(ctx).getBoolean("home_card_done", false)

    fun dismissHomeCard(ctx: Context) {
        prefs(ctx).edit().putBoolean("home_card_done", true).apply()
    }
}
