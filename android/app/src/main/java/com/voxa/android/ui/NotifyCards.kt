package com.voxa.android.ui

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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.clickable
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.diag.Diag
import com.voxa.android.notify.Notifier

/**
 * The first of the two asks: right after signing back in when the session had ended.
 * "Allow" brings up Android's own prompt; either answer counts as one ask.
 */
@Composable
fun ColumnScope.NotifyAskSheet(activity: MainActivity) {
    val c = vx
    Column(Modifier.fillMaxWidth().padding(start = 24.dp, end = 24.dp, bottom = 24.dp).navigationBarsPadding()) {
        Text("Want a notification next time you're signed out?", style = Type.L, color = c.text)
        Spacer(Modifier.height(24.dp))
        PrimaryButton("Allow", Modifier.fillMaxWidth()) {
            Diag.log("notify", "ask after sign-in: allow")
            Notifier.asked(activity)
            activity.hideSheet()
            activity.requestNotifications()
        }
        Spacer(Modifier.height(8.dp))
        SecondaryButton("Not now", Modifier.fillMaxWidth()) {
            Diag.log("notify", "ask after sign-in: not now")
            Notifier.asked(activity)
            activity.hideSheet()
        }
    }
}

/** The second ask: a dismissible Home card, once the first dictation has worked. */
@Composable
fun NotifyHomeCard(activity: MainActivity, onDone: () -> Unit) {
    val c = vx
    VCard(Modifier.padding(bottom = 24.dp), padding = PaddingValues(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconTile(R.drawable.ic_bell)
            Spacer(Modifier.width(12.dp))
            Text("Know when you're signed out", Modifier.weight(1f), style = Type.MBold, color = c.text)
            Icon(
                painterResource(R.drawable.ic_close), "Not now",
                Modifier.size(32.dp).clip(CircleShape).clickable {
                    Diag.log("notify", "home card: dismissed")
                    Notifier.asked(activity)
                    Notifier.dismissHomeCard(activity)
                    onDone()
                }.padding(6.dp),
                tint = c.text56,
            )
        }
        Spacer(Modifier.height(12.dp))
        PrimaryButton("Allow", Modifier.fillMaxWidth()) {
            Diag.log("notify", "home card: allow")
            Notifier.asked(activity)
            Notifier.dismissHomeCard(activity)
            onDone()
            activity.requestNotifications()
        }
    }
}
