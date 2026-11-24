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

/**
 * The microphone: 16 kHz mono PCM16 straight to a raw file from the first byte, so a take survives
 * the keyboard's process dying. Levels go to [onLevel] (the core's meter scale). Losing audio focus
 * or being silenced by the system calls [onInterrupted]: the keyboard treats it as Stop.
 */
class Recorder(
    private val context: Context,
    private val onLevel: (Float) -> Unit,
    private val onInterrupted: () -> Unit,
) {
    enum class StartError { Denied, Missing }

    private val main = Handler(Looper.getMainLooper())
    private val audio = context.getSystemService(AudioManager::class.java)
    private var record: AudioRecord? = null
    private var thread: Thread? = null
    @Volatile private var running = false
    private var startedAt = 0L
    private var path: String? = null
    private var focus: AudioFocusRequest? = null

    private val recordingCallback = object : AudioManager.AudioRecordingCallback() {
        override fun onRecordingConfigChanged(configs: MutableList<AudioRecordingConfiguration>) {
            val mine = record?.audioSessionId ?: return
            if (configs.any { it.clientAudioSessionId == mine && it.isClientSilenced }) {
                Log.i(TAG, "recording silenced by the system")
                main.post(onInterrupted)
            }
        }
    }

    val isRecording get() = running

    @SuppressLint("MissingPermission")
    fun start(pcmPath: String): StartError? {
        if (running) return null
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
            FileOutputStream(pcmPath)
        } catch (e: Exception) {
            r.release()
            return StartError.Missing
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
        startedAt = SystemClock.elapsedRealtime()
        thread = Thread({
            val buf = ByteArray(CHUNK_BYTES)
            out.use {
                while (running) {
                    val n = r.read(buf, 0, buf.size)
                    if (n <= 0) continue
                    it.write(buf, 0, n)
                    val lvl = meterLevel(buf.copyOf(n))
                    main.post { onLevel(lvl) }
                }
                it.flush()
                it.fd.sync()
            }
        }, "voxa-mic").apply { start() }
        return null
    }

    /** Stop and keep the recording. Returns how long it ran, in seconds. */
    fun stop(): Float {
        if (!running) return 0f
        running = false
        thread?.join(1000)
        thread = null
        release()
        return (SystemClock.elapsedRealtime() - startedAt) / 1000f
    }

    /** Stop and delete the recording. */
    fun discard() {
        stop()
        path?.let { File(it).delete() }
    }

    private fun release() {
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
                if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                    Log.i(TAG, "audio focus lost")
                    if (running) onInterrupted()
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
    }
}
