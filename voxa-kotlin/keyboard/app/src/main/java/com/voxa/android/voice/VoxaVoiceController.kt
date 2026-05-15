package com.voxa.android.voice

import android.content.Context
import android.view.inputmethod.InputConnection
import android.widget.Toast
import com.voxa.android.data.AudioRecorder
import com.voxa.android.network.TranscriptionApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * Bridges HeliBoard's `VOICE_INPUT` key to Voxa's Whisper transcription pipeline.
 *
 * HeliBoard ships a "voice" toolbar button that, upstream, calls
 * `RichInputMethodManager.switchToShortcutIme()` to hand off to whichever system
 * voice IME is configured (typically Google's voice typing). In the Voxa fork we
 * keep the same toolbar button but route it here: first tap starts our recorder,
 * second tap stops it, transcribes via ChatGPT, and commits the result at the caret.
 *
 * State machine: Idle -> Recording -> Transcribing -> Idle.
 *
 * Callers should also push UI state updates through [state] so the keyboard chrome
 * can show a recording indicator. Day-1 wiring just commits text and toasts errors;
 * the visual indicator graft comes later.
 */
object VoxaVoiceController {

    enum class State { Idle, Recording, Transcribing }

    @Volatile private var currentState: State = State.Idle
    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private var recorder: AudioRecorder? = null
    // Re-issues the "Listening…" toast every ~2 s while recording so the user
    // always sees the indicator. Cancelled when the state leaves Recording.
    private var feedbackJob: Job? = null

    @JvmStatic
    fun state(): State = currentState

    @JvmStatic
    fun isBusy(): Boolean = currentState != State.Idle

    /** Called from `LatinIME` when the user taps the toolbar mic. */
    @JvmStatic
    fun toggle(context: Context, inputConnection: InputConnection?) {
        when (currentState) {
            State.Idle -> start(context)
            State.Recording -> stopAndCommit(context, inputConnection)
            State.Transcribing -> Unit // ignore taps while we wait for the API
        }
    }

    private fun start(context: Context) {
        currentState = State.Recording
        val rec = AudioRecorder(context.applicationContext).also { recorder = it }
        feedbackJob?.cancel()
        feedbackJob = scope.launch {
            while (isActive && currentState == State.Recording) {
                toast(context, "Listening… tap mic again to stop")
                delay(2_500)
            }
        }
        scope.launch {
            try {
                rec.start()
            } catch (e: Exception) {
                feedbackJob?.cancel()
                currentState = State.Idle
                toast(context, "Recording failed: ${e.message?.take(60) ?: "unknown"}")
            }
        }
    }

    private fun stopAndCommit(context: Context, ic: InputConnection?) {
        val rec = recorder ?: return
        feedbackJob?.cancel()
        currentState = State.Transcribing
        toast(context, "Transcribing…")
        scope.launch {
            try {
                val path = rec.stop()
                val text = TranscriptionApi.transcribeAudio(path)
                if (text.isNotEmpty()) {
                    ic?.commitText(text, 1)
                } else {
                    toast(context, "No speech detected")
                }
            } catch (e: Exception) {
                toast(context, "Transcription failed: ${e.message?.take(60) ?: "unknown"}")
            } finally {
                currentState = State.Idle
                recorder = null
            }
        }
    }

    private fun toast(context: Context, msg: String) {
        Toast.makeText(context.applicationContext, msg, Toast.LENGTH_SHORT).show()
    }
}
