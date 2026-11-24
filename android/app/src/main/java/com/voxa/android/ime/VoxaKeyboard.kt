package com.voxa.android.ime

import android.annotation.SuppressLint
import android.content.Intent
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.diag.Diag
import com.voxa.android.voxa
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.DeliveryResult
import uniffi.voxa_ffi.FfiTarget
import uniffi.voxa_ffi.FieldKind
import uniffi.voxa_ffi.FlowEffect
import uniffi.voxa_ffi.FlowEvent
import uniffi.voxa_ffi.FlowState
import uniffi.voxa_ffi.MicState
import uniffi.voxa_ffi.SessionState
import uniffi.voxa_ffi.TranscribeFailure
import uniffi.voxa_ffi.VoxaFlow
import uniffi.voxa_ffi.fieldKind
import uniffi.voxa_ffi.passwordFieldText

/**
 * The Voxa voice keyboard. A thin shell around the core's dictation flow (voxa-core via voxa-ffi):
 * it feeds events in (mic key, stop, cancel, the field in front) and carries out the effects
 * (record, transcribe, insert), the same way the desktop drives its strip.
 */
class VoxaKeyboard : InputMethodService() {
    private val app get() = applicationContext.voxa
    private val main = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private lateinit var flow: VoxaFlow
    private lateinit var recorder: Recorder
    private var panel: KeyboardPanel? = null

    /** The field in front now, or null when no field is bound. */
    private var target: FfiTarget? = null
    private var field = FieldKind.NORMAL
    private var editorInfo: EditorInfo? = null
    /** A finished text that arrived while no field was bound; the next field decides. */
    private var pendingPaste: FlowEffect.Paste? = null
    private var recordingSince = 0L
    private var haptics = true

    // Mic key: press-and-hold over 400 ms then release stops (hold to talk); a quick tap stays on.
    private var pressStarted = false

    private val timerTick = object : Runnable {
        override fun run() {
            val p = panel ?: return
            val secs = (SystemClock.elapsedRealtime() - recordingSince) / 1000
            p.timer.text = "%d:%02d".format(secs / 60, secs % 60)
            main.postDelayed(this, 500)
        }
    }

    override fun onCreate() {
        super.onCreate()
        Diag.log("ime", "onCreate")
        flow = VoxaFlow(filesDir.path, app.readiness())
        recorder = Recorder(this, onLevel = { panel?.ink?.level(it) }, onInterrupted = { stopIfRecording("interrupted") })
        app.historyChanged()
        scope.launch {
            combine(app.session, app.online) { _, _ -> }.collect { pushReadiness() }
        }
    }

    override fun onDestroy() {
        Diag.log("ime", "onDestroy")
        if (recorder.isRecording) recorder.stop()
        scope.cancel()
        main.removeCallbacksAndMessages(null)
        flow.destroy()
        super.onDestroy()
    }

    override fun onEvaluateFullscreenMode() = false

    override fun onCreateInputView(): View {
        Diag.log("ime", if (panel == null) "panel created" else "panel recreated")
        val p = KeyboardPanel(this)
        panel = p
        ViewCompat.setOnApplyWindowInsetsListener(p) { v, insets ->
            val nav = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            v.setPadding(0, 0, 0, nav.bottom)
            insets
        }
        wire(p)
        render()
        return p
    }

    // ── the field in front ───────────────────────────────────────────────────

    override fun onStartInput(info: EditorInfo, restarting: Boolean) {
        super.onStartInput(info, restarting)
        editorInfo = info
        field = fieldKind(info.inputType, info.imeOptions)
        val t = targetOf(info)
        Diag.log("ime", "onStartInput ${info.packageName} field ${info.fieldId} inputType 0x${Integer.toHexString(info.inputType)} imeOptions 0x${Integer.toHexString(info.imeOptions)} -> $field, restarting $restarting")
        target = t
        dispatch(FlowEvent.Focus(t))
        pendingPaste?.let {
            pendingPaste = null
            run(it)
        }
    }

    override fun onFinishInput() {
        super.onFinishInput()
        Diag.log("ime", "onFinishInput")
        target = null
        editorInfo = null
    }

    override fun onStartInputView(info: EditorInfo, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        Diag.log("ime", "shown (onStartInputView ${info.packageName}, restarting $restarting)")
        Diag.memory("keyboard shown")
        val s = app.core.settings()
        haptics = s.haptics
        pushReadiness()
        app.checkSession()
        render()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        // The keyboard going away while listening means "done", not "throw it away".
        Diag.log("ime", "onFinishInputView finishing $finishingInput")
        stopIfRecording("keyboard hidden")
        super.onFinishInputView(finishingInput)
    }

    override fun onWindowHidden() {
        Diag.log("ime", "onWindowHidden")
        Diag.memory("keyboard hidden")
        stopIfRecording("window hidden")
        super.onWindowHidden()
    }

    private fun stopIfRecording(why: String) {
        if (flow.state() is FlowState.Recording) {
            Log.i(TAG, "stop: $why")
            Diag.log("ime", "stop: $why")
            dispatch(FlowEvent.Stop)
        }
    }

    /** App label plus an identity for the field: package and view id. Fields without an id
     *  (WebView, Compose) count as one target per app. */
    private fun targetOf(info: EditorInfo): FfiTarget {
        val pkg = info.packageName ?: ""
        val label = runCatching {
            packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
        }.getOrDefault(pkg)
        val id = if (info.fieldId == View.NO_ID) 0 else info.fieldId
        val window = (pkg.hashCode().toLong() shl 32) or (id.toLong() and 0xffffffffL)
        return FfiTarget(label, window.toULong())
    }

    private fun pushReadiness() {
        val r = app.readiness()
        if (r != flow.readiness()) {
            Diag.log("readiness", "keyboard: session ${r.session}, network ${r.network}, mic ${r.mic}")
            dispatch(FlowEvent.Readiness(r))
        }
    }

    // ── events in, effects out ───────────────────────────────────────────────

    private fun dispatch(event: FlowEvent) {
        val effects = flow.handle(event)
        Log.d(TAG, "event ${event.javaClass.simpleName} -> ${flow.state().javaClass.simpleName}")
        for (e in effects) run(e)
        app.historyChanged()
        render()
    }

    private fun run(effect: FlowEffect) {
        when (effect) {
            is FlowEffect.ShowStatus -> Unit
            is FlowEffect.HideStatus -> if (effect.afterMs > 0u) {
                val shown = flow.state()
                main.postDelayed({ if (flow.state() == shown) dispatch(FlowEvent.Dismiss) }, effect.afterMs.toLong())
            }
            is FlowEffect.StartCapture -> startCapture(effect)
            is FlowEffect.StopCapture -> {
                val secs = recorder.stop()
                Diag.log("mic", "stopped: %.1f s, %d bytes, peak level %.2f".format(secs, recorder.bytesWritten, recorder.peakLevel))
                Diag.memory("recording stopped")
                main.removeCallbacks(timerTick)
                flow.setRecordingSecs(effect.job, secs)
            }
            is FlowEffect.DiscardCapture -> {
                recorder.discard()
                Diag.log("mic", "discarded")
                main.removeCallbacks(timerTick)
            }
            is FlowEffect.Transcribe -> transcribe(effect)
            is FlowEffect.Paste -> paste(effect)
            is FlowEffect.PasteHere -> {
                val ic = currentInputConnection
                val ok = ic != null && target != null && insert(ic, effect.text)
                Diag.log("insert", "here: ${if (ok) "ok" else "failed"} (${effect.text.length} chars)")
                dispatch(FlowEvent.Delivered(effect.job, if (ok) DeliveryResult.Inserted else DeliveryResult.Failed))
            }
            is FlowEffect.OpenSignIn -> openApp(MainActivity.OPEN_SIGN_IN)
            is FlowEffect.OpenMicSettings -> openApp(MainActivity.OPEN_MIC)
        }
    }

    private fun startCapture(effect: FlowEffect.StartCapture) {
        val err = recorder.start(effect.pcmPath)
        if (err == null) {
            Diag.log("mic", "started")
            Diag.memory("recording started")
            recordingSince = SystemClock.elapsedRealtime()
            main.removeCallbacks(timerTick)
            main.post(timerTick)
            panel?.ink?.start()
            return
        }
        // The mic would not open: undo the take and show why instead (as the desktop does).
        Log.w(TAG, "mic failed: $err")
        Diag.log("mic", "failed to start: $err")
        val t = target
        main.post {
            dispatch(FlowEvent.Cancel)
            val mic = if (err == Recorder.StartError.Missing) MicState.MISSING else MicState.DENIED
            dispatch(FlowEvent.Readiness(flow.readiness().copy(mic = mic)))
            if (t != null) dispatch(FlowEvent.MicPressed(t, field == FieldKind.PRIVATE))
        }
    }

    private fun transcribe(effect: FlowEffect.Transcribe) {
        val job = effect.job
        scope.launch {
            val event = withContext(Dispatchers.IO) {
                try {
                    FlowEvent.Transcribed(job, app.core.transcribeRecording(effect.pcmPath, effect.app))
                } catch (e: TranscribeFailure) {
                    FlowEvent.TranscribeFailed(job, e)
                }
            }
            if (event is FlowEvent.TranscribeFailed) {
                Log.i(TAG, "transcribe failed: ${event.error.javaClass.simpleName}")
                if (event.error is TranscribeFailure.SessionExpired || event.error is TranscribeFailure.SignedOut) {
                    app.session.value = SessionState.EXPIRED
                }
                if (event.error is TranscribeFailure.Blocked) app.blocked.value = true
            }
            dispatch(event)
        }
    }

    private fun paste(effect: FlowEffect.Paste) {
        val ic = currentInputConnection
        val now = target
        if (ic == null || now == null) {
            // No field bound (keyboard closed): nothing is inserted; the next field decides.
            Diag.log("insert", "no field bound; waiting for the next field")
            pendingPaste = effect
            return
        }
        val result = if (now == effect.target) {
            if (insert(ic, effect.text)) DeliveryResult.Inserted else DeliveryResult.Failed
        } else {
            DeliveryResult.TargetChanged(now)
        }
        Diag.log("insert", "paste: ${result.javaClass.simpleName} (${effect.text.length} chars)")
        dispatch(FlowEvent.Delivered(effect.job, result))
    }

    /** Insert at the cursor, with a space before it when it follows a word (dictating in chunks). */
    private fun insert(ic: InputConnection, text: String): Boolean {
        val before = ic.getTextBeforeCursor(1, 0)
        val first = text.firstOrNull()
        val needsSpace = !before.isNullOrEmpty() && !before.last().isWhitespace() && first != null && !first.isWhitespace() && first !in ".,!?;:"
        ic.beginBatchEdit()
        val ok = ic.commitText(if (needsSpace) " $text" else text, 1)
        ic.endBatchEdit()
        return ok
    }

    private fun openApp(open: String) {
        Diag.log("ime", "open app: $open")

        startActivity(Intent(this, MainActivity::class.java).putExtra(MainActivity.EXTRA_OPEN, open).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }

    // ── keys ─────────────────────────────────────────────────────────────────

    @SuppressLint("ClickableViewAccessibility")
    private fun wire(p: KeyboardPanel) {
        p.mic.setOnTouchListener { v, ev ->
            when (ev.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    v.isPressed = true
                    tap(v)
                    pressStarted = false
                    when (flow.state()) {
                        is FlowState.Recording, is FlowState.Transcribing, is FlowState.Delivering -> Unit
                        else -> onMicPressed()
                    }
                    pressStarted = flow.state() is FlowState.Recording
                }
                MotionEvent.ACTION_UP -> {
                    v.isPressed = false
                    // From the touch itself, not from when it is handled: starting the mic can
                    // block the main thread for a few hundred ms, which must not turn a tap into a hold.
                    val held = ev.eventTime - ev.downTime
                    val recording = flow.state() is FlowState.Recording
                    if (recording && (!pressStarted || held > HOLD_MS)) dispatch(FlowEvent.Stop)
                    v.performClick()
                }
                MotionEvent.ACTION_CANCEL -> v.isPressed = false
            }
            true
        }
        // Idle, the bar's only action is "Sign in"; otherwise it is the core's one action.
        p.action.setOnClickListener { tap(it); if (flow.state() is FlowState.Idle) openApp(MainActivity.OPEN_SIGN_IN) else dispatch(FlowEvent.Action) }
        p.close.setOnClickListener {
            tap(it)
            when (flow.state()) {
                is FlowState.Recording, is FlowState.Transcribing -> dispatch(FlowEvent.Cancel)
                is FlowState.Idle -> Unit
                else -> dispatch(FlowEvent.Dismiss)
            }
        }
        p.switchKey.setOnClickListener {
            tap(it)
            if (!switchToPreviousInputMethod()) getSystemService(InputMethodManager::class.java).showInputMethodPicker()
        }
        p.switchKey.setOnLongClickListener {
            getSystemService(InputMethodManager::class.java).showInputMethodPicker()
            true
        }
        p.spaceKey.setOnClickListener { tap(it); currentInputConnection?.commitText(" ", 1) }
        p.enterKey.setOnClickListener { tap(it); enter() }
        repeating(p.backspaceKey) { backspace() }
    }

    private fun onMicPressed() {
        val t = target ?: return
        if (field == FieldKind.PASSWORD) return
        dispatch(FlowEvent.MicPressed(t, field == FieldKind.PRIVATE))
    }

    private fun tap(v: View) {
        if (haptics) v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
    }

    private fun backspace() {
        val ic = currentInputConnection ?: return
        val selected = ic.getSelectedText(0)
        if (!selected.isNullOrEmpty()) ic.commitText("", 1) else ic.deleteSurroundingTextInCodePoints(1, 0)
    }

    private fun enter() {
        val ic = currentInputConnection ?: return
        val info = editorInfo
        val action = (info?.imeOptions ?: 0) and EditorInfo.IME_MASK_ACTION
        val noAction = info == null || (info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0 ||
            action == EditorInfo.IME_ACTION_NONE || action == EditorInfo.IME_ACTION_UNSPECIFIED
        if (noAction) {
            ic.sendKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_ENTER))
            ic.sendKeyEvent(KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_ENTER))
        } else {
            ic.performEditorAction(action)
        }
    }

    /** A key that repeats while held (backspace). */
    @SuppressLint("ClickableViewAccessibility")
    private fun repeating(v: View, act: () -> Unit) {
        val repeat = object : Runnable {
            override fun run() {
                act()
                main.postDelayed(this, 50)
            }
        }
        v.setOnTouchListener { view, ev ->
            when (ev.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    view.isPressed = true
                    tap(view)
                    act()
                    main.postDelayed(repeat, 400)
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    view.isPressed = false
                    main.removeCallbacks(repeat)
                    if (ev.actionMasked == MotionEvent.ACTION_UP) view.performClick()
                }
            }
            true
        }
    }

    // ── render: a pure function of the core's state ──────────────────────────

    private fun render() {
        val p = panel ?: return
        val state = flow.state()
        val status = flow.status()
        val recording = state is FlowState.Recording
        val busy = state is FlowState.Transcribing || state is FlowState.Delivering

        p.inkRow.visibility = if (recording) View.VISIBLE else View.INVISIBLE
        if (recording) p.ink.start() else p.ink.stop()
        p.spinner.visibility = if (busy) View.VISIBLE else View.GONE

        val password = field == FieldKind.PASSWORD && state is FlowState.Idle
        val shown = when {
            password -> passwordFieldText()
            status != null -> status
            else -> null
        }
        if (shown != null) {
            p.title.text = shown.title
            p.detail.text = shown.detail ?: ""
            p.detail.visibility = if (shown.detail.isNullOrEmpty()) View.GONE else View.VISIBLE
            p.action.text = shown.action ?: ""
            // While listening the action is the big mic itself (Stop); the bar offers Cancel.
            p.action.visibility = if (shown.action != null && !recording) View.VISIBLE else View.GONE
            p.close.visibility = if (shown.dismissable || recording || state is FlowState.Transcribing) View.VISIBLE else View.GONE
        } else {
            idleBar(p)
        }
        p.close.contentDescription = if (recording || state is FlowState.Transcribing) "Cancel" else "Close"

        p.micIcon.setImageResource(if (recording) R.drawable.ic_stop else R.drawable.ic_mic)
        p.mic.contentDescription = if (recording) "Stop" else "Talk"
        val micOff = busy || password
        p.mic.alpha = if (micOff) 0.4f else 1f
        p.mic.isEnabled = !micOff
        p.hint.text = when {
            password -> ""
            recording -> "Tap to stop"
            busy -> ""
            else -> "Tap to talk"
        }
    }

    /** Idle: the desktop home's one-line readiness ("Ready to dictate"). */
    private fun idleBar(p: KeyboardPanel) {
        val signedOut = app.session.value == SessionState.EXPIRED
        p.title.text = if (signedOut) "Sign in to ChatGPT to dictate" else "Ready to dictate"
        p.detail.visibility = View.GONE
        p.action.text = "Sign in"
        p.action.visibility = if (signedOut) View.VISIBLE else View.GONE
        p.close.visibility = View.GONE
    }

    companion object {
        const val TAG = "VoxaKeyboard"
        const val HOLD_MS = 400L
    }
}
