package com.voxa.android.service

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.inputmethodservice.InputMethodService
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.voxa.android.VoxaApp
import com.voxa.android.data.AudioRecorder
import com.voxa.android.network.TranscriptionApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

class VoxaInputMethod : InputMethodService() {

    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private val recorder by lazy { AudioRecorder(this) }

    private var isRecording = false
    private var isProcessing = false

    private var micBtn: FrameLayout? = null
    private var micLabel: TextView? = null
    private var statusLabel: TextView? = null
    private var waveRow: LinearLayout? = null

    override fun onCreateInputView(): View = buildImeView()

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        resetState()
    }

    override fun onDestroy() {
        super.onDestroy()
        recorder.release()
        scope.cancel()
    }

    private fun hasMicPermission() = ContextCompat.checkSelfPermission(
        this, Manifest.permission.RECORD_AUDIO
    ) == PackageManager.PERMISSION_GRANTED

    private fun resetState() {
        isRecording = false
        isProcessing = false
        micLabel?.text = "🎤"
        setMicColor(COLOR_AMBER)
        setWaveActive(false)
        statusLabel?.text = when {
            !hasMicPermission() -> "Open Voxa app → grant microphone permission"
            VoxaApp.prefs.getSessionCookie() == null -> "Open Voxa app → sign in to ChatGPT"
            else -> "Tap mic to start recording"
        }
    }

    private fun handleMicTap(view: View) {
        if (isProcessing) return
        view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)

        if (!hasMicPermission()) {
            statusLabel?.text = "Open Voxa app → grant microphone permission"
            return
        }
        if (VoxaApp.prefs.getSessionCookie() == null) {
            statusLabel?.text = "Open Voxa app → sign in to ChatGPT"
            return
        }

        if (!isRecording) {
            isRecording = true
            micLabel?.text = "🎙"
            statusLabel?.text = "Recording… tap to stop"
            setMicColor(COLOR_RED)
            setWaveActive(true)
            scope.launch {
                try {
                    recorder.start()
                } catch (e: Exception) {
                    isRecording = false
                    resetState()
                    statusLabel?.text = "Mic error: ${e.message}"
                }
            }
        } else {
            isRecording = false
            isProcessing = true
            micLabel?.text = "⏳"
            statusLabel?.text = "Transcribing…"
            setMicColor(COLOR_BRIGHT)
            setWaveActive(false)
            scope.launch {
                try {
                    val path = recorder.stop()
                    val text = TranscriptionApi.transcribeAudio(path)
                    val ic = currentInputConnection
                    if (!text.isNullOrEmpty() && ic != null) {
                        ic.commitText(text, text.length)
                    }
                    isProcessing = false
                    resetState()
                    if (text.isNullOrEmpty()) statusLabel?.text = "No speech detected"
                } catch (e: Exception) {
                    isProcessing = false
                    resetState()
                    statusLabel?.text = if (e.message == "NOT_LOGGED_IN")
                        "Open Voxa app → sign in to ChatGPT"
                    else "Error: ${e.message}"
                }
            }
        }
    }

    private fun buildImeView(): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#0D0B08"))
            gravity = Gravity.CENTER_HORIZONTAL
            setPadding(0, dp(24), 0, dp(32))
        }

        statusLabel = TextView(this).apply {
            text = "Tap mic to start recording"
            textSize = 13f
            setTextColor(Color.parseColor("#8A7862"))
            gravity = Gravity.CENTER
            setPadding(dp(16), 0, dp(16), 0)
        }
        root.addView(statusLabel, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { bottomMargin = dp(16) })

        val orbSize = dp(80)
        micBtn = FrameLayout(this).apply {
            layoutParams = LinearLayout.LayoutParams(orbSize, orbSize)
            background = buildCircle(COLOR_AMBER)
            isClickable = true
            isFocusable = true
            setOnClickListener { handleMicTap(it) }
        }
        micLabel = TextView(this).apply {
            text = "🎤"
            textSize = 28f
            gravity = Gravity.CENTER
            layoutParams = FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT
            )
        }
        micBtn!!.addView(micLabel)
        waveRow = buildWaveRow()

        root.addView(micBtn)
        root.addView(waveRow, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT, dp(20)
        ).apply { topMargin = dp(10) })

        val doneBtn = TextView(this).apply {
            text = "Done"
            textSize = 14f
            setTextColor(Color.parseColor("#C97D2E"))
            setPadding(dp(24), dp(8), dp(24), dp(8))
            background = buildPill(Color.parseColor("#1E1A14"), Color.parseColor("#2A231B"))
            setOnClickListener { requestHideSelf(0) }
        }
        val bottomRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER
            setPadding(0, dp(20), 0, 0)
            addView(doneBtn)
        }
        root.addView(bottomRow)
        return root
    }

    private fun buildWaveRow(): LinearLayout {
        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            for (i in 0 until 5) {
                val bar = View(this@VoxaInputMethod).apply {
                    background = buildRect(COLOR_AMBER)
                }
                addView(bar, LinearLayout.LayoutParams(dp(3), dp(4)).apply {
                    if (i > 0) leftMargin = dp(4)
                })
            }
        }
    }

    private fun setWaveActive(active: Boolean) {
        val row = waveRow ?: return
        val heights = if (active) intArrayOf(dp(8), dp(16), dp(20), dp(16), dp(8))
                      else        intArrayOf(dp(4),  dp(4),  dp(4),  dp(4),  dp(4))
        for (i in 0 until row.childCount) {
            val p = row.getChildAt(i).layoutParams as LinearLayout.LayoutParams
            p.height = heights[i]
            row.getChildAt(i).layoutParams = p
        }
    }

    private fun setMicColor(color: Int) { micBtn?.background = buildCircle(color) }
    private fun buildCircle(color: Int) = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(color) }
    private fun buildRect(color: Int) = GradientDrawable().apply { shape = GradientDrawable.RECTANGLE; cornerRadius = dp(2).toFloat(); setColor(color) }
    private fun buildPill(fill: Int, stroke: Int) = GradientDrawable().apply { shape = GradientDrawable.RECTANGLE; cornerRadius = dp(20).toFloat(); setColor(fill); setStroke(dp(1), stroke) }
    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    companion object {
        private val COLOR_AMBER  = Color.parseColor("#C97D2E")
        private val COLOR_RED    = Color.parseColor("#D94030")
        private val COLOR_BRIGHT = Color.parseColor("#E09840")
    }
}