package com.voxa.android.ime

import android.media.AudioManager

/**
 * The keyboard's small decisions, kept apart from the InputMethodService so each one has a
 * regression test (androidTest RegressionsTest, docs/regressions.md). VoxaKeyboard and Recorder
 * only carry them out.
 */
object KeyRules {
    /** A press longer than this is hold-to-talk. */
    const val HOLD_MS = 400L

    /**
     * Mic released. A tap while recording always stops (tap-to-stop did nothing once); only the
     * press that started the take can be a hold, and with hold-to-talk a long one stops on release.
     */
    fun stopOnRelease(recording: Boolean, pressStarted: Boolean, holdToTalk: Boolean, heldMs: Long): Boolean =
        recording && (!pressStarted || (holdToTalk && heldMs > HOLD_MS))

    /** The press was taken away (view rebuilt, parent took the touch): a hold ends as if released. */
    fun stopOnCancel(recording: Boolean, pressStarted: Boolean, holdToTalk: Boolean, heldMs: Long): Boolean =
        recording && pressStarted && holdToTalk && heldMs > HOLD_MS

    /** Phone call, VoIP call (WhatsApp, Meet), call screening or a redirected call. */
    val CALL_MODES = setOf(
        AudioManager.MODE_IN_CALL, AudioManager.MODE_IN_COMMUNICATION,
        4 /* MODE_CALL_SCREENING */, 5 /* MODE_CALL_REDIRECT */, 6 /* MODE_COMMUNICATION_REDIRECT */,
    )

    /** During a call the call owns the microphone (others only get silence): refuse to record. */
    fun inCall(audioMode: Int): Boolean = audioMode in CALL_MODES

    enum class MicTaken { CancelInCall, CancelMicBusy, StopAndSend }

    /** The system took the microphone mid-take: keep what was said; nothing heard means drop it and say why. */
    fun micTaken(peakLevel: Float, inCall: Boolean): MicTaken = when {
        peakLevel > 0.001f -> MicTaken.StopAndSend
        inCall -> MicTaken.CancelInCall
        else -> MicTaken.CancelMicBusy
    }

    /**
     * The field's identity: package, view id and this focus of it. A view id alone can't tell one
     * chat from another (one compose box for every chat), so a text never goes into a field the
     * user moved to and came back to without confirming.
     */
    fun fieldIdentity(pkg: String, fieldId: Int, inputSession: Long): Long {
        var window = pkg.hashCode().toLong()
        window = window * 1_000_003L + fieldId
        window = window * 1_000_003L + inputSession
        return window
    }

    /** Audio focus lost for good ends the take, except in the first second (the system settling). */
    fun focusLossEndsTake(change: Int, sinceStartMs: Long): Boolean =
        change == AudioManager.AUDIOFOCUS_LOSS && sinceStartMs > 1000

    /** How long a "silenced by the system" must last before it counts (some phones blip at start). */
    fun silencedConfirmDelayMs(sinceStartMs: Long): Long = maxOf(1500L, 1000L - sinceStartMs + 1500L)

    /** Read errors in a row that end the take (a dead recorder returns at once, forever: never spin). */
    fun readErrorsEndTake(errorsInARow: Int): Boolean = errorsInARow >= Recorder.MAX_READ_ERRORS
}
