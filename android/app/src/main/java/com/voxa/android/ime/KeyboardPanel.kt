package com.voxa.android.ime

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
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
 *   (•) Listening ........................ 0:04 [action] [x]
 *   detail line (when the core has one)
 *   ▮▮▮▮▮▮▮▮   ( big mic / stop )   ▮▮▮▮▮▮▮▮        <- or the green check, "That's 6 seconds of typing saved"
 *   [switch] [   space / Tap to stop   ] [backspace] [enter]
 */
@SuppressLint("ViewConstructor")
class KeyboardPanel(context: Context) : LinearLayout(context) {
    private val density = resources.displayMetrics.density
    private fun dp(v: Float) = (v * density).toInt()
    val colors = KbColors.of(context)
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
        setBackgroundColor(c.surface)
        clipChildren = false
        clipToPadding = false
        setPadding(dp(16f), dp(16f), dp(16f), dp(24f))

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
        action = text(13f, c.onAccent, bold).apply {
            gravity = Gravity.CENTER
            setPadding(dp(16f), 0, dp(16f), 0)
            background = ripple(rounded(c.accent, dp(16f).toFloat()))
            isClickable = true
        }
        top.addView(action, LayoutParams(WRAP_CONTENT, dp(32f)).apply { marginStart = dp(8f) })
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
        detail = text(13f, c.muted, medium).apply { maxLines = 2; ellipsize = TextUtils.TruncateAt.END }
        stage.addView(detail, FrameLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT, Gravity.TOP))
        bars = LevelBarsView(context).apply { color(c.alpha(c.accent, 0.3f)) }
        stage.addView(bars, FrameLayout.LayoutParams(MATCH_PARENT, dp(64f), Gravity.CENTER))
        spinner = ProgressBar(context).apply { indeterminateTintList = ColorStateList.valueOf(c.accent) }
        stage.addView(spinner, FrameLayout.LayoutParams(dp(120f), dp(120f), Gravity.CENTER))
        mic = RoundView(context, 88f, 12f).apply {
            icon = context.getDrawable(R.drawable.ic_mic)?.mutate()
            style(c.accent, c.alpha(c.accent, if (c.dark) 0.22f else 0.12f), c.alpha(c.accent, if (c.dark) 0.25f else 0.35f), 28f, 12f, c.onAccent)
            isClickable = true
            contentDescription = "Talk"
        }
        stage.addView(mic, FrameLayout.LayoutParams(dp(112f), dp(112f), Gravity.CENTER))
        done = LinearLayout(context).apply {
            orientation = VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
            clipChildren = false
            clipToPadding = false
        }
        check = RoundView(context, 72f, 12f).apply {
            icon = context.getDrawable(R.drawable.ic_check)?.mutate()
            iconSizeDp = 36f
            style(c.good, c.alpha(c.good, if (c.dark) 0.2f else 0.12f), c.alpha(c.good, 0.35f), 20f, 0f, 0xFFFFFFFF.toInt())
            importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        done.addView(check, LayoutParams(dp(96f), dp(96f)))
        saved = text(13f, c.muted, medium).apply { gravity = Gravity.CENTER; setPadding(0, dp(12f), 0, 0) }
        done.addView(saved, LayoutParams(WRAP_CONTENT, WRAP_CONTENT))
        stage.addView(done, FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT, Gravity.CENTER))
        addView(stage, LayoutParams(MATCH_PARENT, dp(176f)))

        // ── keys: 56 | flexible | 56 | 56, 8 apart ──
        val keys = LinearLayout(context).apply { orientation = HORIZONTAL; clipChildren = false }
        switchKey = key(R.drawable.ic_keyboard, "Switch keyboard")
        spaceKey = text(13f, c.alpha(c.text, 0.72f), bold).apply {
            gravity = Gravity.CENTER
            text = "space"
            background = keyBackground()
            elevation = if (c.dark) 0f else dp(1f).toFloat()
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

    private fun key(res: Int, label: String) = ImageView(context).apply {
        setImageResource(res)
        imageTintList = ColorStateList.valueOf(colors.alpha(colors.text, 0.72f))
        scaleType = ImageView.ScaleType.CENTER
        background = keyBackground()
        elevation = if (colors.dark) 0f else dp(1f).toFloat()
        isClickable = true
        contentDescription = label
    }

    private fun keyBackground() = ripple(rounded(colors.key, dp(12f).toFloat()))

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
