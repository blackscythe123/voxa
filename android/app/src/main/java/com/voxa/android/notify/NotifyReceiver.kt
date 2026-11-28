package com.voxa.android.notify

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import com.voxa.android.diag.Diag
import com.voxa.android.voxa

/**
 * The buttons on dictation notifications. Not exported: only Voxa's own notifications can reach
 * it. "Copy" reads the text from History by entry id (the text is never in the notification);
 * "Retry" sends the dictations that couldn't be sent again.
 */
class NotifyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val app = context.voxa
        when (intent.action) {
            ACTION_COPY -> {
                val id = intent.getLongExtra(EXTRA_ENTRY, 0L).toULong()
                val text = app.core.history("").firstOrNull { it.id == id }?.text.orEmpty()
                if (text.isNotEmpty()) {
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Dictation", text))
                    Diag.log("notify", "copy: dictation copied (${text.length} chars)")
                } else {
                    Diag.log("notify", "copy: entry gone or empty")
                }
                Notifier.clearDictations(context)
            }
            ACTION_RETRY -> {
                Diag.log("notify", "retry: sending again")
                VoxaJobs.scheduleSend(context, manual = true)
                Notifier.clearDictations(context)
            }
        }
    }

    companion object {
        const val ACTION_COPY = "com.voxa.android.notify.COPY"
        const val ACTION_RETRY = "com.voxa.android.notify.RETRY"
        const val EXTRA_ENTRY = "entry"

        fun copy(ctx: Context, entry: ULong): PendingIntent = PendingIntent.getBroadcast(
            ctx, 10,
            Intent(ctx, NotifyReceiver::class.java).setAction(ACTION_COPY).putExtra(EXTRA_ENTRY, entry.toLong()),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        fun retry(ctx: Context): PendingIntent = PendingIntent.getBroadcast(
            ctx, 11,
            Intent(ctx, NotifyReceiver::class.java).setAction(ACTION_RETRY),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}
