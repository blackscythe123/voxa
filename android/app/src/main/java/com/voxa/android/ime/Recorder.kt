package com.voxa.android.ime

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioRecordingConfiguration
import android.media.MediaRecorder
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import uniffi.voxa_ffi.meterLevel
import java.io.File
import java.io.FileOutputStream
import java.io.IOException

/**
 * The microphone: 16 kHz mono PCM16 straight to a raw file from the first byte, so a take survives
 * the keyboard's process dying. Levels go to [onLevel] (the core's meter scale). Losing audio focus
 * for good, being silenced by the system or the mic failing calls [onInterrupted]: the keyboard
 * treats it as Stop. A write that fails (storage full) calls [onCantSave]; what was written is kept.
 */
class Recorder(
    private val context: Context,
    private val onLevel: (Float) -> Unit,
    private val onInterrupted: () -> Unit,
    private val onCantSave: () -> Unit,
) {
    enum class StartError { Denied, Missing, CantSave }

    private val main = Handler(Looper.getMainLooper())
    private val audio = context.getSystemService(AudioManager::class.java)
    private var record: AudioRecord? = null
    private var thread: Thread? = null
    @Volatile private var running = false
    private var startedAt = 0L
    private var path: String? = null
    private var focus: AudioFocusRequest? = null

    /** Some phones (seen on OnePlus, Android 14) report the mic as silenced for a moment while
     *  a recording starts. Only a silence that lasts, after the first second, is a real
     *  interruption (a call, another app taking the mic). */
    @Volatile private var silenced = false
    private val confirmSilenced = Runnable {
        if (running && silenced) {
            Log.i(TAG, "recording silenced by the system (lasting)")
            com.voxa.android.diag.Diag.log("mic", "interrupted: silenced by the system")
            onInterrupted()
        }
    }

    private val recordingCallback = object : AudioManager.AudioRecordingCallback() {
        override fun onRecordingConfigChanged(configs: MutableList<AudioRecordingConfiguration>) {
            val mine = record?.audioSessionId ?: return
            val now = configs.any { it.clientAudioSessionId == mine && it.isClientSilenced }
            if (now != silenced) com.voxa.android.diag.Diag.log("mic", "system silenced: $now")
            silenced = now
            main.removeCallbacks(confirmSilenced)
            if (now) {
                val sinceStart = SystemClock.elapsedRealtime() - startedAt
                main.postDelayed(confirmSilenced, KeyRules.silencedConfirmDelayMs(sinceStart))
            }
        }
    }

    val isRecording get() = running
    /** The take is paused: the mic is released (focus, AudioRecord, callback) and the file closed. */
    @Volatile var isPaused = false
        private set
    /** Bytes written and the loudest meter level of the current or last take (diagnostics). */
    @Volatile var bytesWritten = 0L
        private set
    /** Silence written at resumes (part of [bytesWritten], not of the talking time). */
    private var silenceBytes = 0L
    private var resumes = 0
    @Volatile var peakLevel = 0f
        private set

    fun start(pcmPath: String): StartError? {
        if (running || isPaused) return null
        val err = open(pcmPath, append = false)
        if (err == null) {
            bytesWritten = 0L
            silenceBytes = 0L
            resumes = 0
            peakLevel = 0f
        }
        return err
    }

    /** Turn the mic off mid-take: the same release as stopping (focus abandoned, AudioRecord
     *  stopped, callback unregistered) and the file closed. Nothing is recorded until [resume]. */
    fun pause() {
        if (!running) return
        running = false
        thread?.join(1000)
        thread = null
        release()
        isPaused = true
    }

    /** Turn the mic back on and append to the same raw PCM file, after ~300 ms of silence so
     *  words either side of the pause don't run together. */
    fun resume(): StartError? {
        if (!isPaused) return null
        val p = path ?: return StartError.Missing
        val err = open(p, append = true)
        if (err == null) isPaused = false
        return err
    }

    @SuppressLint("MissingPermission")
    private fun open(pcmPath: String, append: Boolean): StartError? {
        if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) return StartError.Denied
        val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return StartError.Missing
        val r = try {
            AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, CHUNK_BYTES * 4))
        } catch (e: SecurityException) {
            return StartError.Denied
        } catch (e: IllegalArgumentException) {
            return StartError.Missing
        }
        if (r.state != AudioRecord.STATE_INITIALIZED) {
            r.release()
            return StartError.Missing
        }
        val out = try {
            File(pcmPath).parentFile?.mkdirs()
            FileOutputStream(pcmPath, append).also {
                if (append && resumes++ < MAX_GAPS) {
                    it.write(ByteArray(GAP_BYTES))
                    bytesWritten += GAP_BYTES
                    silenceBytes += GAP_BYTES
                }
            }
        } catch (e: Exception) {
            // The file can't be created or written: storage full (or the folder is gone).
            r.release()
            return StartError.CantSave
        }
        requestFocus()
        try {
            r.startRecording()
        } catch (e: IllegalStateException) {
            out.close()
            r.release()
            abandonFocus()
            return StartError.Denied
        }
        audio.registerAudioRecordingCallback(recordingCallback, main)
        record = r
        path = pcmPath
        running = true
        // Also on resume, so the first-second grace for focus and silencing applies again.
        startedAt = SystemClock.elapsedRealtime()
        thread = Thread({
            val buf = ByteArray(CHUNK_BYTES)
            var errors = 0
            try {
                out.use {
                    while (running) {
                        val n = r.read(buf, 0, buf.size)
                        if (n <= 0) {
                            // A dead or invalid recorder returns at once, forever: never spin. A
                            // short run of errors is a hiccup; a lasting one ends the take.
                            if (n < 0 && KeyRules.readErrorsEndTake(++errors)) {
                                com.voxa.android.diag.Diag.log("mic", "read failed $errors times (code $n): stopping")
                                main.post { if (running) onInterrupted() }
                                break
                            }
                            Thread.sleep(READ_ERROR_WAIT_MS)
                            continue
                        }
                        errors = 0
                        it.write(buf, 0, n)
                        bytesWritten += n
                        val lvl = meterLevel(buf.copyOf(n))
                        if (lvl > peakLevel) peakLevel = lvl

                        main.post { onLevel(lvl) }
                    }
                    it.flush()
                    it.fd.sync()
                }
            } catch (e: IOException) {
                // Storage full mid-take: never crash the keyboard. What reached the file is kept.
                com.voxa.android.diag.Diag.log("mic", "write failed: ${e.javaClass.simpleName}")
                main.post { if (running) onCantSave() }
            } catch (e: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        }, "voxa-mic").apply { start() }
        return null
    }

    /** Stop and keep the recording (also from paused). Returns the talking time in seconds, from
     *  the audio written, so paused time is left out. */
    fun stop(): Float {
        if (!running && !isPaused) return 0f
        if (running) {
            running = false
            thread?.join(1000)
            thread = null
            release()
        }
        isPaused = false
        return talkingSecs()
    }

    /** Seconds of talking in the take so far: what was written, minus the silence at resumes. */
    fun talkingSecs(): Float = (bytesWritten - silenceBytes) / BYTES_PER_SEC.toFloat()

    /** Stop and delete the recording. */
    fun discard() {
        stop()
        path?.let { File(it).delete() }
    }

    private fun release() {
        main.removeCallbacks(confirmSilenced)
        silenced = false
        runCatching { audio.unregisterAudioRecordingCallback(recordingCallback) }
        record?.let {
            runCatching { it.stop() }
            it.release()
        }
        record = null
        abandonFocus()
    }

    private fun requestFocus() {
        val req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setOnAudioFocusChangeListener({ change ->
                val sinceStart = SystemClock.elapsedRealtime() - startedAt
                if (change == AudioManager.AUDIOFOCUS_LOSS) {
                    com.voxa.android.diag.Diag.log("mic", "audio focus lost after $sinceStart ms")
                    // A focus change in the first second is the system settling, not a call.
                    if (running && KeyRules.focusLossEndsTake(change, sinceStart)) onInterrupted()
                } else if (change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                    // An alarm, a timer or a navigation prompt: recording still works, so the take
                    // goes on. A call silences the mic, and that is caught separately.
                    com.voxa.android.diag.Diag.log("mic", "audio focus lost for a moment after $sinceStart ms: still listening")
                }
            }, main)
            .build()
        focus = req
        audio.requestAudioFocus(req)
    }

    private fun abandonFocus() {
        focus?.let { audio.abandonAudioFocusRequest(it) }
        focus = null
    }

    companion object {
        const val TAG = "VoxaMic"
        const val RATE = 16_000
        /** 40 ms of audio, one ink-line tick. */
        const val CHUNK_BYTES = RATE * 2 * 40 / 1000
        const val BYTES_PER_SEC = RATE * 2
        /** ~300 ms of silence written at each resume. */
        const val GAP_BYTES = BYTES_PER_SEC * 3 / 10
        /** After this many pauses in one take, resume without adding more silence. */
        const val MAX_GAPS = 50
        /** Read errors in a row (about half a second) before the take ends. */
        const val MAX_READ_ERRORS = 25
        const val READ_ERROR_WAIT_MS = 20L
    }
}
