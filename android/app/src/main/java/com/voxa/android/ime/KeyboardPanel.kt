package com.voxa.android.ime

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.core.content.res.ResourcesCompat
import com.voxa.android.R

/**
 * The keyboard's views (docs/ui-mocks/v6-app-redesign.html, "Keyboard, listening" and "done"),
 * plain Android Views so the keyboard opens fast and stays small. Layout only: what it shows is
 * decided by [VoxaKeyboard.render] from the core's state and wording.
 *
 *   (•) Listening ................................. 0:04 [x]
 *   detail line (when the core has one) ............ [action]
 *   ▮▮▮▮▮▮▮▮   ( big mic / stop )   ▮▮▮▮▮▮▮▮        <- or the green check, "That's 6 seconds of typing saved"
 *   [switch] [   space / Tap to stop   ] [backspace] [enter]
 *
 * Drawn in [colors] (the keyboard look); the app's Keyboard look screen shows this same panel as
 * its preview, so there is one drawing path.
 */
@SuppressLint("ViewConstructor")
class KeyboardPanel(context: Context, val colors: KbColors) : LinearLayout(context) {
    private val density = resources.displayMetrics.density
    private fun dp(v: Float) = (v * density).toInt()
    private val bold: Typeface = ResourcesCompat.getFont(context, R.font.manrope_bold) ?: Typeface.DEFAULT_BOLD
    private val medium: Typeface = ResourcesCompat.getFont(context, R.font.manrope_medium) ?: Typeface.DEFAULT

    val dot: View
    val title: TextView
    val right: TextView
    val action: TextView
    val close: ImageView
    val detail: TextView
    val bars: LevelBarsView
    val mic: RoundView
    val spinner: ProgressBar
    val done: LinearLayout
    val check: RoundView
    val saved: TextView
    val switchKey: ImageView
    val spaceKey: TextView
    val backspaceKey: ImageView
    val enterKey: ImageView

    init {
        val c = colors
        orientation = VERTICAL
        // The background (and the photo under its wash) is drawn in onDraw.
        setWillNotDraw(false)
        clipChildren = false
        clipToPadding = false
        bottomInset(0)

        // ── status line ──
        val top = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        dot = View(context).apply { background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(c.recording) } }
        top.addView(dot, LayoutParams(dp(8f), dp(8f)).apply { marginEnd = dp(8f) })
        title = text(16f, c.text, bold).apply { maxLines = 1; ellipsize = TextUtils.TruncateAt.END }
        top.addView(title, LayoutParams(0, WRAP_CONTENT, 1f))
        right = text(13f, c.muted, bold).apply { fontFeatureSettings = "tnum"; setPadding(dp(8f), 0, 0, 0) }
        top.addView(right)
        close = ImageView(context).apply {
            setImageResource(R.drawable.ic_close)
            imageTintList = ColorStateList.valueOf(c.muted)
            setPadding(dp(6f), dp(6f), dp(6f), dp(6f))
            background = ripple(null)
            contentDescription = "Close"
            isClickable = true
        }
        top.addView(close, LayoutParams(dp(32f), dp(32f)).apply { marginStart = dp(4f) })
        addView(top, LayoutParams(MATCH_PARENT, dp(32f)))

        // ── the stage: level bars behind the mic, or the green check ──
        val stage = FrameLayout(context).apply { clipChildren = false; clipToPadding = false }
        // The core's detail line and its one action, under the status line.
        val info = LinearLayout(context).apply { orientation = HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
        detail = text(13f, c.muted, medium).apply { maxLines = 2; ellipsize = TextUtils.TruncateAt.END }
        info.addView(detail, LayoutParams(0, WRAP_CONTENT, 1f))
        action = text(13f, c.onAccent, bold).apply {
            gravity = Gravity.CENTER
            setPadding(dp(16f), 0, dp(16f), 0)
            background = ripple(rounded(c.accent, dp(16f).toFloat()))
            isClickable = true
        }
        info.addView(action, LayoutParams(WRAP_CONTENT, dp(32f)).apply { marginStart = dp(8f) })
        bars = LevelBarsView(context).apply { color(c.bars) }
        stage.addView(bars, FrameLayout.LayoutParams(MATCH_PARENT, dp(64f), Gravity.CENTER))
        spinner = ProgressBar(context).apply { indeterminateTintList = ColorStateList.valueOf(c.spinner) }
        stage.addView(spinner, FrameLayout.LayoutParams(dp(120f), dp(120f), Gravity.CENTER))
        mic = RoundView(context, 88f, 12f).apply {
            icon = context.getDrawable(R.drawable.ic_mic)?.mutate()
            style(c.accent, c.micRing, c.micGlow, 20f, 8f, c.onAccent)
            isClickable = true
            contentDescription = "Talk"
        }
        // 88 + a 12 dp ring each side + room for the 20 dp glow 8 dp down.
        stage.addView(mic, FrameLayout.LayoutParams(dp(168f), dp(168f), Gravity.CENTER))
        done = LinearLayout(context).apply {
            orientation = VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            clipChildren = false
            clipToPadding = false
        }
        check = RoundView(context, 72f, 12f).apply {
            icon = context.getDrawable(R.drawable.ic_check)?.mutate()
            iconSizeDp = 36f
            style(c.good, c.alpha(c.good, if (c.dark) 0.2f else 0.12f), c.alpha(c.good, 0.35f), 16f, 0f, 0xFFFFFFFF.toInt())
            importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        // 72 + a 12 dp ring each side + the 16 dp glow; the caption tucks into the glow's room.
        done.addView(check, LayoutParams(dp(128f), dp(128f)))
        saved = text(13f, c.stageMuted, medium).apply { gravity = Gravity.CENTER; setPadding(0, 0, 0, 0) }
        done.addView(saved, LayoutParams(WRAP_CONTENT, WRAP_CONTENT))
        stage.addView(done, FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT, Gravity.CENTER))
        // Last, so the detail row and its action sit above the mic's glow.
        stage.addView(info, FrameLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT, Gravity.TOP))
        addView(stage, LayoutParams(MATCH_PARENT, dp(176f)))

        // ── keys: 56 | flexible | 56 | 56, 8 apart ──
        val keys = LinearLayout(context).apply { orientation = HORIZONTAL; clipChildren = false }
        switchKey = key(R.drawable.ic_keyboard, "Switch keyboard")
        spaceKey = text(13f, c.keyText, bold).apply {
            gravity = Gravity.CENTER
            text = "space"
            background = keyBackground()
            elevation = if (c.keyElevation) dp(1f).toFloat() else 0f
            isClickable = true
            contentDescription = "Space"
        }
        backspaceKey = key(R.drawable.ic_backspace, "Delete")
        enterKey = key(R.drawable.ic_return, "Enter")
        keys.addView(switchKey, LayoutParams(dp(56f), dp(48f)))
        keys.addView(spaceKey, LayoutParams(0, dp(48f), 1f).apply { marginStart = dp(8f); marginEnd = dp(8f) })
        keys.addView(backspaceKey, LayoutParams(dp(56f), dp(48f)).apply { marginEnd = dp(8f) })
        keys.addView(enterKey, LayoutParams(dp(56f), dp(48f)))
        addView(keys, LayoutParams(MATCH_PARENT, WRAP_CONTENT))
    }

    /** The panel's own padding plus the navigation bar under it. */
    fun bottomInset(px: Int) = setPadding(dp(16f), dp(16f), dp(16f), dp(24f) + px)

    private fun key(res: Int, label: String) = ImageView(context).apply {
        setImageResource(res)
        imageTintList = ColorStateList.valueOf(colors.keyText)
        scaleType = ImageView.ScaleType.CENTER
        background = keyBackground()
        elevation = if (colors.keyElevation) dp(1f).toFloat() else 0f
        isClickable = true
        contentDescription = label
    }

    /** A filled key, or (Key shapes off) only the press ripple in the key's shape. */
    private fun keyBackground(): RippleDrawable {
        if (colors.key != 0) return ripple(rounded(colors.key, dp(12f).toFloat()))
        return RippleDrawable(ColorStateList.valueOf(colors.press), null, rounded(0xFF000000.toInt(), dp(12f).toFloat()))
    }

    // ── the background: a solid colour, or the photo under its wash ──────────

    private var photo: Bitmap? = null
    private val photoPaint = Paint(Paint.FILTER_BITMAP_FLAG or Paint.ANTI_ALIAS_FLAG)
    private val photoMatrix = Matrix()

    /** The photo's up/down position, 0 (top) .. 1 (bottom); the app's preview drags it. */
    var photoPosition = colors.look.position
        set(v) {
            field = v.coerceIn(0f, 1f)
            invalidate()
        }

    /** The decoded photo (any size: drawn centre-cropped, scaled up with bilinear filtering). */
    fun setPhoto(b: Bitmap?) {
        photo = b
        invalidate()
    }

    val hasPhoto get() = photo != null

    /** How far the photo reaches past the panel's top and bottom together, in px (for dragging). */
    fun photoOverflow(): Float {
        val b = photo ?: return 0f
        if (width == 0 || height == 0) return 0f
        val s = maxOf(width / b.width.toFloat(), height / b.height.toFloat())
        return (b.height * s - height).coerceAtLeast(0f)
    }

    private val fillPaint = Paint()

    override fun onDraw(canvas: Canvas) {
        val w = width.toFloat()
        val h = height.toFloat()
        val b = photo
        // Rectangles, not drawColor: inside the app's preview the canvas reaches past the panel.
        if (!colors.photo || b == null || b.isRecycled) {
            fillPaint.color = colors.surface
            canvas.drawRect(0f, 0f, w, h, fillPaint)
        } else {
            canvas.save()
            canvas.clipRect(0f, 0f, w, h)
            val s = maxOf(w / b.width, h / b.height)
            photoMatrix.setScale(s, s)
            photoMatrix.postTranslate((w - b.width * s) / 2f, (h - b.height * s) * photoPosition)
            canvas.drawBitmap(b, photoMatrix, photoPaint)
            canvas.restore()
            fillPaint.color = colors.alpha(colors.surface, colors.washAlpha)
            canvas.drawRect(0f, 0f, w, h, fillPaint)
        }
        super.onDraw(canvas)
    }

    // ── what it shows ────────────────────────────────────────────────────────

    /** Everything the panel shows at one moment; [VoxaKeyboard.render] decides it from the core. */
    class State(
        val title: String,
        val detail: String? = null,
        val action: String? = null,
        val closable: Boolean = false,
        val recording: Boolean = false,
        val transcribing: Boolean = false,
        val busy: Boolean = false,
        /** The words landed: the green check instead of the mic. */
        val done: Boolean = false,
        val right: String = "",
        val saved: String = "",
        /** Disabled (writing, or a password field): 40 % and not tappable. */
        val micOff: Boolean = false,
    )

    fun show(s: State) {
        val c = colors
        title.text = s.title
        title.setTextColor(if (s.done) c.goodTitle else c.text)
        dot.visibility = if (s.recording) VISIBLE else GONE
        detail.text = s.detail ?: ""
        detail.visibility = if (s.detail.isNullOrEmpty()) INVISIBLE else VISIBLE
        action.text = s.action ?: ""
        action.visibility = if (s.action != null) VISIBLE else GONE
        close.visibility = if (s.closable) VISIBLE else GONE
        close.contentDescription = if (s.recording || s.transcribing) "Cancel" else "Close"
        right.text = s.right

        // The stage: bars and the mic, the spinner while writing, or the green check.
        bars.visibility = if (s.recording) VISIBLE else INVISIBLE
        spinner.visibility = if (s.busy) VISIBLE else GONE
        done.visibility = if (s.done) VISIBLE else GONE
        mic.visibility = if (s.done) INVISIBLE else VISIBLE
        saved.text = s.saved
        saved.visibility = if (s.saved.isEmpty()) GONE else VISIBLE
        mic.icon = context.getDrawable(if (s.recording) R.drawable.ic_stop else R.drawable.ic_mic)?.mutate()?.apply { setTint(c.onAccent) }
        mic.contentDescription = if (s.recording) "Stop" else "Talk"
        mic.alpha = if (s.micOff) 0.4f else 1f
        mic.isEnabled = !s.micOff
        spaceKey.text = if (s.recording) "Tap to stop" else "space"
        spaceKey.contentDescription = if (s.recording) "Stop" else "Space"
    }

    /** The app's preview: 0 ready, 1 listening, 2 done, with sample words. */
    fun demo(which: Int) {
        when (which) {
            1 -> {
                show(State("Listening", closable = true, recording = true, right = "0:04"))
                bars.reset()
                for (i in 0 until LevelBarsView.BARS_PER_SIDE) bars.level(0.3f + 0.6f * kotlin.math.abs(kotlin.math.sin(i * 0.9f)))
            }
            2 -> show(State("Pasted in Messages", done = true, right = "12 words", saved = "That's 6 seconds of typing saved"))
            else -> show(State("Ready to dictate"))
        }
    }

    private fun text(sp: Float, color: Int, face: Typeface) = TextView(context).apply {
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
        setTextColor(color)
        typeface = face
        includeFontPadding = false
    }

    private fun rounded(color: Int, radius: Float) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = radius
    }

    private fun ripple(content: GradientDrawable?) =
        RippleDrawable(ColorStateList.valueOf(colors.press), content, content ?: GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(0xFF000000.toInt()) })
}
