package com.voxa.android.notify

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Uri
import android.os.Build
import com.voxa.android.BuildConfig
import com.voxa.android.diag.Diag
import com.voxa.android.voxa
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.FlowState
import uniffi.voxa_ffi.UpdateCheck
import uniffi.voxa_ffi.UpdateFailure
import uniffi.voxa_ffi.UpdateOffer
import uniffi.voxa_ffi.updateCheck
import uniffi.voxa_ffi.updateDownload
import uniffi.voxa_ffi.updateVerifyFile

/**
 * Voxa's self update on Android (docs/updates.md): check the signed feed, download on Wi-Fi,
 * then one tap installs through PackageInstaller. Everything that needs trust (the signature, the
 * version rule, the size cap, the hash) is in the shared Rust core (`voxa-update`); this is the
 * platform glue. The only network peers are github.com and its release asset hosts.
 *
 * State lives in SharedPreferences("update"), always on (also in release builds): the last check,
 * its result, the offer, the staged file and the last PackageInstaller status. Never user data.
 */
object Updater {
    const val PREFS = "update"
    const val ACTION_INSTALL_STATUS = "com.voxa.android.UPDATE_INSTALL_STATUS"

    /** What the Settings row and the Home card say. */
    enum class Status { NONE, CHECKING, UP_TO_DATE, DOWNLOADING, READY, OFFLINE, FAILED, REFUSED, NO_SPACE }

    /** The live status, so open screens follow a check or a download. */
    val status = MutableStateFlow(Status.NONE)
    /** Percent of the download, while downloading (never announced). */
    val progress = MutableStateFlow(0)

    fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // ── settings ─────────────────────────────────────────────────────────────

    /** "Download updates automatically" (default on). */
    fun auto(ctx: Context) = prefs(ctx).getBoolean("auto", true)

    fun setAuto(ctx: Context, on: Boolean) {
        prefs(ctx).edit().putBoolean("auto", on).apply()
        Diag.log("update", "download updates automatically: ${if (on) "on" else "off"}")
        if (on) VoxaJobs.scheduleUpdateCheck(ctx) else VoxaJobs.cancelUpdateJobs(ctx)
    }

    /** Android's "Install unknown apps" switch for Voxa, read live. */
    fun canInstall(ctx: Context): Boolean = (if (BuildConfig.DEBUG) installAllowedForTest else null) ?: ctx.packageManager.canRequestPackageInstalls()

    /** Debug builds only, for instrumented tests: Android kills the app's process when "Install
     *  unknown apps" is revoked, so tests can't flip the real switch back and forth inside one
     *  run. The real switch is driven with adb on the emulator (docs/updates.md). */
    @Volatile var installAllowedForTest: Boolean? = null

    fun loadStatus(ctx: Context): Status =
        runCatching { Status.valueOf(prefs(ctx).getString("result", Status.NONE.name) ?: "NONE") }.getOrDefault(Status.NONE)

    private fun record(ctx: Context, s: Status) {
        status.value = s
        prefs(ctx).edit().putString("result", s.name).apply()
    }

    fun lastCheckMs(ctx: Context) = prefs(ctx).getLong("last_check_ms", 0L)

    // ── the offer ────────────────────────────────────────────────────────────

    fun offer(ctx: Context): UpdateOffer? {
        val p = prefs(ctx)
        val v = p.getString("offer_version", null) ?: return null
        val code = p.getLong("offer_code", 0L)
        // An offer for this version or older (installed meanwhile) is no offer.
        if (code <= BuildConfig.VERSION_CODE) return null
        return UpdateOffer(
            v, code.toULong(), p.getString("offer_abi", "").orEmpty(), p.getString("offer_url", "").orEmpty(),
            p.getString("offer_sha256", "").orEmpty(), p.getLong("offer_size", 0L).toULong(), p.getString("offer_notes", "").orEmpty(),
            p.getLong("offer_serial", 0L).toULong(), p.getString("offer_signer", "").orEmpty(), p.getBoolean("offer_auto", true),
        )
    }

    private fun saveOffer(ctx: Context, o: UpdateOffer) {
        prefs(ctx).edit()
            .putString("offer_version", o.version).putLong("offer_code", o.versionCode.toLong()).putString("offer_abi", o.abi)
            .putString("offer_url", o.url).putString("offer_sha256", o.sha256).putLong("offer_size", o.size.toLong())
            .putString("offer_notes", o.notesUrl).putLong("offer_serial", o.serial.toLong()).putString("offer_signer", o.signerSha256)
            .putBoolean("offer_auto", o.auto).apply()
    }

    private fun clearOffer(ctx: Context) {
        prefs(ctx).edit().apply { for (k in listOf("version", "code", "abi", "url", "sha256", "size", "notes", "serial", "signer", "auto")) remove("offer_$k") }
            .remove("staged_path").apply()
    }

    /** The verified, staged file for the current offer, if it is still there. */
    fun staged(ctx: Context): File? {
        val path = prefs(ctx).getString("staged_path", null) ?: return null
        return File(path).takeIf { it.isFile && offer(ctx) != null }
    }

    fun updatesDir(ctx: Context) = File(ctx.filesDir, "updates")

    /** "Voxa 1.1.0 is ready": an offer whose file is downloaded and verified. */
    fun readyVersion(ctx: Context): String? = offer(ctx)?.takeIf { staged(ctx) != null }?.version

    // ── debug only: a LOCAL test feed (release builds ignore it) ─────────────

    /** Debug builds only: `--es debug_update_feed http://10.0.2.2:8765/feed/android.json
     *  --es debug_update_key <64 hex>` (MainActivity) points the updater at a local test feed
     *  signed with a TEST key. Release builds never read it (BuildConfig.DEBUG is false, so R8
     *  removes the branch). */
    private fun testFeed(ctx: Context): Pair<String, String>? {
        if (!BuildConfig.DEBUG) return null
        val d = ctx.getSharedPreferences("debug", Context.MODE_PRIVATE)
        val url = d.getString("update_feed", null)?.takeIf { it.isNotBlank() } ?: return null
        val key = d.getString("update_key", null)?.takeIf { it.isNotBlank() } ?: return null
        return url to key
    }

    // ── check ────────────────────────────────────────────────────────────────

    /** The result of [check], for the caller's next step. */
    sealed interface Checked {
        data object UpToDate : Checked
        data class Offer(val offer: UpdateOffer, val metered: Boolean) : Checked
        data class Failed(val refused: Boolean, val offline: Boolean) : Checked
        /** Nothing went to the network (not due, or a manual check within a minute). */
        data object Skipped : Checked
    }

    /**
     * Check the feed (network, so call it off the main thread). `manual`: the user pressed "Check
     * for updates". A background check with "Download updates automatically" on schedules the
     * Wi-Fi download itself; a manual one returns the offer so the screen can ask about mobile data.
     */
    suspend fun check(ctx: Context, manual: Boolean): Checked = withContext(Dispatchers.IO) {
        val p = prefs(ctx)
        val before = loadStatus(ctx)
        status.value = Status.CHECKING
        val now = System.currentTimeMillis()
        val test = testFeed(ctx)
        // The core decides whether this goes to the network at all (one automatic check a day at
        // this install's own time, manual at most once a minute, ETag 304s): `state` is its memory.
        val r: UpdateCheck = updateCheck(
            Build.SUPPORTED_ABIS.toList(), BuildConfig.VERSION_CODE.toULong(), p.getString("check_state", "").orEmpty(),
            now.toULong(), manual, test?.first, test?.second,
        )
        p.edit().putString("check_state", r.state).apply()
        if (!r.checked) {
            // Not due (or asked again within a minute): the last answer stands.
            record(ctx, if (before == Status.CHECKING) Status.NONE else before)
            return@withContext Checked.Skipped
        }
        p.edit().putLong("last_check_ms", now).apply()
        if (r.failure.isNotEmpty()) {
            val refused = r.failure == "refused"
            val offline = r.failure == "offline" && !online(ctx)
            Diag.log("update", "check: ${r.failure}${if (test != null) " (test feed)" else ""}")
            // A background refusal changes only the status line; the refused copy is for a manual check.
            record(ctx, when {
                refused && manual -> Status.REFUSED
                offline -> Status.OFFLINE
                else -> Status.FAILED
            })
            return@withContext Checked.Failed(refused, offline)
        }
        val o = r.offer
        if (o == null) {
            Diag.log("update", "check: up to date${if (r.paused) " (feed paused)" else ""}")
            clearOffer(ctx)
            cleanUpdates(ctx)
            record(ctx, Status.UP_TO_DATE)
            return@withContext Checked.UpToDate
        }
        // A signer that is not this app's is never offered as a dead "Tap to install". The feed's
        // signer is compared first; a newer key (a v3 lineage rotation) is accepted once the
        // downloaded file's own lineage shows this app's certificate (see [verifiedArchive]).
        val feedSignerOk = o.signerSha256.isEmpty() || o.signerSha256.lowercase() in ownSigners(ctx)
        Diag.log("update", "check: ${o.version} (code ${o.versionCode}, ${o.abi}) offered${if (!feedSignerOk) ", new signer: checked after download" else ""}")
        val known = offer(ctx)
        if (known?.versionCode != o.versionCode || known.sha256 != o.sha256) {
            clearOffer(ctx)
            saveOffer(ctx, o)
        }
        val metered = metered(ctx)
        if (staged(ctx) != null) {
            record(ctx, Status.READY)
        } else {
            record(ctx, if (loadStatus(ctx) == Status.DOWNLOADING) Status.DOWNLOADING else Status.NONE)
            if (!manual && auto(ctx)) VoxaJobs.scheduleUpdateDownload(ctx, o.size.toLong(), anyNetwork = false)
        }
        Checked.Offer(o, metered)
    }

    // ── download ─────────────────────────────────────────────────────────────

    /** Download the current offer (the 7105 job, network). True when it is staged and verified;
     *  false to try again later (the job's backoff). Posts A4 when it is ready. */
    suspend fun download(ctx: Context): Boolean = withContext(Dispatchers.IO) {
        val o = offer(ctx) ?: return@withContext true
        if (staged(ctx) != null) {
            Notifier.updateReady(ctx, o.version)
            return@withContext true
        }
        val dir = updatesDir(ctx).apply { mkdirs() }
        record(ctx, Status.DOWNLOADING)
        progress.value = 0
        val start = System.currentTimeMillis()
        val watcher = Thread {
            // The core streams into a .part file; its size is the progress.
            while (status.value == Status.DOWNLOADING) {
                val part = dir.listFiles()?.filter { it.name.startsWith(".part-") }?.maxOfOrNull { it.length() } ?: 0L
                if (o.size > 0u) progress.value = (part * 100 / o.size.toLong()).toInt().coerceIn(0, 99)
                Thread.sleep(500)
            }
        }.apply { isDaemon = true; start() }
        val test = testFeed(ctx)
        val path = try {
            updateDownload(o, dir.path, test?.first)
        } catch (e: UpdateFailure) {
            Diag.log("update", "download: ${e.javaClass.simpleName}")
            record(ctx, if (e is UpdateFailure.NoSpace) Status.NO_SPACE else if (e is UpdateFailure.Offline) Status.OFFLINE else Status.FAILED)
            watcher.interrupt()
            // Offline: the job's backoff tries again. A wrong file or no space: the next daily check.
            return@withContext e !is UpdateFailure.Offline
        }
        val file = File(path)
        if (!verifiedArchive(ctx, file, o)) {
            file.delete()
            clearOffer(ctx)
            record(ctx, Status.FAILED)
            Diag.log("update", "download: the file is not a Voxa update for this phone (signer or package), deleted")
            return@withContext true
        }
        prefs(ctx).edit().putString("staged_path", file.path).apply()
        progress.value = 100
        record(ctx, Status.READY)
        Diag.log("update", "download: ${o.version} staged, ${o.size} bytes in ${System.currentTimeMillis() - start} ms")
        Notifier.updateReady(ctx, o.version)
        true
    }

    /** The downloaded file is Voxa (same package), the offered versionCode, and signed by this
     *  app's certificate or a newer one whose lineage includes it (an APK v3 rotation). */
    private fun verifiedArchive(ctx: Context, file: File, o: UpdateOffer): Boolean {
        val pm = ctx.packageManager
        val info = runCatching { pm.getPackageArchiveInfo(file.path, PackageManager.GET_SIGNING_CERTIFICATES) }.getOrNull() ?: return false
        if (info.packageName != ctx.packageName) return false
        if (info.longVersionCode != o.versionCode.toLong()) return false
        val mine = ownSigners(ctx)
        val signing = info.signingInfo
        // Some Android versions don't read an archive's signers: then the feed's signer decides.
        if (signing == null) return o.signerSha256.isEmpty() || o.signerSha256.lowercase() in mine
        val theirs = (if (signing.hasMultipleSigners()) signing.apkContentsSigners else signing.signingCertificateHistory).map { sha256(it.toByteArray()) }
        return theirs.any { it in mine }
    }

    /** SHA-256 of this app's signing certificates, the whole lineage (lowercase hex). */
    fun ownSigners(ctx: Context): Set<String> {
        val info = runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, PackageManager.GET_SIGNING_CERTIFICATES) }.getOrNull() ?: return emptySet()
        val s = info.signingInfo ?: return emptySet()
        val certs = if (s.hasMultipleSigners()) s.apkContentsSigners else s.signingCertificateHistory
        return certs.map { sha256(it.toByteArray()) }.toSet()
    }

    private fun sha256(b: ByteArray) = MessageDigest.getInstance("SHA-256").digest(b).joinToString("") { "%02x".format(it) }

    /** Delete everything in filesDir/updates except the staged file. */
    fun cleanUpdates(ctx: Context, keepStaged: Boolean = false) {
        val keep = if (keepStaged) staged(ctx) else null
        updatesDir(ctx).listFiles()?.forEach { if (it != keep) it.delete() }
    }

    private fun online(ctx: Context): Boolean {
        val cm = ctx.getSystemService(ConnectivityManager::class.java)
        return cm.activeNetwork != null
    }

    fun metered(ctx: Context): Boolean = ctx.getSystemService(ConnectivityManager::class.java).isActiveNetworkMetered

    // ── install ──────────────────────────────────────────────────────────────

    /** A keyboard dictation is recording, writing out or waiting on the user: an install would
     *  kill it (the keyboard shares this process). */
    fun dictationLive(ctx: Context): Boolean {
        val s = runCatching { ctx.voxa.keyboardFlow?.state() }.getOrNull() ?: return false
        return s is FlowState.Recording || s is FlowState.Transcribing || s is FlowState.Delivering || s is FlowState.NeedsYou
    }

    /** What a tap on "Install" leads to (the screen shows the matching sheet). */
    enum class Tap { INSTALLING, DICTATION_LIVE, ASK_PERMISSION, CANT_INSTALL, NOT_READY }

    /** The user tapped Install (notification, Home card or Settings). Never installs while a
     *  dictation is live; asks for "Install unknown apps" once, then only offers the download. */
    fun tapInstall(ctx: Context): Tap {
        val o = offer(ctx)
        val file = staged(ctx)
        if (o == null || file == null) return Tap.NOT_READY
        if (dictationLive(ctx)) {
            Diag.log("update", "install refused: a dictation is live")
            return Tap.DICTATION_LIVE
        }
        if (!canInstall(ctx)) {
            val p = prefs(ctx)
            return if (!p.getBoolean("tap_ask_done", false)) Tap.ASK_PERMISSION else Tap.CANT_INSTALL
        }
        return Tap.INSTALLING
    }

    /** The one later ask was answered with Continue: remember it, then open Android's switch. */
    fun askedAtTap(ctx: Context) {
        prefs(ctx).edit().putBoolean("tap_ask_done", true).putBoolean("await_grant", true).apply()
    }

    /** Back from Android's switch after the tap ask: install now, or say it can't yet. */
    fun returnedFromGrant(ctx: Context): Tap? {
        val p = prefs(ctx)
        if (!p.getBoolean("await_grant", false)) return null
        p.edit().remove("await_grant").apply()
        return if (canInstall(ctx)) tapInstall(ctx) else Tap.CANT_INSTALL
    }

    /** Verify the staged file again (it sits in app storage, but check right before use), then
     *  hand it to PackageInstaller. Off the main thread. */
    suspend fun install(ctx: Context): Boolean = withContext(Dispatchers.IO) {
        val o = offer(ctx) ?: return@withContext false
        val file = staged(ctx) ?: return@withContext false
        try {
            updateVerifyFile(file.path, o)
        } catch (e: UpdateFailure) {
            Diag.log("update", "install: the staged file failed its check (${e.javaClass.simpleName}), deleted")
            file.delete()
            prefs(ctx).edit().remove("staged_path").apply()
            record(ctx, Status.FAILED)
            VoxaJobs.scheduleUpdateDownload(ctx, o.size.toLong(), anyNetwork = false)
            return@withContext false
        }
        commitInstall(ctx, file, o)
    }

    /** The PackageInstaller session. The system shows its own confirmation (STATUS_PENDING_USER_ACTION,
     *  handled in [UpdateReceiver]); every status is logged there. */
    private fun commitInstall(ctx: Context, file: File, o: UpdateOffer): Boolean {
        // Checked again right before the commit: a take may have started since the tap.
        if (dictationLive(ctx)) {
            Diag.log("update", "install refused at commit: a dictation is live")
            return false
        }
        val pi = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(ctx.packageName)
            setSize(file.length())
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED)
        }
        val id = runCatching { pi.createSession(params) }.getOrElse {
            Diag.log("update", "install: no session (${it.javaClass.simpleName})")
            return false
        }
        return runCatching {
            pi.openSession(id).use { session ->
                session.openWrite("voxa.apk", 0, file.length()).use { out ->
                    file.inputStream().use { it.copyTo(out) }
                    session.fsync(out)
                }
                val status = Intent(ctx, UpdateReceiver::class.java).setAction(ACTION_INSTALL_STATUS).setPackage(ctx.packageName)
                // Mutable: PackageInstaller fills in the status extras.
                val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
                val pending = PendingIntent.getBroadcast(ctx, id, status, flags)
                prefs(ctx).edit().putLong("install_from_code", BuildConfig.VERSION_CODE.toLong()).putString("install_target", o.version)
                    .putLong("install_started_ms", System.currentTimeMillis()).apply()
                Diag.log("update", "install: session $id committed for ${o.version}")
                session.commit(pending.intentSender)
            }
            true
        }.getOrElse {
            Diag.log("update", "install: session failed (${it.javaClass.simpleName})")
            runCatching { pi.abandonSession(id) }
            false
        }
    }

    /** The release page, for "What's new" and "Download instead". */
    fun notesUrl(ctx: Context): String =
        offer(ctx)?.notesUrl?.takeIf { it.isNotEmpty() } ?: "https://github.com/blackscythe123/voxa/releases/tag/android-v${BuildConfig.VERSION_NAME}"

    fun openNotes(ctx: Context, url: String = notesUrl(ctx)) {
        runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }

    // ── after an install attempt ─────────────────────────────────────────────

    /** PackageInstaller's answer (UpdateReceiver). */
    fun installStatus(ctx: Context, code: Int, message: String?) {
        prefs(ctx).edit().putInt("installer_status", code).apply()
        Diag.log("update", "installer status $code${message?.let { ": ${it.take(80)}" } ?: ""}")
        if (code != PackageInstaller.STATUS_PENDING_USER_ACTION && code != PackageInstaller.STATUS_SUCCESS) {
            prefs(ctx).edit().putBoolean("not_updated", true).apply()
        }
    }

    /** MY_PACKAGE_REPLACED: the new version runs. Clean up and remember to say so on Home. */
    fun replaced(ctx: Context) {
        val p = prefs(ctx)
        cleanUpdates(ctx)
        clearOffer(ctx)
        p.edit().putString("updated_to", BuildConfig.VERSION_NAME).remove("not_updated").remove("install_target")
            .putString("result", Status.UP_TO_DATE.name).apply()
        status.value = Status.UP_TO_DATE
        Diag.log("update", "updated to ${BuildConfig.VERSION_NAME}")
    }

    /** Home: "Voxa was updated to X" once after an update (null when there is nothing to say). */
    fun updatedNote(ctx: Context): String? = prefs(ctx).getString("updated_to", null)

    fun dismissUpdatedNote(ctx: Context) {
        prefs(ctx).edit().remove("updated_to").apply()
    }

    /** Home: an install was tried and the version is unchanged (Cancel, Play Protect). */
    fun notUpdated(ctx: Context): Boolean {
        val p = prefs(ctx)
        val tried = p.getString("install_target", null) != null
        val same = p.getLong("install_from_code", -1L) == BuildConfig.VERSION_CODE.toLong()
        return tried && same && p.getBoolean("not_updated", false)
    }

    fun dismissNotUpdated(ctx: Context) {
        prefs(ctx).edit().remove("not_updated").remove("install_target").apply()
    }

    /** At app start: the status record, and A4 again if the cap held it back. */
    fun start(ctx: Context) {
        status.value = if (readyVersion(ctx) != null) Status.READY else loadStatus(ctx).let { if (it == Status.DOWNLOADING || it == Status.CHECKING) Status.NONE else it }
        if (auto(ctx)) VoxaJobs.scheduleUpdateCheck(ctx)
        readyVersion(ctx)?.let { Notifier.updateReady(ctx, it) }
    }

    // ── words (docs/updates.md copy deck) ─────────────────────────────────────

    /** The Settings row's value. */
    fun rowValue(s: Status, ready: String?, percent: Int): String = when (s) {
        Status.CHECKING -> "Checking..."
        Status.DOWNLOADING -> "Downloading $percent%"
        Status.READY -> if (ready != null) "$ready ready" else "Up to date"
        Status.OFFLINE, Status.FAILED, Status.REFUSED, Status.NO_SPACE -> if (ready != null) "$ready ready" else "Couldn't check"
        Status.UP_TO_DATE -> "Up to date"
        Status.NONE -> if (ready != null) "$ready ready" else ""
    }

    /** The sentence under the row after a manual check (a polite live region). */
    fun statusLine(s: Status, version: String?, sizeMb: Long): String? = when (s) {
        Status.CHECKING -> "Checking for updates..."
        Status.UP_TO_DATE -> "You have the latest version."
        Status.DOWNLOADING -> "Downloading Voxa ${version.orEmpty()}..."
        Status.READY -> version?.let { "Voxa $it is ready to install." }
        Status.OFFLINE -> "Couldn't check for updates because you're offline. Voxa will try again later."
        Status.FAILED -> "Couldn't check for updates right now. Voxa will try again later."
        Status.REFUSED -> "Voxa didn't install ${version ?: "the update"} because it couldn't confirm it came from Voxa. Your current version keeps working."
        Status.NO_SPACE -> "Not enough space to update. Voxa needs about $sizeMb MB free."
        Status.NONE -> null
    }

    const val DICTATION_LIVE_TEXT = "Finish your dictation first, then tap Install again."
}
