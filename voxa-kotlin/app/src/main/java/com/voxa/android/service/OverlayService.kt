package com.voxa.android.service

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.PixelFormat
import android.os.IBinder
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import androidx.core.app.NotificationCompat
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.VoxaApp
import com.voxa.android.data.AudioRecorder
import com.voxa.android.network.TranscriptionApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class OverlayService : Service() {

    private lateinit var windowManager: WindowManager
    private var overlayView: View? = null
    private val recorder = lazy { AudioRecorder(this) }
    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())

    private var isRecording = false
    private var isProcessing = false

    override fun onBind(intent: IBinder?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        startForegroundWithNotification()
        showFloatingButton()
    }

    override fun onDestroy() {
        super.onDestroy()
        overlayView?.let { windowManager.removeView(it) }
        overlayView = null
        recorder.value.release()
        scope.cancel()
    }

    private fun startForegroundWithNotification() {
        val tapIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE
        )
        val notification: Notification = NotificationCompat.Builder(this, VoxaApp.OVERLAY_CHANNEL_ID)
            .setContentTitle(getString(R.string.overlay_notification_title))
            .setContentText(getString(R.string.overlay_notification_text))
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentIntent(tapIntent)
            .setOngoing(true)
            .build()

        startForeground(VoxaApp.OVERLAY_NOTIFICATION_ID, notification)
    }

    private fun showFloatingButton() {
        val params = WindowManager.LayoutParams(
            dpToPx(64), dpToPx(64),
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
            y = dpToPx(80)
        }

        val btn = buildMicButton()

        var initialX = 0; var initialY = 0
        var touchX = 0f; var touchY = 0f
        var moved = false

        btn.setOnTouchListener { v, event ->
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    initialX = params.x; initialY = params.y
                    touchX = event.rawX; touchY = event.rawY
                    moved = false
                    true
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = (event.rawX - touchX).toInt()
                    val dy = (event.rawY - touchY).toInt()
                    if (Math.abs(dx) > 8 || Math.abs(dy) > 8) moved = true
                    params.x = initialX + dx
                    params.y = initialY - dy
                    windowManager.updateViewLayout(btn, params)
                    true
                }
                MotionEvent.ACTION_UP -> {
                    if (!moved) handleMicTap(btn)
                    true
                }
                else -> false
            }
        }

        overlayView = btn
        windowManager.addView(btn, params)
    }

    private fun buildMicButton(): FrameLayout {
        val size = dpToPx(56)
        val frame = FrameLayout(this).apply {
            layoutParams = FrameLayout.LayoutParams(size, size)
        }

        val circle = View(this).apply {
            background = buildCircleDrawable(Color.parseColor("#C97D2E"))
            layoutParams = FrameLayout.LayoutParams(size, size)
        }

        val label = TextView(this).apply {
            text = "🎤"
            textSize = 22f
            gravity = Gravity.CENTER
            layoutParams = FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT
            )
        }

        frame.addView(circle)
        frame.addView(label)
        return frame
    }

    private fun handleMicTap(btn: FrameLayout) {
        if (isProcessing) return

        val label = btn.getChildAt(1) as TextView
        val circle = btn.getChildAt(0)

        if (!isRecording) {
            isRecording = true
            label.text = "🎙"
            circle.background = buildCircleDrawable(Color.parseColor("#D94030"))
            scope.launch {
                try {
                    recorder.value.start()
                } catch (e: Exception) {
                    isRecording = false
                    label.text = "🎤"
                    circle.background = buildCircleDrawable(Color.parseColor("#C97D2E"))
                    Toast.makeText(this@OverlayService, "Recording failed: ${e.message}", Toast.LENGTH_SHORT).show()
                }
            }
        } else {
            isRecording = false
            isProcessing = true
            label.text = "⏳"
            circle.background = buildCircleDrawable(Color.parseColor("#E09840"))
            scope.launch {
                try {
                    val path = recorder.value.stop()
                    val text = TranscriptionApi.transcribeAudio(path)
                    if (text.isNotEmpty()) {
                        copyToClipboard(text)
                        broadcastTranscript(text)
                        Toast.makeText(this@OverlayService, "Copied: $text", Toast.LENGTH_LONG).show()
                    } else {
                        Toast.makeText(this@OverlayService, "No speech detected", Toast.LENGTH_SHORT).show()
                    }
                } catch (e: Exception) {
                    Toast.makeText(this@OverlayService, "Error: ${e.message}", Toast.LENGTH_SHORT).show()
                } finally {
                    isProcessing = false
                    label.text = "🎤"
                    circle.background = buildCircleDrawable(Color.parseColor("#C97D2E"))
                }
            }
        }
    }

    private fun copyToClipboard(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("voxa_transcript", text))
    }

    private fun broadcastTranscript(text: String) {
        val intent = Intent(VoxaApp.ACTION_TRANSCRIPT).apply {
            putExtra(VoxaApp.EXTRA_TRANSCRIPT, text)
            setPackage(packageName)
        }
        sendBroadcast(intent)
    }

    private fun buildCircleDrawable(color: Int) =
        android.graphics.drawable.GradientDrawable().apply {
            shape = android.graphics.drawable.GradientDrawable.OVAL
            setColor(color)
        }

    private fun dpToPx(dp: Int): Int =
        (dp * resources.displayMetrics.density).toInt()

    companion object {
        fun start(context: Context) {
            context.startForegroundService(Intent(context, OverlayService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, OverlayService::class.java))
        }
    }
}
