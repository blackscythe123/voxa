package com.voxa.android.notify

import android.app.job.JobInfo
import android.app.job.JobParameters
import android.app.job.JobScheduler
import android.app.job.JobService
import android.content.ComponentName
import android.content.Context
import android.os.PersistableBundle
import com.voxa.android.BuildConfig
import com.voxa.android.diag.Diag
import com.voxa.android.voxa
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.SendOutcome

/**
 * Voxa's background work, on the framework JobScheduler (no WorkManager):
 * - [SESSION]: every 12 h on any network, battery not low, kept across restarts. Checks the
 *   ChatGPT session; a definite "signed out" after it was good is confirmed 2 minutes later in
 *   the same job before A1 is posted.
 * - [CONFIRM]: that 2 minute confirmation, for checks the app itself made.
 * - [SEND]: dictations recorded offline ("Sends when you're back online") are sent once there is
 *   a network, with exponential backoff, even when the keyboard is gone. Gives up after 24 h or
 *   [MAX_ATTEMPTS] tries and posts A3.
 */
class VoxaJobs : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    override fun onStartJob(params: JobParameters): Boolean {
        val app = voxa
        Diag.log("jobs", "start ${name(params.jobId)}")
        when (params.jobId) {
            SESSION -> scope.launch {
                runCatching {
                    if (app.debugReady) return@runCatching
                    val r = app.checkSessionNow()
                    if (Notifier.afterCheck(app, r, app.definiteExpiry(r))) {
                        delay(Notifier.confirmDelayMs(app))
                        val again = app.checkSessionNow()
                        Notifier.confirm(app, again, app.definiteExpiry(again))
                    }
                }.onFailure { Diag.log("jobs", "session check failed: ${it.javaClass.simpleName}") }
                jobFinished(params, false)
            }
            CONFIRM -> scope.launch {
                runCatching {
                    val r = app.checkSessionNow()
                    Notifier.confirm(app, r, app.definiteExpiry(r))
                }.onFailure { Diag.log("jobs", "confirm failed: ${it.javaClass.simpleName}") }
                jobFinished(params, false)
            }
            SEND -> scope.launch {
                val again = runCatching { send(params.extras.getBoolean(EXTRA_MANUAL, false)) }
                    .onFailure { Diag.log("jobs", "send failed: ${it.javaClass.simpleName}") }
                    .getOrDefault(true)
                jobFinished(params, again)
            }
            else -> return false
        }
        return true
    }

    /** Cut short by the system (network lost, quota): run again later. */
    override fun onStopJob(params: JobParameters): Boolean {
        Diag.log("jobs", "stopped ${name(params.jobId)} by the system")
        return true
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    /** Send what waits for the network. Returns true to try again later (backoff). */
    private suspend fun send(manual: Boolean): Boolean {
        val app = voxa
        val prefs = getSharedPreferences("notify", MODE_PRIVATE)
        val failedBefore = if (manual) prefs.getString("bg_failed", "").orEmpty().split(',').mapNotNull { it.toULongOrNull() } else emptyList()
        val ids = withContext(Dispatchers.IO) { app.core.waitingForNetwork() } + failedBefore
        if (ids.isEmpty()) {
            prefs.edit().remove("send_attempts").apply()
            Diag.log("jobs", "send: nothing waiting")
            return false
        }
        val attempt = prefs.getInt("send_attempts", 0) + 1
        prefs.edit().putInt("send_attempts", attempt).apply()
        val canned = if (BuildConfig.DEBUG) app.debugTranscript else null
        // Debug builds only (--ez debug_send_unreachable true): every send ends "unreachable", to test A3.
        val unreachable = BuildConfig.DEBUG && app.getSharedPreferences("debug", MODE_PRIVATE).getBoolean("send_unreachable", false)
        val now = System.currentTimeMillis()
        val ready = mutableListOf<ULong>()
        var readyApp = ""
        val failed = mutableListOf<ULong>()
        var again = false
        var needsSignIn = false
        for (id in ids.distinct()) {
            // A dictation the keyboard is still handling is the keyboard's to send.
            if (app.keyboardOwns(id)) {
                Diag.log("jobs", "send: $id belongs to the keyboard")
                continue
            }
            val giveUp = manual || now - id.toLong() > DAY_MS || attempt >= MAX_ATTEMPTS
            val outcome = if (unreachable) SendOutcome.Unreachable(false) else withContext(Dispatchers.IO) { app.core.sendWaiting(id, giveUp, canned) }
            app.keepRefreshedCookies()
            Diag.log("jobs", "send: $id -> ${outcome.javaClass.simpleName} (attempt $attempt${if (giveUp) ", last try" else ""})")
            when (outcome) {
                is SendOutcome.Sent -> {
                    ready += id
                    readyApp = outcome.app
                }
                is SendOutcome.Offline -> again = true
                is SendOutcome.Unreachable -> if (giveUp) failed += id else again = true
                is SendOutcome.NeedsSignIn -> needsSignIn = true
                else -> Unit
            }
        }
        app.historyChanged()
        if (ready.isNotEmpty()) Notifier.ready(app, ready, readyApp)
        if (failed.isNotEmpty()) {
            prefs.edit().putString("bg_failed", failed.joinToString(",")).apply()
            Notifier.notSent(app, failed.size)
        } else if (manual) {
            prefs.edit().remove("bg_failed").apply()
        }
        if (needsSignIn) Notifier.refreshSignIn(app)
        if (!again) prefs.edit().remove("send_attempts").apply()
        return again
    }

    companion object {
        const val SESSION = 7101
        const val CONFIRM = 7102
        const val SEND = 7103
        const val MAX_ATTEMPTS = 6
        private const val DAY_MS = 24 * 60 * 60_000L
        private const val EXTRA_MANUAL = "manual"

        private fun name(id: Int) = when (id) {
            SESSION -> "session check"
            CONFIRM -> "sign-in confirmation"
            SEND -> "background send"
            else -> "job $id"
        }

        private fun component(ctx: Context) = ComponentName(ctx, VoxaJobs::class.java)
        private fun scheduler(ctx: Context) = ctx.getSystemService(JobScheduler::class.java)

        /** The 12 hour session check (scheduled once; kept across reboots). */
        fun schedulePeriodic(ctx: Context) {
            val js = scheduler(ctx)
            if (js.getPendingJob(SESSION) != null) return
            val r = js.schedule(
                JobInfo.Builder(SESSION, component(ctx))
                    .setPeriodic(12 * 60 * 60_000L)
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .setRequiresBatteryNotLow(true)
                    .setPersisted(true)
                    .build(),
            )
            Diag.log("jobs", "session check every 12 h: ${if (r == JobScheduler.RESULT_SUCCESS) "scheduled" else "not scheduled"}")
        }

        /** Confirm a definite "signed out" about 2 minutes from now. */
        fun scheduleConfirm(ctx: Context, delayMs: Long) {
            scheduler(ctx).schedule(
                JobInfo.Builder(CONFIRM, component(ctx))
                    .setMinimumLatency(delayMs)
                    .setOverrideDeadline(delayMs + 10 * 60_000L)
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .build(),
            )
        }

        /** Send dictations waiting for the network once there is one. `manual`: the user's Retry. */
        fun scheduleSend(ctx: Context, manual: Boolean = false) {
            val js = scheduler(ctx)
            if (!manual && js.getPendingJob(SEND) != null) return
            val r = js.schedule(
                JobInfo.Builder(SEND, component(ctx))
                    .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                    .setBackoffCriteria(30_000L, JobInfo.BACKOFF_POLICY_EXPONENTIAL)
                    .setPersisted(true)
                    .setExtras(PersistableBundle().apply { putBoolean(EXTRA_MANUAL, manual) })
                    .build(),
            )
            Diag.log("jobs", "background send ${if (manual) "(retry) " else ""}${if (r == JobScheduler.RESULT_SUCCESS) "scheduled" else "not scheduled"}")
        }

        /** At start and whenever History changes: is anything waiting for the network? */
        fun scheduleSendIfWaiting(ctx: Context) {
            if (ctx.voxa.core.waitingForNetwork().isNotEmpty()) scheduleSend(ctx)
        }
    }
}
