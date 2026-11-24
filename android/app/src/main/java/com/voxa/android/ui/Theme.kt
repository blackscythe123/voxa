package com.voxa.android.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Button
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.Typography
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.R

/** Voxa v7: native Material 3 with warm paper neutrals and sepia ink (desktop home.rs colours). */
object V {
    val Ink = Color(0xFF8F4C2C)
    val InkHover = Color(0xFF7C4125)
    val InkContainer = Color(0xFFF6DCCB)
    val Page = Color(0xFFFBF8F4)
    val Nav = Color(0xFFF1EEEA)
    val Card = Color.White
    val CardBorder = Color(0x14000000)
    val Text = Color(0xFF1A1A1A)
    val Muted = Color(0xFF5F5F5F)
    val Ready = Color(0xFF0E5C0E)
    val Warn = Color(0xFF9D5D00)
    val WarnBg = Color(0xFFFFF4CE)
    val Error = Color(0xFFBA1A1A)
    val ErrorBg = Color(0xFFFDE7E9)
    val Fraunces = FontFamily(Font(R.font.fraunces, FontWeight.SemiBold))
}

private val colors = lightColorScheme(
    primary = V.Ink,
    onPrimary = Color.White,
    primaryContainer = V.InkContainer,
    onPrimaryContainer = Color(0xFF3A1A08),
    secondaryContainer = Color(0xFFECE3D8),
    onSecondaryContainer = Color(0xFF2A221A),
    background = V.Page,
    onBackground = V.Text,
    surface = V.Page,
    onSurface = V.Text,
    surfaceVariant = Color(0xFFF1ECE5),
    onSurfaceVariant = Color(0xFF4E463D),
    surfaceContainer = Color(0xFFF1ECE5),
    surfaceContainerHigh = Color(0xFFEBE5DD),
    surfaceContainerHighest = Color(0xFFE5DED5),
    outline = Color(0xFF80766B),
    outlineVariant = Color(0xFFD3C9BD),
    error = V.Error,
    errorContainer = Color(0xFFFFDAD6),
)

@Composable
fun VoxaTheme(content: @Composable () -> Unit) {
    val base = Typography()
    MaterialTheme(
        colorScheme = colors,
        typography = base.copy(
            bodyLarge = base.bodyLarge.copy(fontSize = 15.sp, lineHeight = 21.sp),
            bodyMedium = base.bodyMedium.copy(fontSize = 14.sp, lineHeight = 20.sp),
        ),
        content = content,
    )
}

/** The "voxa" wordmark in Fraunces, as on the desktop. */
@Composable
fun Wordmark(size: Int = 30) {
    Text("voxa", style = TextStyle(fontFamily = V.Fraunces, fontWeight = FontWeight.SemiBold, fontSize = size.sp, color = V.Text, letterSpacing = (-0.5).sp))
}

/** The desktop's card(): white, thin warm border, 4 px radius. */
@Composable
fun VCard(modifier: Modifier = Modifier, onClick: (() -> Unit)? = null, content: @Composable RowScope.() -> Unit) {
    val shape = RoundedCornerShape(4.dp)
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clip(shape)
            .background(V.Card)
            .border(1.dp, V.CardBorder, shape)
            .let { if (onClick != null) it.clickable(onClick = onClick) else it }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        content = content,
    )
}

@Composable
fun VColumnCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(4.dp)
    Column(
        modifier.fillMaxWidth().clip(shape).background(V.Card).border(1.dp, V.CardBorder, shape).padding(16.dp),
        content = content,
    )
}

@Composable
fun Section(title: String) {
    Text(title, Modifier.padding(top = 24.dp, bottom = 8.dp), fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
}

/** The desktop's wbtn(): accent (ink) or plain, 4 px radius. */
@Composable
fun VButton(label: String, accent: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    val shape = RoundedCornerShape(4.dp)
    val pad = PaddingValues(horizontal = 14.dp, vertical = 0.dp)
    if (accent) {
        Button(onClick, enabled = enabled, shape = shape, contentPadding = pad, modifier = Modifier.heightIn(min = 36.dp),
            colors = ButtonDefaults.buttonColors(containerColor = V.Ink, contentColor = Color.White)) { Text(label, fontSize = 14.sp) }
    } else {
        OutlinedButton(onClick, enabled = enabled, shape = shape, contentPadding = pad, modifier = Modifier.heightIn(min = 36.dp),
            border = BorderStroke(1.dp, Color(0x1A000000)),
            colors = ButtonDefaults.outlinedButtonColors(containerColor = Color.White, contentColor = V.Text)) { Text(label, fontSize = 14.sp) }
    }
}

@Composable
fun VToggle(on: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(if (on) "On" else "Off", fontSize = 14.sp, color = V.Text)
        Spacer(Modifier.width(12.dp))
        Switch(on, onChange, colors = SwitchDefaults.colors(checkedTrackColor = V.Ink, checkedThumbColor = Color.White))
    }
}

/** A round icon chip on the left of a card. */
@Composable
fun CardIcon(res: Int, tint: Color = V.Ink) {
    Box(Modifier.size(36.dp).clip(RoundedCornerShape(18.dp)).background(Color(0xFFF6F2EC)), contentAlignment = Alignment.Center) {
        Icon(painterResource(res), null, Modifier.size(20.dp), tint = tint)
    }
}

/** Fluent-style InfoBar: what needs the user, on top of the page. */
@Composable
fun InfoBar(title: String, detail: String? = null, error: Boolean = false, action: String? = null, onAction: () -> Unit = {}) {
    val shape = RoundedCornerShape(4.dp)
    Row(
        Modifier.fillMaxWidth().padding(bottom = 8.dp).clip(shape).background(if (error) V.ErrorBg else V.WarnBg).border(1.dp, V.CardBorder, shape)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(R.drawable.ic_alert), null, Modifier.size(18.dp), tint = if (error) V.Error else V.Warn)
        Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
            Text(title, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = V.Text)
            if (detail != null) Text(detail, fontSize = 13.sp, color = V.Muted)
        }
        if (action != null) VButton(action, onClick = onAction)
    }
}

@Composable
fun Muted(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier, fontSize = 13.sp, color = V.Muted)
}
