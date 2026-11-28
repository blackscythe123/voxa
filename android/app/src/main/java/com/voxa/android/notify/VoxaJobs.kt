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
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.SendOutcome
import uniffi.voxa_ffi.SessionState

/**
 * Voxa's background work, on the framework JobScheduler (no WorkManager):
 * - [SESSION]: every 12 h on any network, battery not low, kept across restarts. Checks the
 *   ChatGPT session; a definite "signed out" after it was good is confirmed 2 minutes later in
 *   the same job before A1 is posted.
 * - [CONFIRM]: that 2 minute confirmation, for checks the app itself made.
 * - [SEND]: dictations recorded offline ("Sends when you're back online"), and ones waiting for a
 *   sign-in once signed in, are sent once there is a network, with exponential backoff, even when
 *   the keyboard is gone. Long recordings wait for an unmetered network. Each entry gives up 24 h
 *   after it was recorded, or after [MAX_ATTEMPTS] real tries, and posts A3 (never for Cloudflare
 *   blocking). Being offline never gives up. One send runs at a time, and the core never uploads
 *   an entry that is already being sent.
 *
 * Runs off the main thread (History and the vault are read here).
 */
class VoxaJobs : JobService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    /** The coroutine of each running job, so the system's stop really stops it. */
    private val running = HashMap<Int, Job>()

    override fun onStartJob(params: JobParameters): Boolean {
        val app = voxa
        Diag.log("jobs", "start ${name(params.jobId)}")
        val job = when (params.jobId) {
            SESSION -> scope.launch {
                runCatching {
                    if (app.debugReady) return@runCatching
                    val r = app.checkSessionNow()
                    if (Notifier.afterCheck(app, r, app.definiteExpiry(r))) {
                        delay(Notifier.confirmDelayMs(app))
                        val again = app.checkSessionNow()
                        Notifier.confirm(app, again, app.definiteExpiry(again))
                    }
                }.onFailure { if (it is CancellationException) throw it; Diag.log("jobs", "session check failed: ${it.javaClass.simpleName}") }
                jobFinished(params, false)
            }
            CONFIRM -> scope.launch {
                runCatching {
                    val r = app.checkSessionNow()
                    Notifier.confirm(app, r, app.definiteExpiry(r))
                }.onFailure { if (it is CancellationException) throw it; Diag.log("jobs", "confirm failed: ${it.javaClass.simpleName}") }
                jobFinished(params, false)
            }
            SEND -> scope.launch {
                if (!sending.compareAndSet(false, true)) {
                    // Another send loop is still finishing an upload: it does one more pass after
                    // (a plain one: only the user's Retry makes a pass manual).
                    Diag.log("jobs", "send: one is already running")
                    passAsked.set(true)
                    jobFinished(params, false)
                    return@launch
                }
                var manual = params.extras.getBoolean(EXTRA_MANUAL, false)
                var again: Boolean
                try {
                    do {
                        retryAsked.set(false)
                        passAsked.set(false)
                        again = runCatching { send(manual) }
                            .onFailure { if (it is CancellationException) throw it; Diag.log("jobs", "send failed: ${it.javaClass.simpleName}") }
                            .getOrDefault(true)
                        // The user tapped Retry while this loop was sending: one more pass, as a retry.
                        manual = retryAsked.get()
                    } while (manual || passAsked.get())
                } finally {
                    sending.set(false)
                }
                jobFinished(params, again)
            }
            else -> return false
        }
        synchronized(running) { running[params.jobId] = job }
        job.invokeOnCompletion { synchronized(running) { if (running[params.jobId] === job) running.remove(params.jobId) } }
        return true
    }

    /** Cut short by the system (network lost, quota): stop the work now, run again later. An upload
     *  already inside the core finishes on its own; the core keeps it from being sent twice. */
    override fun onStopJob(params: JobParameters): Boolean {
        Diag.log("jobs", "stopped ${name(params.jobId)} by the system")
        synchronized(running) { running.remove(params.jobId) }?.cancel()
        return true
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    /** Send what waits for the network (or a sign-in, once signed in). Returns true to try again
     *  later (backoff). */
    private suspend fun send(manual: Boolean): Boolean {
        val app = voxa
        app.awaitSession()
        val prefs = getSharedPreferences("notify", MODE_PRIVATE)
        val failedBefore = if (manual) ids(prefs.getString("bg_failed", "")) else emptyList()
        val ids = waiting(app) + failedBefore
        // Tries per entry ("id:count,..."), so one entry's failures never cut another's short.
        val attempts = prefs.getString("send_attempts_by_entry", "").orEmpty().split(',').mapNotNull {
            val (id, n) = it.split(':').takeIf { p -> p.size == 2 } ?: return@mapNotNull null
            (id.toULongOrNull() ?: return@mapNotNull null) to (n.toIntOrNull() ?: 0)
        }.toMap().toMutableMap()
        if (ids.isEmpty()) {
            prefs.edit().remove("send_attempts_by_entry").apply()
            Diag.log("jobs", "send: nothing waiting")
            return false
        }
        val canned = if (BuildConfig.DEBUG) app.debugTranscript else null
        // Debug builds only (--ez debug_send_unreachable true): every send ends "unreachable", to test A3.
        val unreachable = BuildConfig.DEBUG && app.getSharedPreferences("debug", MODE_PRIVATE).getBoolean("send_unreachable", false)
        val now = System.currentTimeMillis()
        val ready = mutableListOf<ULong>()
        var readyApp = ""
        val failed = mutableListOf<ULong>()
        var again = false
        var needsSignIn = false
        var changed = false
        for (id in ids.distinct()) {
            // A dictation the keyboard is recording or writing out is the keyboard's: look again
            // later (with backoff), never in a tight loop.
            if (app.keyboardOwns(id)) {
                Diag.log("jobs", "send: $id belongs to the keyboard")
                again = true
                continue
            }
            val attempt = (attempts[id] ?: 0) + 1
            // The entry's id is when it was recorded: it gives up a day later, after enough real
            // tries, or when the user's Retry is for this one (a failed one, not a waiting one).
            val giveUp = id in failedBefore || now - id.toLong() > DAY_MS || attempt >= MAX_ATTEMPTS
            val outcome = if (unreachable) SendOutcome.Unreachable(false) else withContext(Dispatchers.IO) { app.core.sendWaiting(id, giveUp, canned) }
            app.keepRefreshedCookies()
            Diag.log("jobs", "send: $id -> ${outcome.javaClass.simpleName} (attempt $attempt${if (giveUp) ", last try" else ""})")
            when (outcome) {
                is SendOutcome.Sent -> {
                    ready += id
                    readyApp = outcome.app
                    attempts.remove(id)
                    changed = true
                }
                // Only real tries count towards the cap; offline never gives up.
                is SendOutcome.Offline -> {
                    attempts[id] = attempt
                    again = true
                }
                is SendOutcome.Busy -> again = true
                // Cloudflare blocking never posts "couldn't send" (it stays in History).
                is SendOutcome.Unreachable -> {
                    if (!giveUp) {
                        attempts[id] = attempt
                        again = true
                    } else {
                        attempts.remove(id)
                        changed = true
                        if (!outcome.blocked) failed += id
                    }
                }
                is SendOutcome.NeedsSignIn -> {
                    needsSignIn = true
                    attempts.remove(id)
                }
                else -> {
                    attempts.remove(id)
                    changed = true
                }
            }
        }
        prefs.edit().putString("send_attempts_by_entry", attempts.entries.joinToString(",") { "${it.key}:${it.value}" }).apply()
        // Only when something changed: a History change schedules another send, and a pass that
        // changed nothing must not start the next one at once.
        if (changed) app.historyChanged()
        if (ready.isNotEmpty()) Notifier.ready(app, ready, readyApp)
        if (failed.isNotEmpty()) {
            addFailed(app, failed)
            Notifier.notSent(app, failed.size)
        } else if (manual) {
            prefs.edit().remove("bg_failed").apply()
        }
        if (needsSignIn) Notifier.refreshSignIn(app)
        return again
    }

    companion object {
        const val SESSION = 7101
        const val CONFIRM = 7102
        const val SEND = 7103
        /** A cap on real tries per entry (offline and unreachable answers); with JobScheduler's
         *  backoff (30 s doubling, at most 5 h) the 24 h age usually comes first. */
        const val MAX_ATTEMPTS = 12
        /** Waiting recordings above this many bytes (about 4 minutes of speech) are sent on an
         *  unmetered network only, unless the user asked with Retry. */
        const val UNMETERED_ABOVE_BYTES = 8_000_000L
        private const val DAY_MS = 24 * 60 * 60_000L
        private const val EXTRA_MANUAL = "manual"

        /** A send loop is running in this process. */
        private val sending = AtomicBoolean(false)
        /** The user tapped Retry while it ran: it does one more pass instead of a second loop. */
        private val retryAsked = AtomicBoolean(false)
        /** Something new waits while it ran: one more plain pass. */
        private val passAsked = AtomicBoolean(false)

        /** Entries the background sends: waiting for the network, and waiting for a sign-in once
         *  the session is good (dismissed ones and ones from earlier runs too). */
        private fun waiting(app: com.voxa.android.VoxaApp): List<ULong> =
            app.core.waitingForNetwork() + if (app.session.value == SessionState.VALID) app.core.waitingForSignInIds() else emptyList()

        private fun ids(s: String?): List<ULong> = s.orEmpty().split(',').mapNotNull { it.trim().toULongOrNull() }

        /** Dictations that couldn't be sent, for the notification's Retry (the job's and the keyboard's). */
        fun addFailed(ctx: Context, entries: List<ULong>) {
            val p = ctx.getSharedPreferences("notify", Context.MODE_PRIVATE)
            synchronized(this) {
                val all = (ids(p.getString("bg_failed", "")) + entries).distinct()
                p.edit().putString("bg_failed", all.joinToString(",")).apply()
            }
        }

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

        /** Send dictations waiting for the network once there is one. `manual`: the user's Retry.
         *  While a send runs, Retry only asks it for one more pass: scheduling the same job id again
         *  would stop the running one mid-upload and start a second. */
        fun scheduleSend(ctx: Context, manual: Boolean = false) {
            val js = scheduler(ctx)
            if (sending.get()) {
                if (manual) retryAsked.set(true) else passAsked.set(true)
                Diag.log("jobs", "background send already running${if (manual) ": retry after it" else ": one more pass after it"}")
                return
            }
            if (!manual && js.getPendingJob(SEND) != null) return
            // A long recording is not sent again and again over mobile data: it waits for Wi-Fi,
            // unless the user asked (Retry).
            val bytes = runCatching { ctx.voxa.core.audioBytesOf(waiting(ctx.voxa)).toLong() }.getOrDefault(0L)
            val network = if (!manual && bytes > UNMETERED_ABOVE_BYTES) JobInfo.NETWORK_TYPE_UNMETERED else JobInfo.NETWORK_TYPE_ANY
            val r = js.schedule(
                JobInfo.Builder(SEND, component(ctx))
                    .setRequiredNetworkType(network)
                    .setEstimatedNetworkBytes(0L, bytes)
                    .setBackoffCriteria(30_000L, JobInfo.BACKOFF_POLICY_EXPONENTIAL)
                    .setPersisted(true)
                    .setExtras(PersistableBundle().apply { putBoolean(EXTRA_MANUAL, manual) })
                    .build(),
            )
            Diag.log("jobs", "background send ${if (manual) "(retry) " else ""}${if (network == JobInfo.NETWORK_TYPE_UNMETERED) "on Wi-Fi only " else ""}${if (r == JobScheduler.RESULT_SUCCESS) "scheduled" else "not scheduled"}")
        }

        /** At start, whenever History changes and after signing in: is anything waiting? */
        fun scheduleSendIfWaiting(ctx: Context) {
            if (waiting(ctx.voxa).isNotEmpty()) scheduleSend(ctx)
        }
    }
}
