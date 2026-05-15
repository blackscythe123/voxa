package com.voxa.android.service

import android.inputmethodservice.InputMethodService
import android.os.SystemClock
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.Toast
import androidx.compose.runtime.Recomposer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.AndroidUiDispatcher
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.voxa.android.data.AudioRecorder
import com.voxa.android.network.TranscriptionApi
import com.voxa.android.ui.keyboard.KeyAction
import com.voxa.android.ui.keyboard.RecState
import com.voxa.android.ui.keyboard.VoxaKeyboard
import com.voxa.android.ui.theme.VoxaTheme
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class VoxaKeyboardInputMethod : InputMethodService(),
    LifecycleOwner,
    ViewModelStoreOwner,
    SavedStateRegistryOwner {

    private val lifecycleRegistry = LifecycleRegistry(this)
    private val store = ViewModelStore()
    private val savedStateController = SavedStateRegistryController.create(this)

    override val lifecycle: Lifecycle get() = lifecycleRegistry
    override val viewModelStore: ViewModelStore get() = store
    override val savedStateRegistry: SavedStateRegistry get() = savedStateController.savedStateRegistry

    // AndroidUiDispatcher provides the dispatcher AND the MonotonicFrameClock that
    // Recomposer.runRecomposeAndApplyChanges() requires. Plain Dispatchers.Main crashes.
    private val recomposerScope = CoroutineScope(AndroidUiDispatcher.Main + SupervisorJob())
    private val ioScope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private lateinit var recomposer: Recomposer

    private val recorder by lazy { AudioRecorder(this) }

    // Reactive keyboard state — read by the Compose tree.
    private var recState by mutableStateOf(RecState.Idle)
    private var timerLabel by mutableStateOf<String?>(null)
    private var recordStartMs = 0L
    private var timerJob: Job? = null

    override fun onCreate() {
        savedStateController.performAttach()
        savedStateController.performRestore(null)
        super.onCreate()
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_CREATE)

        // Own the Recomposer ourselves; otherwise ComposeView walks up to the IME's
        // window rootView for a ViewTreeLifecycleOwner and crashes.
        recomposer = Recomposer(recomposerScope.coroutineContext)
        recomposerScope.launch { recomposer.runRecomposeAndApplyChanges() }
    }

    override fun onCreateInputView(): View {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_START)
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_RESUME)

        val view = ComposeView(this).apply {
            // Tags for things like rememberSaveable; safe even if Compose doesn't walk here.
            setViewTreeLifecycleOwner(this@VoxaKeyboardInputMethod)
            setViewTreeViewModelStoreOwner(this@VoxaKeyboardInputMethod)
            setViewTreeSavedStateRegistryOwner(this@VoxaKeyboardInputMethod)
            // Stop Compose from looking up the tree for a parent composition / lifecycle.
            setParentCompositionContext(recomposer)
            setContent {
                VoxaTheme {
                    VoxaKeyboard(
                        recState = recState,
                        recordingTimerLabel = timerLabel,
                        onAction = ::onAction,
                    )
                }
            }
        }
        return view
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        if (recState == RecState.Recording) {
            stopRecordingAndTranscribe()
        }
    }

    override fun onDestroy() {
        lifecycleRegistry.handleLifecycleEvent(Lifecycle.Event.ON_DESTROY)
        recomposer.cancel()
        recomposerScope.cancel()
        ioScope.cancel()
        super.onDestroy()
    }

    private fun onAction(action: KeyAction) {
        val ic = currentInputConnection ?: return
        when (action) {
            is KeyAction.Char -> ic.commitText(action.c, 1)
            KeyAction.Backspace -> {
                val selected = ic.getSelectedText(0)
                if (!selected.isNullOrEmpty()) {
                    ic.commitText("", 1)
                } else {
                    ic.deleteSurroundingText(1, 0)
                }
            }
            KeyAction.Space -> ic.commitText(" ", 1)
            KeyAction.Enter -> {
                val opts = currentInputEditorInfo?.imeOptions ?: 0
                val editorAction = opts and EditorInfo.IME_MASK_ACTION
                if (editorAction != EditorInfo.IME_ACTION_NONE && editorAction != EditorInfo.IME_ACTION_UNSPECIFIED) {
                    ic.performEditorAction(editorAction)
                } else {
                    ic.commitText("\n", 1)
                }
            }
            KeyAction.Shift, KeyAction.SymbolsToggle, KeyAction.EmojiToggle -> {
                // Layout state lives in the Composable; nothing to do here.
            }
            KeyAction.Mic -> toggleRecording()
        }
    }

    private fun toggleRecording() {
        when (recState) {
            RecState.Idle -> startRecording()
            RecState.Recording -> stopRecordingAndTranscribe()
            RecState.Transcribing -> Unit
        }
    }

    private fun startRecording() {
        recState = RecState.Recording
        recordStartMs = SystemClock.elapsedRealtime()
        timerLabel = "0:00"
        timerJob = ioScope.launch {
            while (recState == RecState.Recording) {
                val secs = (SystemClock.elapsedRealtime() - recordStartMs) / 1000
                timerLabel = "%d:%02d".format(secs / 60, secs % 60)
                delay(250)
            }
        }
        ioScope.launch {
            try {
                recorder.start()
            } catch (e: Exception) {
                recState = RecState.Idle
                timerLabel = null
                timerJob?.cancel()
                toast("Recording failed: ${e.message}")
            }
        }
    }

    private fun stopRecordingAndTranscribe() {
        if (recState != RecState.Recording) return
        recState = RecState.Transcribing
        timerJob?.cancel()
        ioScope.launch {
            try {
                val path = recorder.stop()
                val text = TranscriptionApi.transcribeAudio(path)
                if (text.isNotEmpty()) {
                    currentInputConnection?.commitText(text, 1)
                } else {
                    toast("No speech detected")
                }
            } catch (e: Exception) {
                toast("Transcription failed: ${e.message?.take(60) ?: "unknown"}")
            } finally {
                recState = RecState.Idle
                timerLabel = null
            }
        }
    }

    private fun toast(msg: String) {
        Toast.makeText(applicationContext, msg, Toast.LENGTH_SHORT).show()
    }
}
