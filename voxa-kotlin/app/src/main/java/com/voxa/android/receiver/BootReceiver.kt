package com.voxa.android.receiver

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import com.voxa.android.VoxaApp
import com.voxa.android.service.OverlayService

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        if (VoxaApp.prefs.getAutoStart() && VoxaApp.prefs.getSessionCookie() != null) {
            OverlayService.start(context)
        }
    }
}
