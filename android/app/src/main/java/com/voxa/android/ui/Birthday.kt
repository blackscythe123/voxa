package com.voxa.android.ui

import android.content.Context
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.snapping.rememberSnapFlingBehavior
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.notify.BirthdayWish
import kotlinx.coroutines.launch

private val MONTHS = listOf("January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December")

/** The user's birthday: day and month only, never the year, kept in this phone's private app
 *  storage (backups are off for the app) and never sent anywhere. */
object Birthday {
    private const val PREFS = "ui"

    /** (month 1..12, day 1..31), or null when not added. */
    fun get(context: Context): Pair<Int, Int>? {
        val p = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val m = p.getInt("birthday_month", 0)
        val d = p.getInt("birthday_day", 0)
        return if (m in 1..12 && d in 1..daysIn(m)) m to d else null
    }

    /** Saved, then the midnight wish's alarm is set for it (see notify/BirthdayWish.kt). */
    fun set(context: Context, month: Int, day: Int) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putInt("birthday_month", month).putInt("birthday_day", day).apply()
        BirthdayWish.schedule(context)
    }

    fun clear(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().remove("birthday_month").remove("birthday_day").apply()
        BirthdayWish.schedule(context)
    }

    /** 29 February is allowed (it is celebrated on the 28th in other years). */
    fun daysIn(month: Int) = when (month) {
        2 -> 29
        4, 6, 9, 11 -> 30
        else -> 31
    }

    fun label(month: Int, day: Int) = "$day ${MONTHS[month - 1]}"
}

/** Settings: "Your birthday", optional. Opens the day and month picker. */
@Composable
fun BirthdayCard(activity: MainActivity) {
    val c = vx
    val context = LocalContext.current
    var value by remember { mutableStateOf(Birthday.get(context)) }
    var wish by remember { mutableStateOf(BirthdayWish.enabled(context)) }
    val open = {
        activity.showSheet {
            BirthdaySheet(value, onSave = { m, d ->
                Birthday.set(context, m, d)
                value = m to d
                activity.hideSheet()
            }, onRemove = {
                Birthday.clear(context)
                value = null
                activity.hideSheet()
            })
        }
    }
    VCard(Modifier.padding(bottom = 8.dp), padding = PaddingValues(horizontal = 16.dp, vertical = 4.dp)) {
        Row(Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = open).padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            IconTile(R.drawable.ic_cake)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text("Your birthday", style = Type.M, color = c.text)
                val v = value
                Text(
                    if (v != null) "${Birthday.label(v.first, v.second)}. Voxa will wish you on the day" else "Add it and Voxa will wish you on the day",
                    Modifier.padding(top = 2.dp), style = Type.S, color = c.text72,
                )
            }
            Text(if (value != null) "Change" else "Add", Modifier.padding(start = 8.dp), style = Type.SBold, color = c.accent)
        }
        // The midnight wish: only once there is a birthday to wish on.
        if (value != null) {
            Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
            Row(Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f).padding(start = 48.dp)) {
                    Text("Wish me on my birthday", style = Type.M, color = c.text)
                    Text("A notification at midnight on the day", Modifier.padding(top = 2.dp), style = Type.S, color = c.text72)
                }
                VSwitch(wish) {
                    wish = it
                    BirthdayWish.setEnabled(context, it)
                }
            }
        }
    }
    Text(
        "Day and month only, no year. It stays on this phone and is never sent anywhere.",
        Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp), style = Type.XS, color = c.text72,
    )
}

/** Two wheels, day and month. Save, or Remove to forget it. */
@Composable
fun BirthdaySheet(current: Pair<Int, Int>?, onSave: (month: Int, day: Int) -> Unit, onRemove: () -> Unit) {
    val c = vx
    var month by remember { mutableStateOf(current?.first ?: 1) }
    var day by remember { mutableStateOf(current?.second ?: 1) }
    Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 24.dp).navigationBarsPadding()) {
        Text("Your birthday", style = Type.L, color = c.text)
        Text("Day and month. Voxa never asks for the year.", Modifier.padding(top = 4.dp), style = Type.S, color = c.text72)
        Box(Modifier.fillMaxWidth().padding(top = 16.dp, bottom = 24.dp)) {
            Box(Modifier.fillMaxWidth().padding(top = 80.dp).height(40.dp).clip(RoundedCornerShape(12.dp)).background(c.accent12))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val days = (1..Birthday.daysIn(month)).toList()
                Wheel(days.map { it.toString() }, (day - 1).coerceIn(0, days.lastIndex), "Day", Modifier.weight(1f)) { day = it + 1 }
                Wheel(MONTHS, month - 1, "Month", Modifier.weight(2f)) {
                    month = it + 1
                    day = day.coerceAtMost(Birthday.daysIn(month))
                }
            }
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            if (current != null) {
                Text("Remove", Modifier.clip(RoundedCornerShape(24.dp)).clickable(role = Role.Button, onClick = onRemove).padding(horizontal = 24.dp, vertical = 14.dp), style = Type.MBold, color = c.accent)
                Spacer(Modifier.width(8.dp))
            }
            Text(
                "Save",
                Modifier.clip(RoundedCornerShape(24.dp)).background(c.accent).clickable(role = Role.Button) { onSave(month, day.coerceAtMost(Birthday.daysIn(month))) }
                    .padding(horizontal = 24.dp, vertical = 14.dp),
                style = Type.MBold, color = c.onAccent,
            )
        }
    }
}

/** A snapping wheel of five 40 dp rows; the middle row is the choice. Tap a row to pick it. */
@Composable
private fun Wheel(items: List<String>, initial: Int, label: String, modifier: Modifier, onPick: (Int) -> Unit) {
    val c = vx
    val state = rememberLazyListState(initialFirstVisibleItemIndex = initial)
    val scope = rememberCoroutineScope()
    val half = with(LocalDensity.current) { 20.dp.roundToPx() }
    val centre by remember { derivedStateOf { state.firstVisibleItemIndex + if (state.firstVisibleItemScrollOffset > half) 1 else 0 } }
    LaunchedEffect(centre, state.isScrollInProgress) {
        if (!state.isScrollInProgress) onPick(centre.coerceIn(0, items.lastIndex))
    }
    // The list shrank (31 days to 30): keep the choice in range.
    LaunchedEffect(items.size) { if (state.firstVisibleItemIndex > items.lastIndex) state.scrollToItem(items.lastIndex) }
    LazyColumn(
        modifier.height(200.dp).semantics { contentDescription = "$label, ${items.getOrNull(centre) ?: ""}" },
        state = state,
        contentPadding = PaddingValues(vertical = 80.dp),
        flingBehavior = rememberSnapFlingBehavior(state),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        items(items.size) { i ->
            val on = i == centre
            Box(
                Modifier.fillMaxWidth().height(40.dp).semantics { selected = on }.clickable { scope.launch { state.animateScrollToItem(i) } },
                contentAlignment = Alignment.Center,
            ) {
                Text(items[i], style = if (on) Type.L else Type.M, color = if (on) c.text else c.text56, textAlign = TextAlign.Center)
            }
        }
    }
}
