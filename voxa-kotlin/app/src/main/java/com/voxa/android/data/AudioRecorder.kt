package com.voxa.android.data

import android.content.Context
import android.media.MediaRecorder
import android.os.Build
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

class AudioRecorder(private val context: Context) {

    private var recorder: MediaRecorder? = null
    private var outputFile: File? = null

    suspend fun start(): String = withContext(Dispatchers.IO) {
        val file = File(context.cacheDir, "voxa_recording_${System.currentTimeMillis()}.webm")
        outputFile = file

        val rec = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            MediaRecorder(context)
        } else {
            @Suppress("DEPRECATION")
            MediaRecorder()
        }

        rec.apply {
            setAudioSource(MediaRecorder.AudioSource.MIC)
            setOutputFormat(MediaRecorder.OutputFormat.WEBM)
            setAudioEncoder(MediaRecorder.AudioEncoder.OPUS)
            setAudioSamplingRate(16000)
            setAudioChannels(1)
            setOutputFile(file.absolutePath)
            prepare()
            start()
        }

        recorder = rec
        file.absolutePath
    }

    suspend fun stop(): String = withContext(Dispatchers.IO) {
        val path = outputFile?.absolutePath ?: throw IllegalStateException("Not recording")
        try {
            recorder?.apply {
                stop()
                release()
            }
        } finally {
            recorder = null
        }
        path
    }

    fun release() {
        try {
            recorder?.release()
        } catch (_: Exception) {}
        recorder = null
    }
}
