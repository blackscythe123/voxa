package com.voxa.android.service

import android.Manifest
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.drawable.GradientDrawable
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
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
import kotlin.math.max

class VoxaInputMethod : InputMethodService() {

    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private val recorder by lazy { AudioRecorder(this) }
    private val handler = Handler(Looper.getMainLooper())

    private enum class Mode { Idle, Recording, Paused, Processing }
    private var mode = Mode.Idle
    private var recordStartMs = 0L
    private var elapsedBeforePauseSec = 0
    private var pauseStartMs = 0L

    private var statusDotView: View? = null
    private var statusTextView: TextView? = null
    private var timerView: TextView? = null
    private var waveView: WaveformView? = null
    private var waveStage: FrameLayout? = null
    private var micButton: FrameLayout? = null
    private var micCenter: View? = null
    private var hintView: TextView? = null
    private var doneBtn: TextView? = null
    private var cancelBtn: TextView? = null
    private var sendBtn: TextView? = null

    private val amplitudePoll = object : Runnable {
        override fun run() {
            if (mode != Mode.Recording) return
            val amp = recorder.getMaxAmplitude()
            waveView?.pushAmplitude(amp)
            val elapsed = elapsedBeforePauseSec + ((System.currentTimeMillis() - recordStartMs) / 1000).toInt()
            timerView?.text = formatTimer(elapsed)
            val cap = VoxaApp.prefs.getMaxDuration()
            if (cap > 0 && elapsed >= cap) {
                handler.removeCallbacks(this)
                handleSend()
                return
            }
            handler.postDelayed(this, 50L)
        }
    }

    private fun currentElapsedSec(): Int = when (mode) {
        Mode.Recording -> elapsedBeforePauseSec + ((System.currentTimeMillis() - recordStartMs) / 1000).toInt()
        Mode.Paused -> elapsedBeforePauseSec
        else -> 0
    }

    private fun formatTimer(seconds: Int): String {
        val m = seconds / 60
        val s = seconds % 60
        return "%d:%02d".format(m, s)
    }

    override fun onCreateInputView(): View = buildImeView()

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        resetToIdle(statusMessage = null)
    }

    override fun onDestroy() {
        super.onDestroy()
        recorder.release()
        handler.removeCallbacks(amplitudePoll)
        scope.cancel()
    }

    private fun hasMicPermission() = ContextCompat.checkSelfPermission(
        this, Manifest.permission.RECORD_AUDIO
    ) == PackageManager.PERMISSION_GRANTED

    private fun resetToIdle(statusMessage: String?) {
        mode = Mode.Idle
        elapsedBeforePauseSec = 0
        handler.removeCallbacks(amplitudePoll)
        waveView?.clear()
        waveStage?.visibility = View.GONE
        timerView?.text = "0:00"
        setStatus(label = "Voxa ready", state = StatusState.Idle)
        setMicState(MicState.Idle)

        cancelBtn?.visibility = View.GONE
        sendBtn?.visibility = View.GONE
        doneBtn?.visibility = View.VISIBLE

        hintView?.visibility = View.VISIBLE
        hintView?.text = statusMessage ?: when {
            !hasMicPermission() -> "Open Voxa to grant microphone permission"
            VoxaApp.prefs.getSessionCookie() == null -> "Open Voxa to sign in to ChatGPT"
            else -> "Tap to dictate. Tap Done to switch back to your keyboard."
        }
    }

    private fun handleMicTap(view: View) {
        if (mode == Mode.Processing) return
        if (VoxaApp.prefs.getHaptics()) {
            view.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
        }

        if (!hasMicPermission()) {
            hintView?.visibility = View.VISIBLE
            hintView?.text = "Open Voxa to grant microphone permission"
            return
        }
        if (VoxaApp.prefs.getSessionCookie() == null) {
            hintView?.visibility = View.VISIBLE
            hintView?.text = "Open Voxa to sign in to ChatGPT"
            return
        }

        when (mode) {
            Mode.Idle -> startRecording()
            Mode.Recording -> pauseRecording()
            Mode.Paused -> resumeRecording()
            Mode.Processing -> {}
        }
    }

    private fun startRecording() {
        mode = Mode.Recording
        recordStartMs = System.currentTimeMillis()
        elapsedBeforePauseSec = 0
        setStatus(label = "Listening", state = StatusState.Recording)
        setMicState(MicState.Recording)

        hintView?.visibility = View.GONE
        doneBtn?.visibility = View.GONE
        cancelBtn?.visibility = View.VISIBLE
        sendBtn?.visibility = View.VISIBLE
        waveStage?.visibility = View.VISIBLE
        waveView?.clear()
        timerView?.text = "0:00"

        scope.launch {
            try {
                recorder.start()
                handler.post(amplitudePoll)
            } catch (e: Exception) {
                resetToIdle("Microphone error: ${e.message}")
            }
        }
    }

    private fun pauseRecording() {
        if (mode != Mode.Recording) return
        mode = Mode.Paused
        elapsedBeforePauseSec += ((System.currentTimeMillis() - recordStartMs) / 1000).toInt()
        pauseStartMs = System.currentTimeMillis()
        handler.removeCallbacks(amplitudePoll)
        recorder.pause()
        setStatus(label = "Paused", state = StatusState.Paused)
        setMicState(MicState.Paused)
        timerView?.text = formatTimer(elapsedBeforePauseSec)
    }

    private fun resumeRecording() {
        if (mode != Mode.Paused) return
        mode = Mode.Recording
        recordStartMs = System.currentTimeMillis()
        recorder.resume()
        setStatus(label = "Listening", state = StatusState.Recording)
        setMicState(MicState.Recording)
        handler.post(amplitudePoll)
    }

    private fun handleCancel() {
        if (mode != Mode.Recording && mode != Mode.Paused) return
        mode = Mode.Idle
        handler.removeCallbacks(amplitudePoll)
        scope.launch {
            try { recorder.stop() } catch (_: Exception) {}
            resetToIdle(statusMessage = "Cancelled. Tap to dictate again.")
        }
    }

    private fun handleSend() {
        if (mode != Mode.Recording && mode != Mode.Paused) return
        mode = Mode.Processing
        handler.removeCallbacks(amplitudePoll)
        setStatus(label = "Transcribing", state = StatusState.Processing)
        setMicState(MicState.Processing)

        cancelBtn?.visibility = View.GONE
        sendBtn?.visibility = View.GONE
        hintView?.visibility = View.GONE
        waveStage?.visibility = View.GONE

        scope.launch {
            try {
                val path = recorder.stop()
                val text = TranscriptionApi.transcribeAudio(path)
                val ic = currentInputConnection
                if (!text.isNullOrEmpty() && ic != null) {
                    ic.commitText(text, text.length)
                }
                mode = Mode.Idle
                resetToIdle(
                    statusMessage = if (text.isNullOrEmpty())
                        "No speech detected. Tap to try again."
                    else "Sent. Tap to dictate more, or Done to switch back."
                )
            } catch (e: Exception) {
                mode = Mode.Idle
                resetToIdle(
                    statusMessage = if (e.message == "NOT_LOGGED_IN")
                        "Open Voxa to sign in to ChatGPT"
                    else "Transcription error: ${e.message}"
                )
            }
        }
    }

    private fun handleDone() {
        if (mode == Mode.Recording || mode == Mode.Paused) handleCancel()
        val switched = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            switchToPreviousInputMethod()
        } else false
        if (!switched) {
            val imm = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager
            @Suppress("DEPRECATION")
            imm.showInputMethodPicker()
        }
    }

    // ─── view tree ────────────────────────────────────────────────────────────

    private enum class MicState { Idle, Recording, Paused, Processing }
    private enum class StatusState { Idle, Recording, Paused, Processing }

    private fun setMicState(state: MicState) {
        val bgColor = when (state) {
            MicState.Idle -> INK
            MicState.Recording -> RECORDING
            MicState.Paused -> Color.parseColor("#7A7A82")
            MicState.Processing -> PRIMARY
        }
        micButton?.background = circle(bgColor)
        val params = micCenter?.layoutParams as? FrameLayout.LayoutParams
        when (state) {
            MicState.Idle -> {
                micCenter?.background = micIconDrawable()
                params?.apply { width = dp(22f).toInt(); height = dp(26f).toInt(); gravity = Gravity.CENTER }
            }
            MicState.Recording -> {
                micCenter?.background = pauseBars()
                params?.apply { width = dp(16f).toInt(); height = dp(18f).toInt(); gravity = Gravity.CENTER }
            }
            MicState.Paused -> {
                micCenter?.background = playTriangle()
                params?.apply { width = dp(16f).toInt(); height = dp(18f).toInt(); gravity = Gravity.CENTER }
            }
            MicState.Processing -> {
                micCenter?.background = circle(Color.WHITE)
                params?.apply { width = dp(14f).toInt(); height = dp(14f).toInt(); gravity = Gravity.CENTER }
            }
        }
        micCenter?.layoutParams = params
        micCenter?.requestLayout()
    }

    private fun setStatus(label: String, state: StatusState) {
        statusTextView?.text = label
        val (dotColor, textColor) = when (state) {
            StatusState.Idle -> Color.parseColor("#A5A29A") to Color.parseColor("#7A7A82")
            StatusState.Recording -> RECORDING to RECORDING
            StatusState.Paused -> Color.parseColor("#7A7A82") to Color.parseColor("#7A7A82")
            StatusState.Processing -> PRIMARY to PRIMARY
        }
        statusDotView?.background = circle(dotColor)
        statusTextView?.setTextColor(textColor)
    }

    private fun buildImeView(): View {
        val host = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = roundedTopBg(SURFACE, dp(16f))
            setPadding(dp(16f).toInt(), dp(12f).toInt(), dp(16f).toInt(), dp(18f).toInt())
            minimumHeight = dp(280f).toInt()
        }

        val topRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        statusDotView = View(this).apply {
            background = circle(Color.parseColor("#A5A29A"))
            layoutParams = LinearLayout.LayoutParams(dp(6f).toInt(), dp(6f).toInt()).apply {
                rightMargin = dp(7f).toInt()
            }
        }
        statusTextView = TextView(this).apply {
            text = "Voxa ready"
            setTextColor(Color.parseColor("#7A7A82"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        }
        val flex = View(this).apply { layoutParams = LinearLayout.LayoutParams(0, 0, 1f) }
        timerView = TextView(this).apply {
            text = "0:00"
            setTextColor(INK)
            typeface = android.graphics.Typeface.MONOSPACE
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
        }
        topRow.addView(statusDotView)
        topRow.addView(statusTextView)
        topRow.addView(flex)
        topRow.addView(timerView)
        host.addView(topRow, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            LinearLayout.LayoutParams.WRAP_CONTENT,
        ))

        val body = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL or Gravity.CENTER_VERTICAL
        }

        // Wave stage (visible during recording + paused)
        waveStage = FrameLayout(this).apply {
            background = roundedRect(Color.parseColor("#F5F5F5"), dp(10f))
            visibility = View.GONE
        }
        waveView = WaveformView(this)
        waveStage!!.addView(
            waveView,
            FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, dp(72f).toInt())
        )
        body.addView(waveStage, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            bottomMargin = dp(14f).toInt()
        })

        // Cancel | Mic (Pause/Resume) | Send
        val ctlRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_HORIZONTAL or Gravity.CENTER_VERTICAL
        }
        cancelBtn = TextView(this).apply {
            text = "Cancel"
            setTextColor(RECORDING)
            background = roundedRect(SURFACE, dp(99f), strokeColor = Color.parseColor("#F5D9DA"))
            setPadding(dp(16f).toInt(), dp(9f).toInt(), dp(16f).toInt(), dp(9f).toInt())
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            visibility = View.GONE
            setOnClickListener { handleCancel() }
        }
        micButton = FrameLayout(this).apply {
            background = circle(INK)
            isClickable = true
            setOnClickListener { handleMicTap(it) }
            setOnLongClickListener { handleCancel(); true }
        }
        micCenter = View(this).apply {
            background = micIconDrawable()
            layoutParams = FrameLayout.LayoutParams(dp(22f).toInt(), dp(26f).toInt()).apply {
                gravity = Gravity.CENTER
            }
        }
        micButton!!.addView(micCenter)
        sendBtn = TextView(this).apply {
            text = "Send"
            setTextColor(Color.WHITE)
            background = roundedRect(PRIMARY, dp(99f))
            setPadding(dp(16f).toInt(), dp(9f).toInt(), dp(16f).toInt(), dp(9f).toInt())
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            visibility = View.GONE
            setOnClickListener { handleSend() }
        }
        ctlRow.addView(cancelBtn, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            rightMargin = dp(16f).toInt()
        })
        ctlRow.addView(micButton, LinearLayout.LayoutParams(dp(60f).toInt(), dp(60f).toInt()))
        ctlRow.addView(sendBtn, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            leftMargin = dp(16f).toInt()
        })
        body.addView(ctlRow, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT))

        hintView = TextView(this).apply {
            text = "Tap to dictate. Tap Done to switch back to your keyboard."
            setTextColor(Color.parseColor("#A5A29A"))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            gravity = Gravity.CENTER
        }
        body.addView(hintView, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            topMargin = dp(14f).toInt()
            leftMargin = dp(20f).toInt(); rightMargin = dp(20f).toInt()
        })

        doneBtn = TextView(this).apply {
            text = "Done"
            setTextColor(PRIMARY)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setPadding(dp(18f).toInt(), dp(7f).toInt(), dp(18f).toInt(), dp(7f).toInt())
            gravity = Gravity.CENTER
            setOnClickListener { handleDone() }
        }
        body.addView(doneBtn, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            topMargin = dp(8f).toInt()
        })

        host.addView(body, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))
        return host
    }

    // ─── drawables ────────────────────────────────────────────────────────────

    private fun circle(color: Int) = GradientDrawable().apply {
        shape = GradientDrawable.OVAL
        setColor(color)
    }

    private fun roundedRect(color: Int, radius: Float, strokeColor: Int? = null) = GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        cornerRadius = radius
        setColor(color)
        if (strokeColor != null) setStroke(dp(1f).toInt(), strokeColor)
    }

    private fun roundedTopBg(color: Int, radius: Float) = GradientDrawable().apply {
        shape = GradientDrawable.RECTANGLE
        cornerRadii = floatArrayOf(radius, radius, radius, radius, 0f, 0f, 0f, 0f)
        setColor(color)
    }

    /** Proper mic icon: capsule + arc + stand + base, all in white. */
    private fun micIconDrawable(): android.graphics.drawable.Drawable {
        return object : android.graphics.drawable.Drawable() {
            private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = Color.WHITE; style = Paint.Style.FILL
            }
            private val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = Color.WHITE; style = Paint.Style.STROKE
                strokeWidth = dp(2f); strokeCap = Paint.Cap.ROUND
            }
            override fun draw(canvas: Canvas) {
                val b = bounds
                val cx = b.centerX().toFloat()
                // Capsule (mic body): upper 55% of the icon, ~45% width.
                val capW = b.width() * 0.42f
                val capH = b.height() * 0.50f
                val capTop = b.top + b.height() * 0.06f
                val capLeft = cx - capW / 2f
                val capR = capW / 2f
                canvas.drawRoundRect(capLeft, capTop, capLeft + capW, capTop + capH, capR, capR, fill)
                // Arc (mic guard) just below the capsule center.
                val arcR = b.width() * 0.36f
                val arcCy = capTop + capH * 0.66f
                canvas.drawArc(cx - arcR, arcCy - arcR, cx + arcR, arcCy + arcR, 0f, 180f, false, stroke)
                // Stand
                val standTop = arcCy + arcR
                val standBottom = b.bottom - b.height() * 0.04f
                canvas.drawLine(cx, standTop, cx, standBottom, stroke)
                // Base
                val baseW = b.width() * 0.46f
                canvas.drawLine(cx - baseW / 2f, standBottom, cx + baseW / 2f, standBottom, stroke)
            }
            override fun setAlpha(alpha: Int) { fill.alpha = alpha; stroke.alpha = alpha }
            override fun setColorFilter(cf: android.graphics.ColorFilter?) { fill.colorFilter = cf; stroke.colorFilter = cf }
            override fun getOpacity(): Int = android.graphics.PixelFormat.TRANSLUCENT
        }
    }

    private fun pauseBars(): android.graphics.drawable.Drawable {
        // Two vertical white bars side by side — the classic pause icon.
        return object : android.graphics.drawable.Drawable() {
            private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; style = Paint.Style.FILL }
            override fun draw(canvas: Canvas) {
                val b = bounds
                val barWidth = b.width() / 3.6f
                val gap = b.width() / 5f
                val left1 = b.left.toFloat() + (b.width() - 2 * barWidth - gap) / 2f
                val left2 = left1 + barWidth + gap
                val radius = barWidth / 2f
                canvas.drawRoundRect(left1, b.top.toFloat(), left1 + barWidth, b.bottom.toFloat(), radius, radius, paint)
                canvas.drawRoundRect(left2, b.top.toFloat(), left2 + barWidth, b.bottom.toFloat(), radius, radius, paint)
            }
            override fun setAlpha(alpha: Int) { paint.alpha = alpha }
            override fun setColorFilter(cf: android.graphics.ColorFilter?) { paint.colorFilter = cf }
            override fun getOpacity(): Int = android.graphics.PixelFormat.TRANSLUCENT
        }
    }

    private fun playTriangle(): android.graphics.drawable.Drawable {
        return object : android.graphics.drawable.Drawable() {
            private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE; style = Paint.Style.FILL }
            private val path = android.graphics.Path()
            override fun draw(canvas: Canvas) {
                val b = bounds
                val w = b.width().toFloat()
                val h = b.height().toFloat()
                path.reset()
                // Triangle pointing right, slightly inset
                val inset = w * 0.12f
                path.moveTo(b.left + inset, b.top.toFloat())
                path.lineTo(b.left + inset, b.bottom.toFloat())
                path.lineTo(b.right - inset, b.top + h / 2f)
                path.close()
                canvas.drawPath(path, paint)
            }
            override fun setAlpha(alpha: Int) { paint.alpha = alpha }
            override fun setColorFilter(cf: android.graphics.ColorFilter?) { paint.colorFilter = cf }
            override fun getOpacity(): Int = android.graphics.PixelFormat.TRANSLUCENT
        }
    }

    private fun dp(v: Float): Float = v * resources.displayMetrics.density

    companion object {
        private val INK       = Color.parseColor("#16161A")
        private val SURFACE   = Color.parseColor("#FFFFFF")
        private val PRIMARY   = Color.parseColor("#1F4FE0")
        private val RECORDING = Color.parseColor("#E5484D")
    }
}

private class WaveformView(ctx: android.content.Context) : View(ctx) {
    private val barCount = 9
    private val buffer = FloatArray(barCount) { 0.05f }
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.parseColor("#16161A")
        style = Paint.Style.FILL
    }
    private val rect = RectF()
    private val density = resources.displayMetrics.density

    fun clear() {
        for (i in 0 until barCount) buffer[i] = 0.05f
        postInvalidate()
    }

    fun pushAmplitude(rawAmp: Int) {
        val n = (rawAmp.coerceIn(0, 32767)) / 32767f
        val logged = if (n > 0f) (kotlin.math.ln(1f + 19f * n) / kotlin.math.ln(20f)) else 0f
        for (i in 0 until barCount - 1) buffer[i] = buffer[i + 1]
        buffer[barCount - 1] = logged.coerceIn(0.05f, 1f)
        postInvalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        val centerY = h / 2f
        val barWidth = 5f * density
        val spacing = (w - barCount * barWidth) / (barCount + 1)
        var x = spacing
        for (i in 0 until barCount) {
            val amp = buffer[i]
            val pos = i / (barCount - 1f)
            val edgeBoost = 1f - 0.6f * (1f - 4f * pos * (1f - pos))
            val barHeight = max(2f * density, amp * (h * 0.85f) * edgeBoost)
            rect.set(x, centerY - barHeight / 2f, x + barWidth, centerY + barHeight / 2f)
            canvas.drawRoundRect(rect, barWidth / 2f, barWidth / 2f, paint)
            x += barWidth + spacing
        }
    }
}
