package com.voxa.android.data

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.media.MediaRecorder
import android.os.Build
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.io.File
import kotlin.coroutines.resume

private const val TAG = "VoxaAudio"

class AudioRecorder(private val context: Context) {

    private var recorder: MediaRecorder? = null
    private var outputFile: File? = null

    private val audioManager by lazy { context.getSystemService(Context.AUDIO_SERVICE) as AudioManager }
    private var previousAudioMode: Int = AudioManager.MODE_NORMAL
    private var modeChanged: Boolean = false
    private var startedBluetoothSco: Boolean = false
    private var routedCommunicationDevice: Boolean = false

    suspend fun start(): String = withContext(Dispatchers.IO) {
        val file = File(context.cacheDir, "voxa_recording_${System.currentTimeMillis()}.webm")
        outputFile = file

        // 1. Decide & set up routing. If no external mic is wired/paired, this is a no-op
        //    and we record from the built-in mic at MODE_NORMAL with VOICE_RECOGNITION.
        val route = setupAudioRoute()
        Log.i(TAG, "Resolved route: $route")

        // 2. Build MediaRecorder. Audio source choice depends on route:
        //    - BT SCO / wired headset → VOICE_COMMUNICATION so platform negotiates the path
        //    - Built-in → VOICE_RECOGNITION (less processing, cleaner for Whisper)
        val source = when (route) {
            Route.Builtin -> MediaRecorder.AudioSource.VOICE_RECOGNITION
            else -> MediaRecorder.AudioSource.VOICE_COMMUNICATION
        }

        val rec = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) MediaRecorder(context)
                  else @Suppress("DEPRECATION") MediaRecorder()
        rec.apply {
            setAudioSource(source)
            setOutputFormat(MediaRecorder.OutputFormat.WEBM)
            setAudioEncoder(MediaRecorder.AudioEncoder.OPUS)
            setAudioSamplingRate(16000)
            setAudioChannels(1)
            setOutputFile(file.absolutePath)
            prepare()
            start()
        }

        // 3. Hard-bind the recorder to the chosen physical device so the platform
        //    doesn't fall back to a different mic mid-record.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && route != Route.Builtin) {
            bindRecorderToHeadset(rec, route)
        }

        recorder = rec
        file.absolutePath
    }

    fun getMaxAmplitude(): Int = try { recorder?.maxAmplitude ?: 0 } catch (_: Exception) { 0 }
    fun pause() { try { recorder?.pause() } catch (_: Exception) {} }
    fun resume() { try { recorder?.resume() } catch (_: Exception) {} }

    suspend fun stop(): String = withContext(Dispatchers.IO) {
        val path = outputFile?.absolutePath ?: throw IllegalStateException("Not recording")
        try {
            recorder?.apply { stop(); release() }
        } finally {
            recorder = null
            restoreAudioRouting()
        }
        path
    }

    fun release() {
        try { recorder?.release() } catch (_: Exception) {}
        recorder = null
        restoreAudioRouting()
    }

    // ─── routing ──────────────────────────────────────────────────────────────

    private enum class Route { Builtin, WiredHeadset, UsbHeadset, BluetoothSco }

    /**
     * On API 31+: uses `availableCommunicationDevices` (the only list that
     * `setCommunicationDevice` accepts). On older API: SCO start broadcast.
     *
     * Falls back to Built-in cleanly when no headset is paired/connected.
     */
    private suspend fun setupAudioRoute(): Route {
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                setupAudioRouteApi31()
            } else {
                setupAudioRouteLegacy()
            }
        } catch (e: SecurityException) {
            Log.w(TAG, "Audio route security exception: ${e.message}")
            Route.Builtin
        } catch (e: Exception) {
            Log.w(TAG, "Audio route setup failed: ${e.message}")
            Route.Builtin
        }
    }

    private suspend fun setupAudioRouteApi31(): Route {
        val avail = audioManager.availableCommunicationDevices
        for (d in avail) Log.i(TAG, "Avail comm device: ${d.productName} type=${d.type}")

        // Priority: BT SCO > BLE headset > USB > wired > built-in.
        val bt = avail.firstOrNull { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO ||
            it.type == AudioDeviceInfo.TYPE_BLE_HEADSET }
        val usb = avail.firstOrNull { it.type == AudioDeviceInfo.TYPE_USB_HEADSET ||
            it.type == AudioDeviceInfo.TYPE_USB_DEVICE }
        val wired = avail.firstOrNull { it.type == AudioDeviceInfo.TYPE_WIRED_HEADSET }

        val target = bt ?: usb ?: wired
        if (target == null) {
            Log.i(TAG, "No external mic available — using built-in")
            return Route.Builtin
        }

        previousAudioMode = audioManager.mode
        audioManager.mode = AudioManager.MODE_IN_COMMUNICATION
        modeChanged = true

        val ok = audioManager.setCommunicationDevice(target)
        Log.i(TAG, "setCommunicationDevice(${target.productName})=$ok")
        if (!ok) {
            audioManager.mode = previousAudioMode
            modeChanged = false
            return Route.Builtin
        }
        routedCommunicationDevice = true

        // Platform needs a beat for the route to settle (especially BT SCO).
        if (target.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO ||
            target.type == AudioDeviceInfo.TYPE_BLE_HEADSET) {
            delay(400)
            return Route.BluetoothSco
        }
        delay(80)
        return when (target.type) {
            AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE -> Route.UsbHeadset
            AudioDeviceInfo.TYPE_WIRED_HEADSET -> Route.WiredHeadset
            else -> Route.Builtin
        }
    }

    @Suppress("DEPRECATION")
    private suspend fun setupAudioRouteLegacy(): Route {
        val inputs = audioManager.getDevices(AudioManager.GET_DEVICES_INPUTS)
        val bt = inputs.firstOrNull { it.type == AudioDeviceInfo.TYPE_BLUETOOTH_SCO }
        val usb = inputs.firstOrNull { it.type == AudioDeviceInfo.TYPE_USB_HEADSET ||
            it.type == AudioDeviceInfo.TYPE_USB_DEVICE }
        val wired = inputs.firstOrNull { it.type == AudioDeviceInfo.TYPE_WIRED_HEADSET }

        // Wired & USB headsets route automatically — Android picks their mic when MediaRecorder runs.
        if (bt == null) {
            return when {
                usb != null -> Route.UsbHeadset
                wired != null -> Route.WiredHeadset
                else -> Route.Builtin
            }
        }

        // BT requires SCO start + waiting for STATE_CONNECTED.
        previousAudioMode = audioManager.mode
        audioManager.mode = AudioManager.MODE_IN_COMMUNICATION
        modeChanged = true
        startedBluetoothSco = true

        val connected = withTimeoutOrNull(2500L) {
            suspendCancellableCoroutine<Boolean> { cont ->
                val filter = IntentFilter(AudioManager.ACTION_SCO_AUDIO_STATE_UPDATED)
                val receiver = object : BroadcastReceiver() {
                    override fun onReceive(c: Context?, intent: Intent?) {
                        val state = intent?.getIntExtra(AudioManager.EXTRA_SCO_AUDIO_STATE,
                            AudioManager.SCO_AUDIO_STATE_ERROR)
                        if (state == AudioManager.SCO_AUDIO_STATE_CONNECTED) {
                            try { context.unregisterReceiver(this) } catch (_: Exception) {}
                            if (cont.isActive) cont.resume(true)
                        } else if (state == AudioManager.SCO_AUDIO_STATE_DISCONNECTED ||
                                   state == AudioManager.SCO_AUDIO_STATE_ERROR) {
                            try { context.unregisterReceiver(this) } catch (_: Exception) {}
                            if (cont.isActive) cont.resume(false)
                        }
                    }
                }
                context.registerReceiver(receiver, filter)
                cont.invokeOnCancellation { try { context.unregisterReceiver(receiver) } catch (_: Exception) {} }
                audioManager.startBluetoothSco()
                audioManager.isBluetoothScoOn = true
            }
        } ?: false

        if (!connected) {
            Log.w(TAG, "BT SCO did not connect — falling back to built-in")
            audioManager.isBluetoothScoOn = false
            audioManager.stopBluetoothSco()
            audioManager.mode = previousAudioMode
            startedBluetoothSco = false
            modeChanged = false
            return Route.Builtin
        }
        return Route.BluetoothSco
    }

    private fun bindRecorderToHeadset(rec: MediaRecorder, route: Route) {
        try {
            val targetTypes = when (route) {
                Route.BluetoothSco -> intArrayOf(AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
                    AudioDeviceInfo.TYPE_BLE_HEADSET)
                Route.UsbHeadset -> intArrayOf(AudioDeviceInfo.TYPE_USB_HEADSET,
                    AudioDeviceInfo.TYPE_USB_DEVICE)
                Route.WiredHeadset -> intArrayOf(AudioDeviceInfo.TYPE_WIRED_HEADSET)
                Route.Builtin -> return
            }
            val device = audioManager.getDevices(AudioManager.GET_DEVICES_INPUTS)
                .firstOrNull { it.type in targetTypes }
            if (device != null) {
                val ok = rec.setPreferredDevice(device)
                Log.i(TAG, "setPreferredDevice(${device.productName} type=${device.type})=$ok")
            } else {
                Log.w(TAG, "No matching input device to bind for route $route")
            }
        } catch (e: Exception) {
            Log.w(TAG, "setPreferredDevice failed: ${e.message}")
        }
    }

    private fun restoreAudioRouting() {
        try {
            if (routedCommunicationDevice && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                audioManager.clearCommunicationDevice()
                routedCommunicationDevice = false
            }
            if (startedBluetoothSco) {
                @Suppress("DEPRECATION") audioManager.isBluetoothScoOn = false
                @Suppress("DEPRECATION") audioManager.stopBluetoothSco()
                startedBluetoothSco = false
            }
            if (modeChanged) {
                audioManager.mode = previousAudioMode
                modeChanged = false
            }
        } catch (_: Exception) {}
    }
}
