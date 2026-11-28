package com.voxa.android.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.voxa.android.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// ── surfaces ─────────────────────────────────────────────────────────────────

/** A 24 dp card: the base colour with a soft, sepia-tinted shadow (lifted by colour in dark). */
fun Modifier.vcard(c: VxColors, radius: Dp = 24.dp): Modifier {
    val shape = RoundedCornerShape(radius)
    val lifted = if (c.dark) this else this.shadow(12.dp, shape, ambientColor = c.shadow.copy(alpha = 0.10f), spotColor = c.shadow.copy(alpha = 0.16f))
    return lifted.clip(shape).background(c.card)
}

@Composable
fun VCard(modifier: Modifier = Modifier, padding: PaddingValues = PaddingValues(24.dp), onClick: (() -> Unit)? = null, content: @Composable ColumnScope.() -> Unit) {
    val c = vx
    Column(
        modifier.fillMaxWidth().vcard(c).then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier).padding(padding),
        content = content,
    )
}

@Composable
fun ScreenTitle(text: String) {
    Text(text, Modifier.padding(top = 8.dp, bottom = 24.dp), style = Type.Title, color = vx.text)
}

// ── buttons ──────────────────────────────────────────────────────────────────

@Composable
fun PrimaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, icon: Int? = null, onClick: () -> Unit) {
    val c = vx
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier.fillMaxWidth().height(56.dp)
            .then(if (enabled && !c.dark) Modifier.shadow(10.dp, shape, ambientColor = c.accent.copy(alpha = 0.2f), spotColor = c.accent.copy(alpha = 0.45f)) else Modifier)
            .alpha(if (enabled) 1f else 0.4f).clip(shape).background(c.accent)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(painterResource(icon), null, Modifier.size(24.dp), tint = c.onAccent)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, style = Type.MBold, color = c.onAccent)
    }
}

@Composable
fun SecondaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, icon: Int? = null, onClick: () -> Unit) {
    val c = vx
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier.fillMaxWidth().height(56.dp).alpha(if (enabled) 1f else 0.4f).clip(shape).background(c.accent5).border(1.dp, c.accent12, shape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(painterResource(icon), null, Modifier.size(24.dp), tint = c.accent)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, style = Type.MBold, color = c.accent)
    }
}

/** A quiet text-only action (13 sp, bold). */
@Composable
fun TextAction(text: String, color: Color = vx.accent, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Text(text, modifier.clip(RoundedCornerShape(8.dp)).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 8.dp), style = Type.SBold, color = color)
}

/** The floating pill above the tabs ("Try Voxa", "Add a fix"). */
@Composable
fun Fab(text: String, icon: Int, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = vx
    val shape = RoundedCornerShape(28.dp)
    Row(
        modifier.height(56.dp)
            .shadow(14.dp, shape, ambientColor = c.accent.copy(alpha = 0.25f), spotColor = c.accent.copy(alpha = if (c.dark) 0.3f else 0.55f))
            .clip(shape).background(c.accent).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), null, Modifier.size(24.dp), tint = c.onAccent)
        Spacer(Modifier.width(8.dp))
        Text(text, style = Type.MBold, color = c.onAccent)
    }
}

// ── small parts ──────────────────────────────────────────────────────────────

enum class PillKind { Warn, Copy, Good }

@Composable
fun Pill(text: String, kind: PillKind, modifier: Modifier = Modifier) {
    val c = vx
    val (bg, fg) = when (kind) {
        PillKind.Warn -> c.warn10 to c.warn
        PillKind.Copy -> c.accent5 to c.accent
        PillKind.Good -> c.good12 to c.good
    }
    Text(text, modifier.clip(RoundedCornerShape(8.dp)).background(bg).padding(horizontal = 8.dp, vertical = 4.dp), style = Type.XSBold, color = fg)
}

/** 48 x 28 switch: sepia when on. */
@Composable
fun VSwitch(on: Boolean, onChange: (Boolean) -> Unit) {
    val c = vx
    Box(
        Modifier.size(48.dp, 28.dp).clip(CircleShape).background(if (on) c.accent else c.text16)
            .clickable(role = Role.Switch) { onChange(!on) },
    ) {
        Box(
            Modifier.offset(x = if (on) 24.dp else 4.dp, y = 4.dp).size(20.dp)
                .shadow(1.dp, CircleShape).clip(CircleShape).background(Color.White),
        )
    }
}

/** 36 dp icon tile in a settings row. */
@Composable
fun IconTile(icon: Int, danger: Boolean = false) {
    val c = vx
    Box(Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(if (danger) c.danger8 else c.accent5), contentAlignment = Alignment.Center) {
        Icon(painterResource(icon), null, Modifier.size(20.dp), tint = if (danger) c.danger else c.accent)
    }
}

/** Setup checklist mark: green with a check when done, a grey ring when not. */
@Composable
fun CheckMark(done: Boolean) {
    val c = vx
    Box(Modifier.size(24.dp).clip(CircleShape).background(if (done) c.good else c.surface), contentAlignment = Alignment.Center) {
        Icon(painterResource(R.drawable.ic_check), null, Modifier.size(14.dp), tint = if (done) Color.White else c.text40)
    }
}

/** The user's initials on a slate circle (Voxa has no photo of them). */
@Composable
fun Avatar(name: String, size: Dp = 40.dp) {
    val c = vx
    val initials = name.split(' ', '.', '@').filter { it.isNotBlank() }.take(2).joinToString("") { it.first().uppercase() }.ifEmpty { "" }
    Box(Modifier.size(size).clip(CircleShape).background(c.avatar), contentAlignment = Alignment.Center) {
        if (initials.isEmpty()) {
            Icon(painterResource(R.drawable.ic_you), null, Modifier.size(size * 0.55f), tint = Color.White.copy(alpha = 0.85f))
        } else {
            Text(initials, style = if (size >= 56.dp) Type.L else Type.SBold, color = Color.White)
        }
    }
}

/** An app's badge: its real icon when the phone has it, else a brand-coloured tile with a glyph,
 *  else its initial on a soft sepia tile. */
@Composable
fun AppBadge(app: String, size: Dp = 40.dp, radius: Dp = 12.dp) {
    val c = vx
    val context = LocalContext.current
    var icon by remember(app) { mutableStateOf(AppIcons.cached(app)) }
    LaunchedEffect(app) {
        if (icon == null) icon = withContext(Dispatchers.IO) { AppIcons.load(context, app) }
    }
    val real = icon?.bitmap
    if (real != null) {
        Image(real, app, Modifier.size(size))
        return
    }
    val brand = AppIcons.brand(app)
    Box(
        Modifier.size(size).clip(RoundedCornerShape(radius)).background(brand?.let { Color(it.color) } ?: c.accent12),
        contentAlignment = Alignment.Center,
    ) {
        if (brand != null) {
            Image(painterResource(brand.glyph), null, Modifier.size(size * 0.55f))
        } else {
            Text(app.trim().firstOrNull()?.uppercase() ?: "V", style = if (size > 48.dp) Type.L else Type.MBold, color = c.accent)
        }
    }
}

/** Holder so "no real icon" can be cached too. */
class LoadedIcon(val bitmap: ImageBitmap?)

// ── tabs ─────────────────────────────────────────────────────────────────────

enum class Tab(val label: String, val icon: Int) {
    Home("Home", R.drawable.ic_home),
    History("History", R.drawable.ic_history),
    Words("Words", R.drawable.ic_words),
    Settings("Settings", R.drawable.ic_settings),
}

@Composable
fun TabBar(selected: Tab?, onSelect: (Tab) -> Unit) {
    val c = vx
    Column(Modifier.fillMaxWidth().background(c.base).navigationBarsPadding()) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
        Row(Modifier.fillMaxWidth().height(72.dp)) {
            for (t in Tab.entries) {
                val on = t == selected
                Column(
                    Modifier.weight(1f).fillMaxWidth().height(72.dp)
                        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, role = Role.Tab) { onSelect(t) },
                    horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
                ) {
                    Icon(painterResource(t.icon), t.label, Modifier.size(24.dp), tint = if (on) c.accent else c.text56)
                    Spacer(Modifier.height(4.dp))
                    Text(t.label, style = if (on) Type.XSBold else Type.XS, color = if (on) c.text else c.text56)
                }
            }
        }
    }
}

// ── fields ───────────────────────────────────────────────────────────────────

/** History's search: surface at rest, white with a sepia ring while focused. */
@Composable
fun SearchField(value: String, onValue: (String) -> Unit, placeholder: String, onFocus: (Boolean) -> Unit, onSearch: () -> Unit, modifier: Modifier = Modifier) {
    val c = vx
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(16.dp)
    BasicTextField(
        value, onValue,
        modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused; onFocus(it.isFocused) },
        singleLine = true,
        textStyle = Type.M.copy(color = c.text),
        cursorBrush = SolidColor(c.accent),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSearch() }),
        decorationBox = { inner ->
            Row(
                Modifier.fillMaxWidth().height(48.dp)
                    .then(if (focused) Modifier.border(2.dp, c.accent, shape) else Modifier)
                    .clip(shape).background(if (focused) c.base else c.surface).padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(painterResource(R.drawable.ic_search), null, Modifier.size(24.dp), tint = if (focused) c.text else c.text56)
                Spacer(Modifier.width(8.dp))
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty()) Text(placeholder, style = Type.M, color = c.text56)
                    inner()
                }
                if (value.isNotEmpty()) {
                    Icon(painterResource(R.drawable.ic_close), "Clear", Modifier.size(32.dp).clip(CircleShape).clickable { onValue("") }.padding(6.dp), tint = c.text56)
                }
            }
        },
    )
}

/** A text box on the surface colour (Words editor, Edit, the practice field). */
@Composable
fun VTextField(
    value: String,
    onValue: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    singleLine: Boolean = true,
    minHeight: Dp = 56.dp,
    style: TextStyle = Type.M,
    /** The field's colour at rest (the surface by default; the base on a surface background). */
    rest: Color? = null,
) {
    val c = vx
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(16.dp)
    BasicTextField(
        value, onValue,
        modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused },
        singleLine = singleLine,
        textStyle = style.copy(color = c.text),
        cursorBrush = SolidColor(c.accent),
        decorationBox = { inner ->
            Box(
                Modifier.fillMaxWidth().then(if (singleLine) Modifier.height(minHeight) else Modifier.heightIn(min = minHeight))
                    .then(if (focused) Modifier.border(2.dp, c.accent, shape) else Modifier)
                    .clip(shape).background(if (focused) c.base else rest ?: c.surface).padding(16.dp),
                contentAlignment = if (singleLine) Alignment.CenterStart else Alignment.TopStart,
            ) {
                if (value.isEmpty()) Text(placeholder, style = style, color = c.text56)
                inner()
            }
        },
    )
}

/** A field label above an input (13 sp bold). */
@Composable
fun FieldLabel(text: String) {
    Text(text, Modifier.padding(bottom = 8.dp), style = Type.SBold, color = vx.text)
}

// ── dialogs ──────────────────────────────────────────────────────────────────

@Composable
fun ConfirmDialog(title: String, body: String, confirm: String, danger: Boolean = true, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    val c = vx
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.card,
        shape = RoundedCornerShape(24.dp),
        title = { Text(title, style = Type.L, color = c.text) },
        text = { Text(body, style = Type.M, color = c.text72) },
        confirmButton = { TextButton({ onConfirm() }) { Text(confirm, style = Type.MBold, color = if (danger) c.danger else c.accent) } },
        dismissButton = { TextButton(onDismiss) { Text("Cancel", style = Type.MBold, color = c.text72) } },
    )
}

/** A row of a settings card: icon tile, label, and a value or a control on the right. */
@Composable
fun OptionRow(icon: Int, label: String, last: Boolean = false, danger: Boolean = false, onClick: (() -> Unit)? = null, trailing: @Composable RowScope.() -> Unit = {}) {
    val c = vx
    Column {
        Row(
            Modifier.fillMaxWidth().then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier).padding(vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconTile(icon, danger)
            Spacer(Modifier.width(12.dp))
            Text(label, Modifier.weight(1f), style = Type.M, color = c.text)
            trailing()
        }
        if (!last) Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
    }
}

@Composable
fun OptionValue(text: String) {
    Text(text, style = Type.SBold, color = vx.text56)
}

/** Centre content in a box (used by illustrations). */
@Composable
fun Centered(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    Box(modifier, contentAlignment = Alignment.Center, content = content)
}
