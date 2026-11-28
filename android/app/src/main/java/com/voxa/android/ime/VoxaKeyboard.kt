package com.voxa.android.ime

import android.annotation.SuppressLint
import android.content.Intent
import android.graphics.Bitmap
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.MotionEvent
import android.media.AudioManager
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.voxa.android.MainActivity
import com.voxa.android.diag.Diag
import com.voxa.android.notify.Notifier
import com.voxa.android.voxa
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import com.voxa.android.notify.VoxaJobs
import uniffi.voxa_ffi.DeliveryResult
import uniffi.voxa_ffi.EntryStatus
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
import uniffi.voxa_ffi.pauseKeyLabel

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
    /** What the panel's colours were built from ([KeyboardLook.stamp]); a new stamp on show rebuilds it. */
    private var builtStamp: String? = null
    /** The navigation bar under the keyboard, px (the panel's bottom padding). */
    private var navInset = 0

    /** The decoded photo of the Keyboard look, kept here (not in the panel) so a rebuilt panel
     *  doesn't decode it again; keyed by file, modification time, size, position and soften. */
    private var photo: Bitmap? = null
    private var photoKey: String? = null
    private var photoJob: Job? = null

    /** The field in front now, or null when no field is bound. */
    private var target: FfiTarget? = null
    /** Goes up every time a field gets focus (not when the same field restarts its input). Part
     *  of the target's identity: every chat in a messaging app, every field in a web page and
     *  every Compose field share one view id, so a field is only "the same" within one focus. */
    private var inputSession = 0L
    private var field = FieldKind.NORMAL
    private var editorInfo: EditorInfo? = null
    /** A finished text that arrived while no field was bound; the next field decides. */
    private var pendingPaste: FlowEffect.Paste? = null
    /** When the mic last went live in this take (start or resume). */
    private var recordingSince = 0L
    /** Talking time before the current stretch, in ms (paused time is left out). */
    private var talkedMs = 0L
    private var haptics = true
    /** Settings, "Hold to talk": a press held past [HOLD_MS] stops on release. Off: taps only. */
    private var holdToTalk = true

    // Mic key: press-and-hold over 400 ms then release stops (hold to talk); a quick tap stays on.
    private var pressStarted = false
    /** A finger is on the mic key now. */
    private var pressActive = false
    /** A rebuild of the keyboard view waits until no take is live and no key is held: a rebuild
     *  removes the mic view, and the release of a hold-to-talk press would never arrive. */
    private var rebuildPending = false

    /** Set first thing in onDestroy: nothing may use the Rust flow after that. */
    private var destroyed = false

    /** A short message shown in the bar while idle, e.g. "You're on a call" (cleared after a few s). */
    private var notice: Pair<String, String>? = null
    private val clearNotice = Runnable { notice = null; render() }

    /** Shown instead of the paused detail when Resume was refused (on a call); the bar's usual
     *  notices only show while idle. Cleared when the take leaves the paused state. */
    private var pausedNotice: String? = null

    private val timerTick = object : Runnable {
        override fun run() {
            val p = panel ?: return
            val secs = talkedSecs()
            p.right.text = "%d:%02d".format(secs / 60, secs % 60)
            main.postDelayed(this, 500)
        }
    }

    private fun isPaused(): Boolean = (flow.state() as? FlowState.Recording)?.paused == true

    /** Talking time of the take on screen, frozen while paused. */
    private fun talkedSecs(): Long =
        (talkedMs + if (isPaused()) 0L else SystemClock.elapsedRealtime() - recordingSince) / 1000

    override fun onCreate() {
        super.onCreate()
        Diag.log("ime", "onCreate")
        flow = VoxaFlow(filesDir.path, app.readiness())
        app.keyboardFlow = flow
        recorder = Recorder(
            this,
            onLevel = { if (!destroyed && !isPaused()) panel?.bars?.level(if (debugLevels) syntheticLevel() else it) },
            onInterrupted = { onMicInterrupted() },
            onCantSave = { onCantSave() },
        )
        app.historyChanged()
        scope.launch {
            combine(app.session, app.online) { _, _ -> }.collect { pushReadiness() }
        }
    }

    /** Debug builds only (see [VoxaApp.debugLevels]): a speech-like level, read once per take. */
    private var debugLevels = false
    private var levelTick = 0
    private fun syntheticLevel(): Float {
        levelTick++
        val syllable = kotlin.math.sin(levelTick * 0.55).toFloat().coerceAtLeast(0f)
        return (0.25f + 0.7f * syllable * (0.6f + 0.4f * kotlin.math.sin(levelTick * 0.13).toFloat())).coerceIn(0f, 1f)
    }

    override fun onDestroy() {
        Diag.log("ime", "onDestroy")
        destroyed = true
        if (recorder.isRecording) recorder.stop()
        scope.cancel()
        releasePhoto()
        main.removeCallbacksAndMessages(null)
        // super.onDestroy() calls onFinishInputView (which checks the flow): destroy the flow after it.
        super.onDestroy()
        app.keyboardFlow = null
        Notifier.keyboardShowing = false
        flow.destroy()
    }

    override fun onEvaluateFullscreenMode() = false

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        // The photo is decoded again (off the main thread) the next time the keyboard shows.
        if (level >= TRIM_MEMORY_RUNNING_LOW && !isInputViewShown && photo != null) {
            Diag.log("look", "photo released (trim level $level)")
            releasePhoto()
        }
    }

    override fun onCreateInputView(): View {
        Diag.log("ime", if (panel == null) "panel created" else "panel recreated")
        val colors = KbColors.of(this)
        builtStamp = KeyboardLook.stamp(this)
        val p = KeyboardPanel(this, colors)
        panel = p
        if (colors.photo) attachPhoto(p) else releasePhoto()
        // A rebuilt panel gets no new insets until they change: start from the last ones.
        p.bottomInset(navInset)
        ViewCompat.setOnApplyWindowInsetsListener(p) { v, insets ->
            val nav = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            navInset = nav.bottom
            (v as KeyboardPanel).bottomInset(nav.bottom)
            insets
        }
        ViewCompat.requestApplyInsets(p)
        wire(p)
        render()
        return p
    }

    // ── the field in front ───────────────────────────────────────────────────

    override fun onStartInput(info: EditorInfo, restarting: Boolean) {
        super.onStartInput(info, restarting)
        editorInfo = info
        field = fieldKind(info.inputType, info.imeOptions)
        if (!restarting) inputSession++
        val t = targetOf(info)
        Diag.log("ime", "onStartInput ${info.packageName} field ${info.fieldId} inputType 0x${Integer.toHexString(info.inputType)} imeOptions 0x${Integer.toHexString(info.imeOptions)} -> $field, restarting $restarting")
        target = t
        dispatch(FlowEvent.Focus(t))
        // A text finished while no field was bound: this is a new field, so the core asks
        // "Insert in X?" instead of inserting into a field nobody confirmed.
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
        // A Keyboard look or Appearance change (or new phone colours) since the view was built:
        // build it again, so the change shows now and not only after the keyboard restarts.
        val p = panel
        if (p != null && KeyboardLook.stamp(this) != builtStamp) {
            rebuildWhenQuiet("look changed")
        } else if (p != null && p.colors.photo && !p.hasPhoto && photoJob?.isActive != true) {
            attachPhoto(p)
        }
        Diag.memory("keyboard shown")
        Notifier.keyboardShowing = true
        val s = app.core.settings()
        haptics = s.haptics
        holdToTalk = s.holdToTalk
        debugLevels = app.debugLevels
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
        Notifier.keyboardShowing = false
        stopIfRecording("window hidden")
        super.onWindowHidden()
    }

    /** Rebuild the keyboard view now, or once the take on screen is over and no key is held. */
    private fun rebuildWhenQuiet(why: String) {
        if (busyForRebuild()) {
            if (!rebuildPending) Diag.log("look", "$why: keyboard view rebuilt after this take")
            rebuildPending = true
            return
        }
        rebuildPending = false
        Diag.log("look", "$why: keyboard view rebuilt")
        setInputView(onCreateInputView())
    }

    private fun busyForRebuild(): Boolean = pressActive || when (flow.state()) {
        is FlowState.Recording, is FlowState.Transcribing, is FlowState.Delivering -> true
        else -> false
    }

    private fun stopIfRecording(why: String) {
        if (destroyed) return
        if (flow.state() is FlowState.Recording) {
            Log.i(TAG, "stop: $why")
            Diag.log("ime", "stop: $why")
            dispatch(FlowEvent.Stop)
        }
    }

    /** App label plus an identity for the field: package, view id and this focus of it. A view
     *  id alone can't tell one chat from another (one compose box layout for every chat; no id at
     *  all in WebView and Compose), so a text never goes into a field the user moved to and came
     *  back to without confirming: the core asks "Insert in X?" instead. */
    private fun targetOf(info: EditorInfo): FfiTarget {
        val pkg = info.packageName ?: ""
        val label = runCatching {
            packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
        }.getOrDefault(pkg)
        val id = if (info.fieldId == View.NO_ID) 0 else info.fieldId
        return FfiTarget(label, KeyRules.fieldIdentity(pkg, id, inputSession).toULong())
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
        if (rebuildPending && !busyForRebuild()) main.post { if (!destroyed && rebuildPending && !busyForRebuild()) rebuildWhenQuiet("deferred") }
    }

    private fun run(effect: FlowEffect) {
        when (effect) {
            is FlowEffect.ShowStatus -> Unit
            is FlowEffect.HideStatus -> if (effect.afterMs > 0u) {
                val shown = flow.state()
                // "Pasted in X" gets out of the way after a moment and a half.
                val after = if (shown is FlowState.Delivered) minOf(effect.afterMs.toLong(), DONE_MS) else effect.afterMs.toLong()
                main.postDelayed({ if (!destroyed && flow.state() == shown) dispatch(FlowEvent.Dismiss) }, after)
            }
            is FlowEffect.StartCapture -> startCapture(effect)
            is FlowEffect.StopCapture -> {
                val secs = recorder.stop()
                Diag.log("mic", "stopped: %.1f s, %d bytes, peak level %.2f".format(secs, recorder.bytesWritten, recorder.peakLevel))
                Diag.memory("recording stopped")
                main.removeCallbacks(timerTick)
                flow.setRecordingSecs(effect.job, secs)
            }
            is FlowEffect.PauseCapture -> {
                talkedMs += SystemClock.elapsedRealtime() - recordingSince
                recorder.pause()
                main.removeCallbacks(timerTick)
                panel?.bars?.reset()
                Diag.log("mic", "paused: %.1f s talked, %d bytes".format(recorder.talkingSecs(), recorder.bytesWritten))
            }
            is FlowEffect.ResumeCapture -> {
                val err = recorder.resume()
                if (err == null) {
                    recordingSince = SystemClock.elapsedRealtime()
                    main.removeCallbacks(timerTick)
                    main.post(timerTick)
                    Diag.log("mic", "resumed")
                } else {
                    // Like an interruption: finish and send what was recorded. The clock restarts
                    // now so the timer doesn't count the pause for the moment before the stop.
                    recordingSince = SystemClock.elapsedRealtime()
                    Diag.log("mic", "resume failed: $err; sending what was recorded")
                    main.post { if (!destroyed && flow.state() is FlowState.Recording) dispatch(FlowEvent.Stop) }
                }
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
            is FlowEffect.OpenStorage -> openApp(MainActivity.OPEN_STORAGE)
        }
    }

    private fun startCapture(effect: FlowEffect.StartCapture) {
        val err = recorder.start(effect.pcmPath)
        if (err == null) {
            Diag.log("mic", "started")
            Diag.memory("recording started")
            recordingSince = SystemClock.elapsedRealtime()
            talkedMs = 0L
            main.removeCallbacks(timerTick)
            main.post(timerTick)
            panel?.bars?.reset()
            return
        }
        // The mic would not open: undo the take and show why instead (as the desktop does).
        Log.w(TAG, "mic failed: $err")
        Diag.log("mic", "failed to start: $err")
        val t = target
        main.post {
            dispatch(FlowEvent.Cancel)
            val mic = when (err) {
                Recorder.StartError.Missing -> MicState.MISSING
                Recorder.StartError.CantSave -> MicState.CANT_SAVE
                else -> MicState.DENIED
            }
            dispatch(FlowEvent.Readiness(flow.readiness().copy(mic = mic)))
            if (t != null) dispatch(FlowEvent.MicPressed(t, field == FieldKind.PRIVATE))
        }
    }

    private fun transcribe(effect: FlowEffect.Transcribe) {
        val job = effect.job
        val flow = flow
        // On the app's scope, not the keyboard's: switching keyboards mid-upload must not throw
        // the paid-for result away. The core also writes it into History itself.
        app.scope.launch {
            // A cold start reads the saved sign-in off the main thread: wait for it.
            app.awaitSession()
            val event = withContext(Dispatchers.IO) {
                try {
                    // Debug builds only: canned text; the rest of the flow (paste, History, the success state) runs for real.
                    val canned = app.debugTranscript
                    if (canned != null) Diag.log("debug", "canned transcript used instead of ChatGPT")
                    val text = app.core.transcribeTake(effect.entry, effect.pcmPath, effect.app, canned)
                    // Empty only when the device found no speech.
                    if (text.isBlank()) FlowEvent.NoSpeech(job) else FlowEvent.Transcribed(job, text)
                } catch (e: TranscribeFailure) {
                    FlowEvent.TranscribeFailed(job, e)
                } catch (e: CancellationException) {
                    throw e
                } catch (t: Throwable) {
                    // Every upload the flow asked for gets an answer, or the take waits forever.
                    Diag.log("transcribe", "failed: ${t.javaClass.simpleName}")
                    FlowEvent.TranscribeFailed(job, TranscribeFailure.Server(0u))
                }
            }
            if (event is FlowEvent.TranscribeFailed) {
                Log.i(TAG, "transcribe failed: ${event.error.javaClass.simpleName}")
                if (event.error is TranscribeFailure.SessionExpired || event.error is TranscribeFailure.SignedOut) {
                    app.session.value = SessionState.EXPIRED
                }
                if (event.error is TranscribeFailure.Blocked) app.blocked.value = true
            }
            if (destroyed || this@VoxaKeyboard.flow !== flow) {
                // The keyboard went away meanwhile: the core kept the result in History; say so.
                if (event is FlowEvent.Transcribed && effect.entry != 0uL) Notifier.ready(app, listOf(effect.entry), effect.app)
                return@launch
            }
            // Not the dictation on screen (sent when the network came back): it finishes unseen.
            val current = (flow.state() as? FlowState.Transcribing)?.job == job
            dispatch(event)
            if (!current && !destroyed) notifyBackground(job, event)
        }
    }

    /** A2 / A3 for a dictation that finished off screen, by what History now says (a take the user
     *  cancelled meanwhile has no entry, so nothing is posted). Never for incognito takes. A3 comes
     *  only after the core's three tries, never for Cloudflare blocking, and its Retry sends this
     *  entry again (it is added to the Retry list). */
    private fun notifyBackground(job: ULong, event: FlowEvent) {
        val entry = flow.entryForJob(job) ?: return
        val unreachable = event is FlowEvent.TranscribeFailed && (event.error is TranscribeFailure.Timeout || event.error is TranscribeFailure.Server)
        if (event !is FlowEvent.Transcribed && !unreachable) return
        scope.launch {
            val e = withContext(Dispatchers.IO) { app.core.entry(entry) } ?: return@launch
            when {
                event is FlowEvent.Transcribed && e.status is EntryStatus.Ready -> Notifier.ready(this@VoxaKeyboard, listOf(entry), e.app)
                unreachable && e.status is EntryStatus.Failed -> {
                    VoxaJobs.addFailed(this@VoxaKeyboard, listOf(entry))
                    Notifier.notSent(this@VoxaKeyboard, 1)
                }
            }
        }
    }

    private fun paste(effect: FlowEffect.Paste) {
        val ic = currentInputConnection
        val now = target
        if (ic == null || now == null) {
            // No field bound (keyboard closed): nothing is inserted; the next field decides.
            Diag.log("insert", "no field bound; waiting for the next field")
            pendingPaste = effect
            // The keyboard went away (a paused take stopped when the user switched apps): if no
            // field comes back soon, say it's ready in History.
            main.postDelayed({
                if (!destroyed && pendingPaste === effect && !Notifier.keyboardShowing) {
                    // No field came back: it waits in History (and the notification can copy it).
                    // It is never inserted later into whatever field comes next.
                    pendingPaste = null
                    val entry = flow.entryForJob(effect.job) ?: return@postDelayed
                    scope.launch {
                        val e = withContext(Dispatchers.IO) { app.core.entry(entry) } ?: return@launch
                        if (e.status is EntryStatus.Ready) Notifier.ready(this@VoxaKeyboard, listOf(entry), e.app)
                    }
                }
            }, PASTE_GRACE_MS)
            return
        }
        val result = if (now == effect.target) {
            if (insert(ic, effect.text)) DeliveryResult.Inserted else DeliveryResult.Failed
        } else {
            DeliveryResult.TargetChanged(now)
        }
        Diag.log("insert", "paste: ${result.javaClass.simpleName} (${effect.text.length} chars)")
        dispatch(FlowEvent.Delivered(effect.job, result))
        // Inserted after all: a "ready" notice for it would offer a second paste.
        if (result is DeliveryResult.Inserted) Notifier.clearDictations(this)
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

    // ── the Keyboard look's photo ────────────────────────────────────────────

    /** Decodes the photo off the main thread (once per file and size) and hands it to [p]. Any
     *  failure, out of memory included, falls back to the Voxa look instead of crashing. */
    private fun attachPhoto(p: KeyboardPanel) {
        val look = p.colors.look
        val m = resources.displayMetrics
        val w = m.widthPixels
        val h = (Photo.PANEL_DP * m.density).toInt()
        val file = KeyboardLook.photoFile(this)
        val key = "${file.path}:${file.lastModified()}:$w:$h:${look.position}:${look.soften}"
        val have = photo
        if (key == photoKey && have != null && !have.isRecycled) {
            p.setPhoto(have, cropped = true)
            return
        }
        photoJob?.cancel()
        photoJob = scope.launch {
            val bmp = withContext(Dispatchers.IO) {
                try {
                    KeyboardLook.decodePhoto(this@VoxaKeyboard, w, h, look.position, look.soften, crop = true)
                } catch (e: CancellationException) {
                    throw e
                } catch (t: Throwable) {
                    Diag.log("look", "photo could not be decoded: ${t.javaClass.simpleName}")
                    null
                }
            }
            if (destroyed) return@launch
            if (bmp == null) {
                KeyboardLook.fallback(this@VoxaKeyboard, "photo decode")
                if (panel === p) rebuildWhenQuiet("photo could not be decoded")
                return@launch
            }
            val old = photo
            photo = bmp
            photoKey = key
            if (old != null && old !== bmp) old.recycle()
            panel?.takeIf { it.colors.photo }?.setPhoto(bmp, cropped = true)
            Diag.memory("photo decoded ${bmp.width} x ${bmp.height} ${bmp.config}")
        }
    }

    private fun releasePhoto() {
        photoJob?.cancel()
        photoJob = null
        panel?.setPhoto(null)
        photo?.recycle()
        photo = null
        photoKey = null
    }

    // ── keys ─────────────────────────────────────────────────────────────────

    @SuppressLint("ClickableViewAccessibility")
    private fun wire(p: KeyboardPanel) {
        p.mic.setOnTouchListener { v, ev ->
            when (ev.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    // Only the circle itself, not its glow.
                    if (!p.mic.hit(ev.x, ev.y) || !v.isEnabled) return@setOnTouchListener false
                    v.isPressed = true
                    pressActive = true
                    tap(v)
                    // Only a press that starts the recording can be a hold-to-talk press; a tap
                    // while recording always stops it (on release).
                    val wasBusy = when (flow.state()) {
                        is FlowState.Recording, is FlowState.Transcribing, is FlowState.Delivering -> true
                        else -> false
                    }
                    if (!wasBusy) onMicPressed()
                    pressStarted = !wasBusy && flow.state() is FlowState.Recording
                    Diag.log("ime", "mic down: ${if (wasBusy) "while busy" else "start"}")
                }
                MotionEvent.ACTION_UP -> {
                    v.isPressed = false
                    pressActive = false
                    // From the touch itself, not from when it is handled: starting the mic can
                    // block the main thread for a few hundred ms, which must not turn a tap into a hold.
                    val held = ev.eventTime - ev.downTime
                    val recording = flow.state() is FlowState.Recording
                    val stop = KeyRules.stopOnRelease(recording, pressStarted, holdToTalk, held)
                    Diag.log("ime", "mic up after $held ms: ${if (stop) "stop" else if (recording) "keep listening (tap)" else "not recording"}")
                    if (stop) dispatch(FlowEvent.Stop)
                    v.performClick()
                }
                MotionEvent.ACTION_CANCEL -> {
                    // The press was taken away (the view went, or a parent took the touch): a
                    // hold-to-talk press ends as if released, so the mic never keeps recording.
                    v.isPressed = false
                    pressActive = false
                    val held = ev.eventTime - ev.downTime
                    if (KeyRules.stopOnCancel(flow.state() is FlowState.Recording, pressStarted, holdToTalk, held)) {
                        Diag.log("ime", "mic press cancelled after $held ms: stop")
                        dispatch(FlowEvent.Stop)
                    } else if (rebuildPending && !busyForRebuild()) {
                        main.post { if (!destroyed && rebuildPending && !busyForRebuild()) rebuildWhenQuiet("deferred") }
                    }
                }
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
        // During a take the wide key pauses and resumes (the big mic is Stop); otherwise it is space.
        p.spaceKey.setOnClickListener {
            tap(it)
            when {
                isPaused() -> onResumePressed()
                flow.state() is FlowState.Recording -> dispatch(FlowEvent.Pause)
                else -> currentInputConnection?.commitText(" ", 1)
            }
        }
        p.enterKey.setOnClickListener { tap(it); enter() }
        repeating(p.backspaceKey) { backspace() }
    }

    private fun onMicPressed() {
        val t = target ?: return
        if (field == FieldKind.PASSWORD) return
        // During a call the call owns the microphone: other apps only get silence. Say so instead
        // of pretending to listen.
        if (inCall()) {
            Diag.log("ime", "mic refused: in a call (audio mode ${audioMode()})")
            showNotice("You're on a call", "Voxa can't use the microphone during a call. Try again when it ends.")
            return
        }
        notice = null
        // The flow may think it is offline after one failed upload while the phone is online:
        // tell it what is true now, so this take is sent at once (and waiting ones go too).
        pushReadiness()
        dispatch(FlowEvent.MicPressed(t, field == FieldKind.PRIVATE))
    }

    /** Resume, unless a call owns the microphone now: then stay paused and say so. */
    private fun onResumePressed() {
        if (inCall()) {
            Diag.log("ime", "resume refused: in a call (audio mode ${audioMode()})")
            pausedNotice = "You're on a call. Resume when it ends"
            render()
            return
        }
        pausedNotice = null
        dispatch(FlowEvent.Resume)
    }

    private fun audioMode(): Int = getSystemService(AudioManager::class.java)?.mode ?: AudioManager.MODE_NORMAL

    /** Phone call, VoIP call (WhatsApp, Meet), call screening or a redirected call. */
    private fun inCall(): Boolean = KeyRules.inCall(audioMode())

    private fun showNotice(title: String, detail: String) {
        notice = title to detail
        main.removeCallbacks(clearNotice)
        main.postDelayed(clearNotice, 6000)
        render()
    }

    /** The system took the microphone mid-take (a call started, another app grabbed it). Keep
     *  what was said; if nothing was heard at all, drop the empty take and say why. */
    private fun onMicInterrupted() {
        // Paused, the mic is already released: nothing of ours was interrupted.
        if (destroyed || flow.state() !is FlowState.Recording || isPaused()) return
        val taken = KeyRules.micTaken(recorder.peakLevel, inCall())
        if (taken != KeyRules.MicTaken.StopAndSend) {
            Diag.log("ime", "mic taken with nothing recorded: cancelling (audio mode ${audioMode()})")
            dispatch(FlowEvent.Cancel)
            if (taken == KeyRules.MicTaken.CancelInCall) showNotice("You're on a call", "Voxa can't use the microphone during a call. Nothing was recorded.")
            else showNotice("Another app is using the microphone", "Nothing was recorded. Try again when it's free.")
        } else {
            stopIfRecording("interrupted")
        }
    }

    /** The recording couldn't be written (storage full). What was saved is sent; the next press
     *  says the storage is full and offers to free up space. */
    private fun onCantSave() {
        if (destroyed) return
        Diag.log("mic", "can't save the recording: storage full")
        if (flow.state() is FlowState.Recording) dispatch(FlowEvent.Stop)
        pushReadiness()
        if (flow.state() !is FlowState.Recording && flow.state() !is FlowState.Transcribing) {
            showNotice("Your phone's storage is full", "What was recorded is kept. Free up space to dictate again.")
        }
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
        val paused = (state as? FlowState.Recording)?.paused == true
        if (!paused) pausedNotice = null
        val busy = state is FlowState.Transcribing || state is FlowState.Delivering
        val done = if (state is FlowState.Delivered) flow.delivered() else null
        val password = field == FieldKind.PASSWORD && state is FlowState.Idle

        // The status line.
        val shown = when {
            password -> passwordFieldText()
            status != null -> status
            else -> null
        }
        val title: String
        var detail: String? = null
        var action: String? = null
        var closable = false
        val n = notice
        if (shown == null && n != null && state is FlowState.Idle) {
            title = n.first
            detail = n.second
        } else if (shown != null) {
            title = shown.title
            detail = pausedNotice.takeIf { paused } ?: shown.detail
            // While listening the action is the big mic itself (Stop); the bar offers Cancel.
            action = shown.action.takeIf { !recording }
            closable = shown.dismissable || recording || state is FlowState.Transcribing
        } else {
            // Idle: the desktop home's one-line readiness.
            val signedOut = app.session.value == SessionState.EXPIRED
            title = if (signedOut) "Sign in to ChatGPT to dictate" else "Ready to dictate"
            if (signedOut) action = "Sign in"
        }
        if (done != null) {
            detail = null
            closable = false
        }
        p.show(
            KeyboardPanel.State(
                title = title,
                detail = detail,
                action = action,
                closable = closable,
                recording = recording,
                transcribing = state is FlowState.Transcribing,
                busy = busy,
                done = done != null,
                right = when {
                    recording -> talkedSecs().let { "%d:%02d".format(it / 60, it % 60) }
                    done != null -> if (done.words == 1u) "1 word" else "${done.words} words"
                    else -> ""
                },
                saved = done?.let { savedLine(it.savedSecs.toInt()) } ?: "",
                micOff = busy || password,
                paused = recording && paused,
                wideKey = if (recording) pauseKeyLabel(paused) else null,
            ),
        )
    }

    /** "That's 6 seconds of typing saved"; nothing when speaking wasn't faster. */
    private fun savedLine(secs: Int): String = when {
        secs < 1 -> ""
        secs == 1 -> "That's 1 second of typing saved"
        secs < 60 -> "That's $secs seconds of typing saved"
        else -> "That's ${secs / 60} min ${secs % 60} s of typing saved"
    }

    companion object {
        const val TAG = "VoxaKeyboard"
        const val HOLD_MS = KeyRules.HOLD_MS
        /** How long "Pasted in X" stays before the keyboard is back to idle. */
        const val DONE_MS = 1500L
        /** How long a finished text waits for a field after the keyboard went away before the
         *  "ready" notification says it's in History. */
        const val PASTE_GRACE_MS = 3000L
    }
}
