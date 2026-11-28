package com.voxa.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.voxa.android.BuildConfig
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.notify.Updater
import com.voxa.android.notify.VoxaJobs
import com.voxa.android.voxa
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter

// The update copy (docs/updates.md). Sentence case, no dashes; buttons carry the version.

/** "Install" was tapped (the notification, the Home card or Settings). */
fun installTapped(activity: MainActivity) {
    when (Updater.tapInstall(activity)) {
        Updater.Tap.INSTALLING -> activity.voxa.scope.launch {
            if (!Updater.install(activity)) message(activity, "Voxa couldn't start the update. It will try again later.")
        }
        Updater.Tap.DICTATION_LIVE -> message(activity, Updater.DICTATION_LIVE_TEXT)
        // Shown once, the first time Install is tapped without the permission.
        Updater.Tap.ASK_PERMISSION -> activity.showSheet {
            UpdateSheet(
                activity, "Let Voxa install its updates",
                "Android asks you once to let Voxa install new versions. On the next screen, turn on the switch for Voxa, then come back.",
                "Continue", { Updater.askedAtTap(activity); activity.openInstallSettings() },
                "Not now", { Updater.prefs(activity).edit().putBoolean("tap_ask_done", true).apply() },
            )
        }
        Updater.Tap.CANT_INSTALL -> cantInstall(activity)
        Updater.Tap.NOT_READY -> checkNow(activity)
    }
}

/** After the one ask: the update can only be downloaded from the release page. */
private fun cantInstall(activity: MainActivity) = activity.showSheet {
    UpdateSheet(activity, "Voxa can't install updates yet. This version keeps working.", null, "Download instead", { Updater.openNotes(activity) }, "Not now", {})
}

private fun message(activity: MainActivity, text: String) = activity.showSheet { UpdateSheet(activity, text, null, "OK", {}, null, {}) }

/** Back from Android's "Install unknown apps" switch after the tap ask. */
fun afterInstallGrant(activity: MainActivity) {
    when (Updater.returnedFromGrant(activity) ?: return) {
        Updater.Tap.CANT_INSTALL -> cantInstall(activity)
        else -> installTapped(activity)
    }
}

/** "Check for updates": check now; an offer downloads at once on Wi-Fi, and asks first on mobile data. */
fun checkNow(activity: MainActivity) {
    activity.voxa.scope.launch {
        val r = Updater.check(activity, manual = true)
        if (r !is Updater.Checked.Offer || Updater.staged(activity) != null) return@launch
        val bytes = r.offer.size.toLong()
        if (!r.metered) return@launch VoxaJobs.scheduleUpdateDownload(activity, bytes, anyNetwork = true)
        val mb = ((bytes + 500_000) / 1_000_000).coerceAtLeast(1)
        activity.showSheet {
            UpdateSheet(
                activity, "Download Voxa ${r.offer.version} using mobile data? It's about $mb MB.", null,
                "Download", { VoxaJobs.scheduleUpdateDownload(activity, bytes, anyNetwork = true) },
                "Wait for Wi-Fi", { VoxaJobs.scheduleUpdateDownload(activity, bytes, anyNetwork = false) },
            )
        }
    }
}

/** One sheet for every update question: a heading first, then the buttons in reading order. */
@Composable
private fun ColumnScope.UpdateSheet(
    activity: MainActivity, title: String, body: String?,
    primary: String, onPrimary: () -> Unit, secondary: String?, onSecondary: () -> Unit,
) {
    val c = vx
    Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 24.dp).navigationBarsPadding()) {
        Text(title, Modifier.semantics { heading() }, style = Type.L, color = c.text)
        if (body != null) {
            Spacer(Modifier.height(8.dp))
            Text(body, style = Type.M, color = c.text72)
        }
        Spacer(Modifier.height(24.dp))
        PrimaryButton(primary, Modifier.fillMaxWidth()) {
            activity.hideSheet()
            onPrimary()
        }
        if (secondary != null) {
            Spacer(Modifier.height(8.dp))
            SecondaryButton(secondary, Modifier.fillMaxWidth()) {
                activity.hideSheet()
                onSecondary()
            }
        }
    }
}

/** Home: "ready to install", "was updated" or "wasn't updated". Nothing when there is nothing to say. */
@Composable
fun UpdateHomeCard(activity: MainActivity) {
    val tick = activity.tick.intValue
    val status by Updater.status.collectAsState()
    val ready = remember(tick, status) { Updater.readyVersion(activity) }
    var updated by remember(tick) { mutableStateOf(Updater.updatedNote(activity)) }
    var notUpdated by remember(tick, status) { mutableStateOf(Updater.notUpdated(activity)) }
    when {
        updated != null -> UpdateCard(
            "Voxa was updated to $updated.", "Your history and settings stay as they are.", "What's new",
            onDismiss = { Updater.dismissUpdatedNote(activity); updated = null },
        ) { Updater.openNotes(activity, "https://github.com/blackscythe123/voxa/releases/tag/android-v$updated") }
        notUpdated && ready != null -> UpdateCard(
            "Voxa wasn't updated. You're still on ${BuildConfig.VERSION_NAME}.",
            "If Play Protect asks to scan Voxa, scanning is fine.", "Try again",
            onDismiss = { Updater.dismissNotUpdated(activity); notUpdated = false },
        ) {
            Updater.dismissNotUpdated(activity)
            installTapped(activity)
        }
        ready != null -> UpdateCard("Voxa $ready is ready to install", "Tap to install. Your history and settings stay as they are.", "Install Voxa $ready", onDismiss = null) {
            installTapped(activity)
        }
    }
}

@Composable
private fun UpdateCard(title: String, body: String, button: String, onDismiss: (() -> Unit)?, onClick: () -> Unit) {
    val c = vx
    VCard(Modifier.padding(bottom = 24.dp), padding = PaddingValues(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconTile(R.drawable.ic_update)
            Spacer(Modifier.width(12.dp))
            Text(title, Modifier.weight(1f).semantics { heading() }, style = Type.MBold, color = c.text)
            if (onDismiss != null) {
                Icon(
                    painterResource(R.drawable.ic_close), "Dismiss",
                    Modifier.size(48.dp).clip(CircleShape).clickable(onClick = onDismiss).padding(14.dp),
                    tint = c.text56,
                )
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(body, style = Type.S, color = c.text72)
        Spacer(Modifier.height(12.dp))
        PrimaryButton(button, Modifier.fillMaxWidth(), onClick = onClick)
    }
}

/** Settings: the update rows, the Version row among them (spec 6.4). */
@Composable
fun UpdateRows(activity: MainActivity) {
    val c = vx
    val tick = activity.tick.intValue
    val status by Updater.status.collectAsState()
    val percent by Updater.progress.collectAsState()
    var auto by remember { mutableStateOf(Updater.auto(activity)) }
    var manual by remember { mutableStateOf(false) }
    val ready = remember(tick, status) { Updater.readyVersion(activity) }
    val canInstall = remember(tick) { Updater.canInstall(activity) }
    val toggle = { on: Boolean ->
        auto = on
        Updater.setAuto(activity, on)
    }
    Column {
        Row(Modifier.fillMaxWidth().clickable { toggle(!auto) }.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            IconTile(R.drawable.ic_update)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text("Download updates automatically", style = Type.M, color = c.text)
                Text("Voxa downloads new versions on Wi-Fi and tells you when one is ready to install.", style = Type.XS, color = c.text56)
            }
            Spacer(Modifier.width(12.dp))
            VSwitch(auto, toggle)
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
    }
    OptionRow(
        R.drawable.ic_refresh, "Check for updates",
        onClick = {
            if (ready != null) installTapped(activity) else if (status != Updater.Status.CHECKING) {
                manual = true
                checkNow(activity)
            }
        },
    ) { OptionValue(Updater.rowValue(status, ready, percent)) }
    // The sentence after a manual check: a polite live region, so TalkBack reads the result once.
    if (manual) {
        val line = when (status) {
            Updater.Status.UP_TO_DATE -> "You have the latest version. Last checked ${lastChecked(Updater.lastCheckMs(activity))}."
            else -> Updater.statusLine(status, Updater.offer(activity)?.version, ((Updater.offer(activity)?.size?.toLong() ?: 0L) / 1_000_000 + 1) * 2)
        }
        if (line != null && status != Updater.Status.DOWNLOADING) {
            Text(line, Modifier.padding(bottom = 12.dp).semantics { liveRegion = LiveRegionMode.Polite }, style = Type.S, color = c.text72)
        }
    }
    OptionRow(R.drawable.ic_info, "Version") { OptionValue(BuildConfig.VERSION_NAME) }
    OptionRow(R.drawable.ic_flag, "What's new", onClick = { Updater.openNotes(activity) }) {
        Icon(painterResource(R.drawable.ic_chevron), null, Modifier.size(20.dp), tint = c.text40)
    }
    // User initiated, so not a nag: opens Android's own switch.
    OptionRow(R.drawable.ic_lock, "Allow Voxa to install updates", last = true, onClick = { activity.openInstallSettings() }) {
        OptionValue(if (canInstall) "On" else "Off")
    }
}

/** "today at 14:05", or the date. */
private fun lastChecked(ms: Long): String {
    if (ms <= 0) return "just now"
    val t = Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault())
    val time = t.format(DateTimeFormatter.ofPattern("HH:mm"))
    return if (t.toLocalDate() == LocalDate.now()) "today at $time" else "${t.format(DateTimeFormatter.ofPattern("d MMM"))} at $time"
}

/** Onboarding art: the update arrow in its rings, checked once allowed. */
@Composable
fun UpdateArt(granted: Boolean) {
    val c = vx
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(64.dp).clip(CircleShape).background(if (granted) c.good else c.accent), contentAlignment = Alignment.Center) {
            Icon(painterResource(if (granted) R.drawable.ic_check else R.drawable.ic_update), null, Modifier.size(28.dp), tint = c.onAccent)
        }
        Spacer(Modifier.height(16.dp))
        Text(if (granted) "Updates allowed" else "Updates", style = Type.SBold, color = c.text)
    }
}
