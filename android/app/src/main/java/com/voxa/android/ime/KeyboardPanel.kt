package com.voxa.android.ime

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
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
import com.voxa.android.R

/**
 * The keyboard's views, plain Android Views so the keyboard opens fast. Layout only: what it shows
 * is decided by [VoxaKeyboard.render] from the core's state and wording.
 *
 *   [mark] Title / detail ............ [action] [x]      <- status bar (the desktop strip)
 *   [ ink line ............................. 0:07 ]      <- while listening
 *                      ( big mic )
 *                      Tap to talk
 *   [switch]      [   space   ]       [backspace] [enter]
 */
@SuppressLint("ViewConstructor")
class KeyboardPanel(context: Context) : LinearLayout(context) {
    private val density = resources.displayMetrics.density
    private fun dp(v: Float) = (v * density).toInt()

    val mark: FrameLayout
    val title: TextView
    val detail: TextView
    val action: TextView
    val close: ImageView
    val spinner: ProgressBar
    val ink: InkLineView
    val timer: TextView
    val inkRow: LinearLayout
    val mic: FrameLayout
    val micIcon: ImageView
    val hint: TextView
    val switchKey: ImageView
    val spaceKey: TextView
    val backspaceKey: ImageView
    val enterKey: ImageView

    init {
        orientation = VERTICAL
        setBackgroundColor(SURFACE)

        // ── status bar ──
        val bar = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(12f), dp(6f), dp(4f), dp(6f))
            minimumHeight = dp(56f)
        }
        mark = FrameLayout(context).apply {
            background = rounded(INK, dp(7f).toFloat())
            addView(icon(R.drawable.ic_feather, Color.WHITE), FrameLayout.LayoutParams(dp(16f), dp(16f), Gravity.CENTER))
        }
        bar.addView(mark, LayoutParams(dp(28f), dp(28f)))
        val texts = LinearLayout(context).apply { orientation = VERTICAL; setPadding(dp(12f), 0, dp(8f), 0) }
        title = text(15f, TEXT, bold = true).apply { maxLines = 2 }
        detail = text(12.5f, MUTED).apply { maxLines = 1; ellipsize = TextUtils.TruncateAt.END }
        texts.addView(title)
        texts.addView(detail)
        bar.addView(texts, LayoutParams(0, WRAP_CONTENT, 1f))
        spinner = ProgressBar(context).apply { indeterminateTintList = ColorStateList.valueOf(INK) }
        bar.addView(spinner, LayoutParams(dp(20f), dp(20f)).apply { marginEnd = dp(8f) })
        action = text(14f, Color.WHITE, bold = true).apply {
            gravity = Gravity.CENTER
            setPadding(dp(16f), 0, dp(16f), 0)
            background = ripple(rounded(INK, dp(18f).toFloat()))
            isClickable = true
        }
        bar.addView(action, LayoutParams(WRAP_CONTENT, dp(36f)))
        close = icon(R.drawable.ic_close, MUTED).apply {
            setPadding(dp(10f), dp(10f), dp(10f), dp(10f))
            background = ripple(null)
            contentDescription = "Close"
            isClickable = true
        }
        bar.addView(close, LayoutParams(dp(44f), dp(44f)))
        addView(bar, LayoutParams(MATCH_PARENT, WRAP_CONTENT))
        addView(View(context).apply { setBackgroundColor(HAIR) }, LayoutParams(MATCH_PARENT, dp(1f)))

        // ── ink line while listening ──
        inkRow = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(16f), 0, dp(16f), 0)
        }
        ink = InkLineView(context)
        inkRow.addView(ink, LayoutParams(0, dp(40f), 1f))
        timer = text(15f, TEXT, bold = true).apply {
            setPadding(dp(10f), 0, 0, 0)
            fontFeatureSettings = "tnum"
        }
        inkRow.addView(timer)
        addView(inkRow, LayoutParams(MATCH_PARENT, dp(44f)))

        // ── the mic ──
        val center = LinearLayout(context).apply { orientation = VERTICAL; gravity = Gravity.CENTER }
        mic = FrameLayout(context).apply {
            background = ripple(GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(INK) })
            elevation = dp(2f).toFloat()
            isClickable = true
            contentDescription = "Talk"
        }
        micIcon = icon(R.drawable.ic_mic, Color.WHITE)
        mic.addView(micIcon, FrameLayout.LayoutParams(dp(36f), dp(36f), Gravity.CENTER))
        center.addView(mic, LayoutParams(dp(84f), dp(84f)))
        hint = text(13f, MUTED).apply { gravity = Gravity.CENTER; setPadding(0, dp(10f), 0, 0) }
        center.addView(hint, LayoutParams(WRAP_CONTENT, WRAP_CONTENT))
        addView(center, LayoutParams(MATCH_PARENT, dp(150f)))

        // ── keys ──
        val keys = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(6f), dp(4f), dp(6f), dp(8f))
        }
        switchKey = key(R.drawable.ic_keyboard, "Switch keyboard")
        spaceKey = text(14f, MUTED).apply {
            text = "space"
            gravity = Gravity.CENTER
            background = ripple(rounded(KEY, dp(6f).toFloat()))
            isClickable = true
            contentDescription = "Space"
        }
        backspaceKey = key(R.drawable.ic_backspace, "Delete")
        enterKey = key(R.drawable.ic_return, "Enter")
        keys.addView(switchKey, keyParams(1.3f))
        keys.addView(spaceKey, keyParams(3.4f))
        keys.addView(backspaceKey, keyParams(1.3f))
        keys.addView(enterKey, keyParams(1.3f))
        addView(keys, LayoutParams(MATCH_PARENT, WRAP_CONTENT))
    }

    private fun keyParams(weight: Float) = LayoutParams(0, dp(48f), weight).apply { marginStart = dp(3f); marginEnd = dp(3f) }

    private fun key(res: Int, label: String) = icon(res, TEXT).apply {
        scaleType = ImageView.ScaleType.CENTER_INSIDE
        setPadding(dp(12f), dp(12f), dp(12f), dp(12f))
        background = ripple(rounded(KEY, dp(6f).toFloat()))
        isClickable = true
        contentDescription = label
    }

    private fun icon(res: Int, tint: Int) = ImageView(context).apply {
        setImageResource(res)
        imageTintList = ColorStateList.valueOf(tint)
    }

    private fun text(sp: Float, color: Int, bold: Boolean = false) = TextView(context).apply {
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
        setTextColor(color)
        if (bold) typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    }

    private fun rounded(color: Int, radius: Float) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = radius
    }

    private fun ripple(content: GradientDrawable?) = RippleDrawable(ColorStateList.valueOf(0x22000000), content, content ?: GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(Color.BLACK) })

    companion object {
        val INK = Color.parseColor("#8F4C2C")
        val SURFACE = Color.parseColor("#F1ECE5")
        val KEY = Color.parseColor("#FDFBF8")
        val TEXT = Color.parseColor("#1F1B16")
        val MUTED = Color.parseColor("#5A5045")
        val HAIR = Color.parseColor("#1A000000")
    }
}
