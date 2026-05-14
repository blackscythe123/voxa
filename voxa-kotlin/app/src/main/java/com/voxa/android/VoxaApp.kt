package com.voxa.android

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import com.voxa.android.data.Prefs

class VoxaApp : Application() {

    override fun onCreate() {
        super.onCreate()
        prefs = Prefs(this)
        createNotificationChannel()
    }

    private fun createNotificationChannel() {
        val channel = NotificationChannel(
            OVERLAY_CHANNEL_ID,
            getString(R.string.channel_overlay),
            NotificationManager.IMPORTANCE_LOW
        ).apply {
            description = "Voxa floating mic overlay"
            setShowBadge(false)
        }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    companion object {
        const val OVERLAY_CHANNEL_ID = "voxa_overlay"
        const val OVERLAY_NOTIFICATION_ID = 1001
        const val ACTION_TRANSCRIPT = "com.voxa.android.TRANSCRIPT"
        const val EXTRA_TRANSCRIPT = "transcript"

        lateinit var prefs: Prefs
            private set
    }
}
