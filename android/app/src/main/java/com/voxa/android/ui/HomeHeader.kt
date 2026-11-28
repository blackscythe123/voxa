package com.voxa.android.ui

import android.content.Context
import android.provider.Settings
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.background
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.voxa.android.BuildConfig
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Route
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.voxa_ffi.AccountInfo
import uniffi.voxa_ffi.HomeGreeting
import uniffi.voxa_ffi.SessionState
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId

/** The clock Home greets by. Debug builds can pin it for screenshots:
 *  `adb shell am start -n com.voxa.android/.MainActivity --es debug_now 2026-12-02T13:05` ("" clears). */
object GreetClock {
    fun now(context: Context): LocalDateTime {
        if (BuildConfig.DEBUG) {
            val pinned = context.getSharedPreferences("debug", Context.MODE_PRIVATE).getString("now", null)
            if (!pinned.isNullOrEmpty()) runCatching { return LocalDateTime.parse(pinned) }
        }
        return LocalDateTime.now()
    }

    /** Debug builds: `--es debug_name Alexandria` stands in for the account's first name. */
    fun debugName(context: Context): String? =
        if (BuildConfig.DEBUG) context.getSharedPreferences("debug", Context.MODE_PRIVATE).getString("name", null)?.takeIf { it.isNotEmpty() } else null
}

/** The system's "Remove animations" (animator duration scale 0). */
fun reducedMotion(context: Context): Boolean =
    Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f

/** Everything wrong right now, the one that blocks dictation most first:
 *  signed out, then the microphone, then the keyboard, then offline. */
@Composable
fun rememberProblems(activity: MainActivity): List<Problem> {
    val app = activity.voxa
    val tick = activity.tick.intValue
    val session by app.session.collectAsState()
    val online by app.online.collectAsState()
    val blocked by app.blocked.collectAsState()
    val mic = remember(tick) { app.micGranted() }
    val enabled = remember(tick) { app.keyboardEnabled() }
    return buildList {
        if (session == SessionState.EXPIRED) add(Problem.SignedOut)
        if (!mic) add(Problem.MicDenied)
        if (!enabled) add(Problem.KeyboardOff)
        if (!online) add(Problem.Offline)
        if (blocked && online) add(Problem.Blocked)
    }
}

private const val PHONE_HORIZON = true

private val Greet40 = Type.Title.copy(fontSize = 40.sp, lineHeight = 44.sp, letterSpacing = (-0.03).em)
private val Greet32 = Type.Title.copy(fontSize = 32.sp, lineHeight = 38.sp, letterSpacing = (-0.03).em)

/**
 * Home's header (docs/ui-mocks/v7-home-greeting-and-splash.html; layout locked in
 * v7-home-fullscreen-compare.html, "Phone, new", doodle above, bigger): the doodle cropped to its
 * drawing above the greeting, the avatar in its own clear corner, the greeting at 40 sp, and one
 * notice only while something is wrong.
 */
@Composable
fun HomeHeader(activity: MainActivity, account: AccountInfo?, problems: List<Problem>) {
    val c = vx
    val context = LocalContext.current
    val app = activity.voxa
    val greeting by produceState<HomeGreeting?>(null) {
        value = withContext(Dispatchers.IO) {
            val now = GreetClock.now(context)
            val zone = ZoneId.systemDefault()
            val offset = zone.rules.getOffset(Instant.now()).totalSeconds / 60
            val bday = Birthday.get(context)
            val installed = runCatching {
                val t = context.packageManager.getPackageInfo(context.packageName, 0).firstInstallTime
                Instant.ofEpochMilli(t).atZone(zone).toLocalDate().toString()
            }.getOrNull()
            val prefs = context.getSharedPreferences("ui", Context.MODE_PRIVATE)
            val firstOpen = prefs.getBoolean("greet_first_open", false)
            if (firstOpen) prefs.edit().remove("greet_first_open").apply()
            app.core.homeGreeting(
                now.toLocalDate().toString(), (now.hour * 60 + now.minute).toUInt(), offset, null,
                (bday?.first ?: 0).toUByte(), (bday?.second ?: 0).toUByte(), installed, firstOpen,
            )
        }
    }
    val reduced = remember { reducedMotion(context) }
    val first = account?.name?.trim()?.split(' ')?.firstOrNull()?.takeIf { it.isNotEmpty() } ?: remember { GreetClock.debugName(context) }

    // The doodle above the greeting, cropped to its own drawing and as big as the width allows up
    // to 150 dp, clear of the avatar's corner (DoodleLayout); the greeting right under it.
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val width = maxWidth.value
        val g = greeting
        val crop = remember(g) { DoodleLayout.crop(g?.bounds ?: emptyList()) }
        val box = remember(g, width) { g?.let { DoodleLayout.place(width, crop[2] - crop[0], crop[3] - crop[1]) } }
        val avatar = DoodleLayout.avatar(width)
        Box(Modifier.fillMaxWidth().height(DoodleLayout.rowHeight(width, box).dp)) {
            if (g != null && box != null) {
                DoodleBand(
                    // The drawing starts once the splash has gone.
                    g, play = g.play && !activity.splashShowing, still = reduced || problems.isNotEmpty(),
                    onPlayed = { app.core.doodlePlayed(g.doodle, g.date) },
                    modifier = Modifier.offset(box.x.dp, box.y.dp).size(box.w.dp, box.h.dp).clipToBounds(),
                    crop = crop, horizon = PHONE_HORIZON,
                )
            }
            Box(Modifier.offset(avatar.x.dp, avatar.y.dp).clip(CircleShape).clickable { activity.go(Route.Tabbed(Tab.Settings)) }) {
                Avatar(account?.name?.ifEmpty { account?.email } ?: "", size = DoodleLayout.AVATAR.dp, image = account?.image ?: "")
            }
        }
    }
    Spacer(Modifier.height(DoodleLayout.GREETING_GAP.dp))
    GreetingLine(greeting?.lead ?: "", first)
    problems.firstOrNull()?.let { p ->
        Spacer(Modifier.height(16.dp))
        Notice(p, problems.size - 1) { fix(activity, p) }
    }
}

/** The greeting's text and whether it is at 40 sp (else 32 sp); [fits40] measures one line at
 *  40 sp. Owner decision: one line when it fits. */
fun greetingLayout(lead: String, name: String?, fits40: (String) -> Boolean): Pair<String, Boolean> {
    val full = if (name != null) "$lead, $name" else lead
    return when {
        fits40(full) -> full to true
        name == null -> lead to false
        fits40("$lead,") && fits40(name) -> "$lead,\n$name" to true
        else -> "$lead,\n$name" to false
    }
}

/** The greeting on one line when it fits at 40 sp; otherwise it breaks before the name, never
 *  inside the greeting words; if a line still doesn't fit, 32 sp, never smaller. */
@Composable
fun GreetingLine(lead: String, name: String?) {
    val c = vx
    val measurer = rememberTextMeasurer()
    BoxWithConstraints(Modifier.fillMaxWidth().heightIn(min = 44.dp)) {
        if (lead.isEmpty()) return@BoxWithConstraints
        val max = constraints.maxWidth
        fun fits(text: String, style: androidx.compose.ui.text.TextStyle) =
            measurer.measure(text, style, maxLines = 1, softWrap = false).size.width <= max
        val (text, big) = greetingLayout(lead, name) { fits(it, Greet40) }
        val style = if (big) Greet40 else Greet32
        Text(text, Modifier.semantics { heading() }, style = style, color = c.text, softWrap = false, maxLines = 2)
    }
}

private enum class Tone { Warn, Bad, Calm }

private data class NoticeText(val tone: Tone, val icon: Int, val title: String, val sub: String, val action: String?)

private fun noticeOf(p: Problem) = when (p) {
    Problem.SignedOut -> NoticeText(Tone.Warn, R.drawable.ic_you, "Signed out of ChatGPT", "Voxa needs it to write", "Sign in")
    Problem.MicDenied -> NoticeText(Tone.Bad, R.drawable.ic_mic_off, "Microphone is blocked", "Voxa can't hear you", "Allow")
    Problem.KeyboardOff -> NoticeText(Tone.Warn, R.drawable.ic_keyboard, "Voxa keyboard is off", "Turn it on to use it in any app", "Turn on")
    Problem.Offline -> NoticeText(Tone.Calm, R.drawable.ic_cloud_off, "You're offline", "Dictation comes back with the internet", null)
    Problem.Blocked -> NoticeText(Tone.Warn, R.drawable.ic_cloud_off, "Couldn't reach ChatGPT", "Voxa will try again", "Retry")
}

/** One compact notice under the greeting; the whole row is the button. "1 more to fix" when there
 *  is more than one problem. */
@Composable
private fun Notice(p: Problem, more: Int, onFix: () -> Unit) {
    val c = vx
    val n = noticeOf(p)
    val tint = when (n.tone) {
        Tone.Warn -> c.warnText
        Tone.Bad -> c.dangerText
        Tone.Calm -> c.text56
    }
    val sub = if (more > 0) "${n.sub}. $more more to fix" else n.sub
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(16.dp)).background(c.surface)
            .then(if (n.action != null || more > 0) Modifier.clickable(role = Role.Button, onClick = onFix) else Modifier)
            .padding(start = 16.dp, end = 12.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(n.icon), null, Modifier.size(20.dp), tint = tint)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(n.title, style = Type.SBold, color = c.text)
            Text(sub, Modifier.padding(top = 2.dp), style = Type.XS, color = c.text72)
        }
        if (n.action != null) {
            Row(Modifier.padding(start = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(n.action, style = Type.SBold, color = c.accent)
                Spacer(Modifier.width(4.dp))
                Icon(painterResource(R.drawable.ic_chevron), null, Modifier.size(16.dp), tint = c.accent)
            }
        }
    }
}
