package com.voxa.android.diag

import android.app.ActivityManager
import android.content.Context
import android.os.Debug
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import java.io.File
import java.io.FileOutputStream
import java.io.PrintWriter
import java.io.StringWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * The diagnostics log: files/diag/voxa-YYYYMMDD.log, one file a day, at most 5 MB each (then
 * voxa-YYYYMMDD.1.log), kept 3 days. Kotlin and Rust (through the core's log sink) both write here.
 *
 * Privacy: never cookies, tokens, headers, session JSON, or what the user types. Transcripts only
 * as their length (and the first 20 characters in debug builds, decided in the core).
 */
object Diag {
    @Volatile var enabled = false
        private set
    private lateinit var dir: File
    private val io = Executors.newSingleThreadScheduledExecutor { r -> Thread(r, "voxa-diag").apply { isDaemon = true } }
    private val stamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US)
    private val dayFormat = SimpleDateFormat("yyyyMMdd", Locale.US)

    const val MAX_BYTES = 5L * 1024 * 1024
    const val KEEP_DAYS = 3
    const val SLOW_MAIN_MS = 100L

    fun init(context: Context, on: Boolean) {
        dir = File(context.filesDir, "diag")
        enabled = on
        if (!on) return
        dir.mkdirs()
        io.execute { prune() }
        installCrashHandler()
        watchMainThread()
        val am = context.getSystemService(ActivityManager::class.java)
        log("diag", "started: process ${android.os.Process.myPid()}, memory class ${am.memoryClass} MB, low-RAM device ${am.isLowRamDevice}")
        io.scheduleWithFixedDelay({ writeMemory("every 60 s") }, 60, 60, TimeUnit.SECONDS)
    }

    fun log(tag: String, message: String) {
        if (!enabled) return
        val line = "${stamp.format(Date())} [${Thread.currentThread().name}] $tag: $message\n"
        io.execute { append(line) }
    }

    /** Memory now (total PSS, Java heap, native heap, graphics; Runtime used/max), with why. */
    fun memory(why: String) {
        if (!enabled) return
        io.execute { writeMemory(why) }
    }

    private fun writeMemory(why: String) {
        val m = Debug.MemoryInfo()
        Debug.getMemoryInfo(m)
        val rt = Runtime.getRuntime()
        val kb = { k: String -> m.getMemoryStat(k)?.toIntOrNull() ?: -1 }
        append(
            "${stamp.format(Date())} [voxa-diag] memory: ($why) total PSS ${kb("summary.total-pss") / 1024} MB, " +
                "java heap ${kb("summary.java-heap") / 1024} MB, native heap ${kb("summary.native-heap") / 1024} MB, " +
                "graphics ${kb("summary.graphics") / 1024} MB, code ${kb("summary.code") / 1024} MB, " +
                "runtime used ${(rt.totalMemory() - rt.freeMemory()) / 1_048_576} / max ${rt.maxMemory() / 1_048_576} MB\n",
        )
    }

    private fun fileFor(day: String, part: Int = 0) = File(dir, if (part == 0) "voxa-$day.log" else "voxa-$day.$part.log")

    private fun append(line: String) {
        runCatching {
            val f = fileFor(dayFormat.format(Date()))
            if (f.length() + line.length > MAX_BYTES) {
                // Keep the newest 5 MB plus the previous 5 MB of the day.
                val old = File(f.path.removeSuffix(".log") + ".1.log")
                old.delete()
                f.renameTo(old)
            }
            FileOutputStream(f, true).use { it.write(line.toByteArray()) }
        }
    }

    private fun prune() {
        val cutoff = System.currentTimeMillis() - KEEP_DAYS * 86_400_000L
        dir.listFiles()?.filter { it.lastModified() < cutoff }?.forEach { it.delete() }
    }

    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            // Written here and now, not on the log thread: the process is about to die.
            val sw = StringWriter()
            e.printStackTrace(PrintWriter(sw))
            runCatching { append("${stamp.format(Date())} [${t.name}] CRASH: $sw\n") }
            previous?.uncaughtException(t, e)
        }
    }

    /** Logs every main-thread message that takes longer than [SLOW_MAIN_MS] (ANR-prone work). */
    private fun watchMainThread() {
        var startedAt = 0L
        Looper.getMainLooper().setMessageLogging { s ->
            if (s.startsWith(">>>>> ")) {
                startedAt = SystemClock.uptimeMillis()
            } else if (s.startsWith("<<<<< ") && startedAt > 0) {
                val ms = SystemClock.uptimeMillis() - startedAt
                if (ms > SLOW_MAIN_MS) log("slow", "main thread busy $ms ms: ${s.removePrefix("<<<<< Finished to ").take(160)}")
            }
        }
    }

    /** Zip the diag folder into [outDir] (for the share sheet). Returns the zip. */
    fun zip(outDir: File): File {
        val done = java.util.concurrent.CountDownLatch(1)
        io.execute { done.countDown() } // let queued lines land first
        done.await(2, TimeUnit.SECONDS)
        outDir.mkdirs()
        outDir.listFiles()?.forEach { it.delete() }
        val out = File(outDir, "voxa-diagnostics-${SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())}.zip")
        ZipOutputStream(FileOutputStream(out)).use { z ->
            dir.listFiles()?.sortedBy { it.name }?.forEach { f ->
                z.putNextEntry(ZipEntry(f.name))
                f.inputStream().use { it.copyTo(z) }
                z.closeEntry()
            }
        }
        Log.i("Voxa", "diagnostics zipped: ${out.name} (${out.length()} bytes)")
        return out
    }
}
