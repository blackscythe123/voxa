package com.voxa.android.ui

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.Route
import com.voxa.android.voxa
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import uniffi.voxa_ffi.AppCount
import uniffi.voxa_ffi.EntryStatus
import uniffi.voxa_ffi.HistoryEntry
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter

@Composable
fun HistoryScreen(activity: MainActivity) {
    val c = vx
    val app = activity.voxa
    val version by app.historyVersion.collectAsState()
    var query by remember { mutableStateOf(activity.getSharedPreferences("ui", 0).getString("history_query", "") ?: "") }
    var focused by remember { mutableStateOf(false) }
    var entries by remember { mutableStateOf<List<HistoryEntry>?>(null) }
    var apps by remember { mutableStateOf(emptyList<AppCount>()) }
    var recents by remember { mutableStateOf(RecentSearches.load(activity)) }
    val focus = LocalFocusManager.current
    LaunchedEffect(version, query) { entries = withContext(Dispatchers.IO) { app.core.history(query.trim()) } }
    LaunchedEffect(version) { apps = withContext(Dispatchers.IO) { app.core.appCounts() } }
    val remember = { q: String ->
        if (q.isNotBlank()) {
            RecentSearches.add(activity, q.trim())
            recents = RecentSearches.load(activity)
        }
    }
    val setQuery = { q: String ->
        query = q
        activity.getSharedPreferences("ui", 0).edit().putString("history_query", q).apply()
    }

    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp)) {
        ScreenTitle("History")
        SearchField(query, setQuery, "Search what you said", onFocus = { focused = it }, onSearch = { remember(query); focus.clearFocus() })
        Spacer(Modifier.height(24.dp))
        if (focused && query.isBlank()) {
            SearchSuggestions(recents, apps) { q ->
                setQuery(q)
                remember(q)
                focus.clearFocus()
            }
            return@Column
        }
        val list = entries ?: return@Column
        if (list.isEmpty()) {
            Text(
                if (query.isBlank()) "Nothing yet. What you dictate shows up here, and stays on this phone." else "Nothing matches \"${query.trim()}\".",
                Modifier.fillMaxWidth().padding(top = 24.dp), style = Type.M, color = c.text56, textAlign = TextAlign.Center,
            )
            return@Column
        }
        val groups = list.groupBy { dayOf(it.id) }
        LazyColumn(Modifier.fillMaxSize()) {
            groups.forEach { (day, items) ->
                item(key = "day-$day") {
                    Text(dayLabel(day), Modifier.padding(top = 8.dp, bottom = 4.dp), style = Type.SBold, color = c.text56)
                }
                items(items, key = { it.id.toLong() }) { e ->
                    HistoryItem(e) {
                        remember(query)
                        activity.go(Route.Detail(e.id))
                    }
                }
            }
            item { Spacer(Modifier.height(24.dp)) }
        }
    }
}

/** History search, "Your apps": the owner's cap is 8 (two rows of four), most used first. */
const val YOUR_APPS_CAP = 8

fun yourApps(apps: List<AppCount>): List<List<AppCount>> = apps.take(YOUR_APPS_CAP).chunked(4)

@Composable
private fun SearchSuggestions(recents: List<String>, apps: List<AppCount>, pick: (String) -> Unit) {
    val c = vx
    if (recents.isNotEmpty()) {
        Text("Recent searches", Modifier.padding(bottom = 4.dp), style = Type.SBold, color = c.text)
        Column(Modifier.padding(bottom = 24.dp)) {
            recents.forEach { q ->
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).clickable { pick(q) }.padding(vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(painterResource(R.drawable.ic_history), null, Modifier.size(24.dp), tint = c.text40)
                    Spacer(Modifier.width(12.dp))
                    Text(q, style = Type.M, color = c.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
    if (apps.isNotEmpty()) {
        Text("Your apps", Modifier.padding(bottom = 12.dp), style = Type.SBold, color = c.text)
        yourApps(apps).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 16.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                row.forEach { a ->
                    Column(
                        Modifier.weight(1f).clip(RoundedCornerShape(12.dp)).clickable { pick(a.app) },
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        AppBadge(a.app, 56.dp, 16.dp)
                        Spacer(Modifier.height(8.dp))
                        Text(a.app, style = Type.XS, color = c.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text("${a.count}", style = Type.XS, color = c.text56)
                    }
                }
                repeat(4 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
    if (recents.isEmpty() && apps.isEmpty()) {
        Text("Search the words you said, or an app's name.", style = Type.M, color = c.text56)
    }
}

/** One dictation in a list: app badge, app and time, two lines of text, and its status if not pasted. */
@Composable
fun HistoryItem(e: HistoryEntry, onClick: () -> Unit) {
    val c = vx
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).clickable(onClick = onClick).padding(vertical = 12.dp)) {
        AppBadge(e.app.ifEmpty { "Voxa" })
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row {
                Text(e.app.ifEmpty { "Voxa" }, Modifier.weight(1f), style = Type.SBold, color = c.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.width(8.dp))
                Text(timeOf(e.id), style = Type.S, color = c.text56)
            }
            if (e.text.isNotBlank()) {
                Spacer(Modifier.height(4.dp))
                Text(e.text, style = Type.M.copy(lineHeight = Type.M.fontSize * 1.4f), color = c.text72, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            statusPill(e.status)?.let { (label, kind) ->
                Spacer(Modifier.height(8.dp))
                Pill(label, kind)
            }
        }
    }
}

/** Pasted needs no pill; everything else says what it is. */
fun statusPill(s: EntryStatus): Pair<String, PillKind>? = when (s) {
    is EntryStatus.Pasted -> null
    is EntryStatus.Ready -> "Not pasted" to PillKind.Copy
    is EntryStatus.Waiting -> s.v1 to PillKind.Warn
    is EntryStatus.Failed -> s.v1 to PillKind.Warn
}

fun statusWord(s: EntryStatus): String = when (s) {
    is EntryStatus.Pasted -> "pasted"
    is EntryStatus.Ready -> "not pasted"
    is EntryStatus.Waiting -> s.v1.replaceFirstChar { it.lowercase() }
    is EntryStatus.Failed -> s.v1.replaceFirstChar { it.lowercase() }
}

private fun zoned(id: ULong) = Instant.ofEpochMilli(id.toLong()).atZone(ZoneId.systemDefault())

fun dayOf(id: ULong): LocalDate = zoned(id).toLocalDate()

fun timeOf(id: ULong): String = zoned(id).format(DateTimeFormatter.ofPattern("H:mm"))

fun dayLabel(d: LocalDate): String {
    val today = LocalDate.now()
    return when (d) {
        today -> "Today"
        today.minusDays(1) -> "Yesterday"
        else -> d.format(DateTimeFormatter.ofPattern(if (d.year == today.year) "EEEE d MMMM" else "d MMMM yyyy"))
    }
}

/** "Today at 9:31", "Yesterday at 18:02", "Monday 24 November at 8:12". */
fun whenLabel(id: ULong): String = "${dayLabel(dayOf(id))} at ${timeOf(id)}"

/** The last five searches, kept on this phone only. */
object RecentSearches {
    private const val KEY = "recent_searches"

    fun load(context: Context): List<String> {
        val raw = context.getSharedPreferences("ui", 0).getString(KEY, null) ?: return emptyList()
        return runCatching { JSONArray(raw).let { a -> List(a.length()) { a.getString(it) } } }.getOrDefault(emptyList())
    }

    fun add(context: Context, q: String) {
        val next = (listOf(q) + load(context).filterNot { it.equals(q, ignoreCase = true) }).take(5)
        context.getSharedPreferences("ui", 0).edit().putString(KEY, JSONArray(next).toString()).apply()
    }
}
