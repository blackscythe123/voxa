package com.voxa.android.ui

import android.graphics.Bitmap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.voxa.android.MainActivity
import com.voxa.android.R
import com.voxa.android.ime.KbColors
import com.voxa.android.ime.KeyboardLook
import com.voxa.android.ime.KeyboardPanel
import com.voxa.android.ime.Photo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Settings, Keyboard look: the real keyboard panel as a pinned preview (Ready, Listening, Done),
 * then the background (Voxa, Phone colours, the solid colours, your photo), Photo strength and
 * Soften for a photo, Key shapes, Mic colour, a field to try it, and Reset. Changes save at once.
 */
@Composable
fun KeyboardLookScreen(activity: MainActivity) {
    val c = vx
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var look by remember { mutableStateOf(KeyboardLook.load(ctx)) }
    /** Bumped when the photo copy changes, so the grid, thumbnail and preview reload. */
    var photoStamp by remember { mutableIntStateOf(0) }
    var importing by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    var stage by remember { mutableIntStateOf(0) }
    var tried by remember { mutableStateOf("") }
    val dark = c.dark

    fun update(l: KeyboardLook.Look) {
        look = l
        KeyboardLook.save(ctx, l)
    }

    val hasPhoto = remember(photoStamp) { KeyboardLook.hasPhoto(ctx) }
    val grid = remember(photoStamp) { runCatching { KeyboardLook.readGrid(ctx) }.getOrNull() }
    val aspect = remember { Photo.aspect(ctx) }
    val cap = remember(grid, look.position, look.keyShapes, dark) {
        grid?.let { Photo.maxStrength(it, look.position, dark, look.keyShapes, aspect) } ?: 1f
    }
    val colors = remember(look, dark, photoStamp) {
        try {
            KbColors.forLook(ctx, look, dark)
        } catch (t: Throwable) {
            KeyboardLook.fallback(ctx, "preview", t)
            look = KeyboardLook.load(ctx)
            KbColors.voxa(dark, look)
        }
    }

    // The preview's photo: the whole height (it can be dragged), soft or sharp.
    var previewPhoto by remember { mutableStateOf<Bitmap?>(null) }
    var thumb by remember { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(photoStamp, look.soften, hasPhoto) {
        previewPhoto = if (!hasPhoto) null else withContext(Dispatchers.IO) {
            runCatching {
                val m = ctx.resources.displayMetrics
                KeyboardLook.decodePhoto(ctx, m.widthPixels, (Photo.PANEL_DP * m.density).toInt(), 0.5f, look.soften, crop = false)
            }.getOrNull()
        }
        thumb = if (!hasPhoto) null else withContext(Dispatchers.IO) { KeyboardLook.thumbnail(ctx, 144) }
    }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        if (uri != null) {
            importing = true
            failed = false
            scope.launch {
                val ok = withContext(Dispatchers.IO) { KeyboardLook.importPhoto(ctx, uri) }
                importing = false
                failed = !ok
                look = KeyboardLook.load(ctx)
                photoStamp++
            }
        }
    }
    val pick = { picker.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }

    Column(Modifier.fillMaxSize()) {
        // Title, with the way back.
        Row(Modifier.padding(horizontal = 24.dp).height(56.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(40.dp).clip(CircleShape).clickable(role = Role.Button) { activity.back() }
                    .semantics { contentDescription = "Back" },
                contentAlignment = Alignment.Center,
            ) {
                Icon(painterResource(R.drawable.ic_back), null, Modifier.size(24.dp), tint = c.text)
            }
            Spacer(Modifier.width(8.dp))
            Text("Keyboard look", style = Type.L, color = c.text)
        }

        // The real keyboard panel, pinned. Touches go to the drag layer above it, not its keys.
        Box(Modifier.fillMaxWidth()) {
            var panelRef by remember { mutableStateOf<KeyboardPanel?>(null) }
            key(colors) {
                AndroidView(
                    factory = { KeyboardPanel(it, colors).also { p -> panelRef = p } },
                    modifier = Modifier.fillMaxWidth(),
                    update = { p ->
                        p.setPhoto(if (colors.photo) previewPhoto else null)
                        p.demo(stage)
                    },
                )
            }
            Box(
                Modifier.matchParentSize().pointerInput(colors.photo) {
                    if (!colors.photo) return@pointerInput
                    detectVerticalDragGestures(
                        onDragEnd = { panelRef?.let { update(look.copy(position = it.photoPosition)) } },
                    ) { change, dy ->
                        val p = panelRef ?: return@detectVerticalDragGestures
                        val over = p.photoOverflow()
                        if (over > 0f) {
                            change.consume()
                            p.photoPosition -= dy / over
                        }
                    }
                },
            )
            if (importing) {
                Box(Modifier.matchParentSize().background(c.base.copy(alpha = 0.6f)), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = c.accent, strokeWidth = 3.dp, modifier = Modifier.size(40.dp))
                }
            }
        }

        // Ready / Listening / Done, to check the timer, the bars and the green state.
        Column(Modifier.padding(horizontal = 24.dp).padding(top = 12.dp)) {
            Segmented(listOf("Ready", "Listening", "Done"), stage) { stage = it }
            if (colors.photo) {
                Text("Drag to move your photo", Modifier.padding(top = 8.dp).fillMaxWidth(), style = Type.XS, color = c.text56)
            }
        }

        // Above the real keyboard while trying it (edge to edge, the window doesn't resize for it).
        Column(Modifier.weight(1f).imePadding().verticalScroll(rememberScrollState()).padding(top = 16.dp, bottom = 24.dp)) {
            SectionLabel("Background")
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Swatch("Voxa", look.type == KeyboardLook.VOXA, onClick = { update(look.copy(type = KeyboardLook.VOXA)) }) {
                    Row(Modifier.fillMaxSize()) {
                        Box(Modifier.weight(1f).fillMaxSize().background(Color(0xFFF3F3F1)))
                        Box(Modifier.weight(1f).fillMaxSize().background(Color(0xFF1A1A19)))
                    }
                    Box(Modifier.size(18.dp).clip(CircleShape).background(Color(0xFF8F4C2C)))
                }
                if (KeyboardLook.phoneColorsAvailable) {
                    val n = remember { Color(ctx.getColor(if (dark) android.R.color.system_neutral1_900 else android.R.color.system_neutral1_50)) }
                    val a = remember { Color(ctx.getColor(if (dark) android.R.color.system_accent1_200 else android.R.color.system_accent1_600)) }
                    Swatch("Phone colours", look.type == KeyboardLook.PHONE, onClick = { update(look.copy(type = KeyboardLook.PHONE)) }) {
                        Box(Modifier.fillMaxSize().background(n))
                        Box(Modifier.size(18.dp).clip(CircleShape).background(a))
                    }
                }
                for (p in KeyboardLook.presets) {
                    val on = look.type == KeyboardLook.COLOR && look.color == p.color
                    Swatch(p.name, on, onClick = { update(look.copy(type = KeyboardLook.COLOR, color = p.color)) }) {
                        Box(Modifier.fillMaxSize().background(Color(p.color)))
                    }
                }
                Swatch(if (hasPhoto) "Your photo" else "Photo", look.type == KeyboardLook.PHOTO, onClick = {
                    if (!hasPhoto) {
                        pick()
                    } else {
                        val photoChosen = look.type == KeyboardLook.PHOTO
                        activity.showSheet {
                            Column(Modifier.padding(start = 24.dp, end = 24.dp, bottom = 24.dp)) {
                                Text("Your photo", Modifier.padding(bottom = 8.dp), style = Type.L, color = vx.text)
                                if (!photoChosen) {
                                    OptionRow(R.drawable.ic_image, "Use this photo", onClick = {
                                        activity.hideSheet()
                                        update(KeyboardLook.load(ctx).copy(type = KeyboardLook.PHOTO))
                                    })
                                }
                                OptionRow(R.drawable.ic_refresh, "Change photo", onClick = {
                                    activity.hideSheet()
                                    pick()
                                })
                                OptionRow(R.drawable.ic_delete, "Remove photo", last = true, danger = true, onClick = {
                                    activity.hideSheet()
                                    KeyboardLook.removePhoto(ctx)
                                    look = KeyboardLook.load(ctx)
                                    photoStamp++
                                })
                            }
                        }
                    }
                }) {
                    val t = thumb
                    if (hasPhoto && t != null) {
                        Image(t.asImageBitmap(), null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                    } else {
                        Box(Modifier.fillMaxSize().background(c.surface))
                        Icon(painterResource(R.drawable.ic_add), null, Modifier.size(24.dp), tint = c.text72)
                    }
                }
            }
            if (failed) {
                Text(
                    "That photo couldn't be used. Try another one.",
                    Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp), style = Type.S, color = c.danger,
                )
            }

            if (look.type == KeyboardLook.PHOTO) {
                VCard(Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp), padding = PaddingValues(16.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Photo strength", Modifier.weight(1f), style = Type.M, color = c.text)
                        Text("${(minOf(look.strength, cap) * 100).toInt()} %", style = Type.SBold, color = c.text56)
                    }
                    // The thumb stops where text would stop reading (the rest of the track can't be picked).
                    Slider(
                        value = minOf(look.strength, cap),
                        onValueChange = { look = look.copy(strength = it.coerceAtMost(cap)) },
                        onValueChangeFinished = { update(look) },
                        colors = SliderDefaults.colors(
                            thumbColor = c.accent, activeTrackColor = c.accent, inactiveTrackColor = c.text16,
                            activeTickColor = Color.Transparent, inactiveTickColor = Color.Transparent,
                        ),
                    )
                    Box(Modifier.fillMaxWidth().height(1.dp).background(c.hair))
                    Row(
                        Modifier.fillMaxWidth().clickable { update(look.copy(soften = !look.soften)) }.padding(vertical = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Soften photo", Modifier.weight(1f), style = Type.M, color = c.text)
                        VSwitch(look.soften) { update(look.copy(soften = it)) }
                    }
                    Text("Your photo stays on this phone.", style = Type.S, color = c.text56)
                }
            }

            VCard(Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp), padding = PaddingValues(horizontal = 16.dp, vertical = 4.dp)) {
                Row(
                    Modifier.fillMaxWidth().clickable { update(look.copy(keyShapes = !look.keyShapes)) }.padding(vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Key shapes", Modifier.weight(1f), style = Type.M, color = c.text)
                    VSwitch(look.keyShapes) { update(look.copy(keyShapes = it)) }
                }
            }

            SectionLabel("Mic colour")
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                // The default follows the look: Voxa sepia, or the phone's accent with Phone colours.
                val def = if (look.type == KeyboardLook.PHONE) KbColors.forLook(ctx, look.copy(micColor = 0), dark).accent
                else if (colors.dark) 0xFFD9946D.toInt() else 0xFF8F4C2C.toInt()
                MicDot("Default", Color(def), look.micColor == 0) { update(look.copy(micColor = 0)) }
                for (m in KeyboardLook.micColors) {
                    MicDot(m.name, Color(m.color), look.micColor == m.color) { update(look.copy(micColor = m.color)) }
                }
            }

            SectionLabel("Try it")
            Column(Modifier.padding(horizontal = 24.dp)) {
                VTextField(tried, { tried = it }, "Try your keyboard here", singleLine = false, minHeight = 96.dp)
                Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Using another keyboard?", style = Type.S, color = c.text56)
                    TextAction("Switch to Voxa") { activity.showKeyboardPicker() }
                }
                TextAction("Reset to Voxa look", modifier = Modifier.padding(top = 16.dp)) {
                    KeyboardLook.reset(ctx)
                    look = KeyboardLook.load(ctx)
                    failed = false
                    photoStamp++
                }
            }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text, Modifier.padding(start = 24.dp, end = 24.dp, top = 24.dp, bottom = 12.dp), style = Type.SBold, color = vx.text)
}

/** A round 56 dp swatch with its name under it; a sepia ring when chosen. */
@Composable
private fun Swatch(label: String, selected: Boolean, onClick: () -> Unit, content: @Composable () -> Unit) {
    val c = vx
    Column(
        Modifier.widthIn(min = 72.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }.padding(vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier.size(60.dp).border(2.dp, if (selected) c.accent else Color.Transparent, CircleShape).padding(4.dp)
                .clip(CircleShape).border(1.dp, c.hair, CircleShape),
            contentAlignment = Alignment.Center,
        ) { content() }
        Text(label, Modifier.padding(top = 4.dp), style = if (selected) Type.XSBold else Type.XS, color = if (selected) c.text else c.text56, maxLines = 1, softWrap = false)
    }
}

@Composable
private fun MicDot(name: String, color: Color, selected: Boolean, onClick: () -> Unit) {
    val c = vx
    Box(
        Modifier.size(48.dp).clip(CircleShape).clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { contentDescription = name; this.selected = selected }
            .border(2.dp, if (selected) c.text else Color.Transparent, CircleShape).padding(5.dp).clip(CircleShape).background(color),
        contentAlignment = Alignment.Center,
    ) {
        Icon(painterResource(R.drawable.ic_mic), null, Modifier.size(20.dp), tint = Color(KbColors.micIcon(color.toArgbInt())))
    }
}

private fun Color.toArgbInt(): Int = android.graphics.Color.argb((alpha * 255).toInt(), (red * 255).toInt(), (green * 255).toInt(), (blue * 255).toInt())
