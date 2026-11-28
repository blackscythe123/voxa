package com.voxa.android.ui

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Handler
import android.os.Looper
import java.io.File
import java.io.RandomAccessFile

/**
 * Plays a History recording: the raw 16 kHz mono PCM16 file the keyboard saved, streamed through
 * an AudioTrack from a small buffer (no decoder, no whole-file copy in memory).
 */
class PcmPlayer {
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var track: AudioTrack? = null
    @Volatile private var running = false
    private var thread: Thread? = null
    private var totalFrames = 1L
    private var startFrame = 0L

    val playing get() = running

    /** Seconds in a recording file. */
    fun durationSecs(path: String): Float = File(path).length() / 2f / RATE

    /** Play from [fromFraction] (0..1); [onEnd] runs on the main thread when it reaches the end. */
    fun play(path: String, fromFraction: Float, onEnd: () -> Unit) {
        stop()
        val file = File(path)
        if (!file.exists()) return
        totalFrames = (file.length() / 2).coerceAtLeast(1)
        startFrame = (totalFrames * fromFraction.coerceIn(0f, 1f)).toLong().let { if (it >= totalFrames - RATE / 10) 0L else it }
        val min = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val t = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setAudioFormat(AudioFormat.Builder().setSampleRate(RATE).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build())
            .setBufferSizeInBytes(maxOf(min, 8192))
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        track = t
        running = true
        t.play()
        thread = Thread({
            val buf = ByteArray(4096)
            var written = 0L
            runCatching {
                RandomAccessFile(file, "r").use { raf ->
                    raf.seek(startFrame * 2)
                    while (running) {
                        val n = raf.read(buf)
                        if (n <= 0) break
                        t.write(buf, 0, n - n % 2)
                        written += n / 2
                    }
                }
            }
            // Let what is buffered play out, then report the end.
            while (running && t.playbackHeadPosition < written) Thread.sleep(20)
            if (running && track === t) {
                running = false
                main.post {
                    if (track === t) {
                        runCatching { t.stop() }
                        t.release()
                        track = null
                    }
                    onEnd()
                }
            }
        }, "voxa-play").apply { start() }
    }

    /** Where playback is, 0..1 of the whole recording. */
    fun progress(): Float {
        val t = track ?: return 0f
        val head = runCatching { t.playbackHeadPosition.toLong() }.getOrDefault(0L)
        return ((startFrame + head).toFloat() / totalFrames).coerceIn(0f, 1f)
    }

    /** Stop (or pause: the caller keeps the progress and plays from it again). */
    fun stop() {
        running = false
        val t = track
        // Silence first, then let the writer thread see `running` and leave.
        t?.let { runCatching { it.pause(); it.flush() } }
        thread?.join(500)
        thread = null
        t?.let {
            runCatching { it.stop() }
            it.release()
        }
        track = null
    }

    companion object {
        const val RATE = 16_000
    }
}
