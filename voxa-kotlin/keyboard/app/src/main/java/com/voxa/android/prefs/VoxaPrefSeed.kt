package com.voxa.android.prefs

import android.content.Context
import android.content.SharedPreferences
import helium314.keyboard.latin.utils.DeviceProtectedUtils

/**
 * One-shot seeding of HeliBoard preferences with the Voxa team's preferred defaults.
 *
 * HeliBoard's own [helium314.keyboard.latin.settings.Defaults] holds upstream's
 * intended defaults — we don't fork those because we want clean upstream merges.
 * Instead we write the Voxa overrides into the *actual* SharedPreferences once,
 * the first time a build with this seed version launches. After that the user is
 * in control: any change they make sticks, and the seeder won't re-overwrite it.
 *
 * To roll out a new wave of defaults later, bump [SEED_VERSION] and add the new
 * keys to [applySeed]. Previously-set keys are left alone unless explicitly listed.
 *
 * Captured from the project lead's tuned device on 2026-05-16 via
 * `adb shell run-as com.voxa.android cat /data/user_de/0/com.voxa.android/shared_prefs/com.voxa.android_preferences.xml`.
 */
object VoxaPrefSeed {

    private const val SEED_VERSION_KEY = "voxa_seed_version"
    private const val SEED_VERSION = 1

    fun applySeed(context: Context) {
        val prefs: SharedPreferences = DeviceProtectedUtils.getSharedPreferences(context)
        val existing = prefs.getInt(SEED_VERSION_KEY, 0)
        if (existing >= SEED_VERSION) return

        prefs.edit().apply {
            // ── Typing intelligence ───────────────────────────────────
            putBoolean("more_auto_correction", true)            // aggressive autocorrect
            putBoolean("suggest_punctuation", false)            // no punct in strip
            putBoolean("block_potentially_offensive", false)    // allow all suggestions

            // ── Feedback ──────────────────────────────────────────────
            putBoolean("sound_on", true)
            putFloat("keypress_sound_volume", 0.21f)

            // ── Layout & languages ────────────────────────────────────
            putBoolean("show_number_row", true)                 // permanent number row
            putBoolean("show_number_row_hints", true)           // dot hints under digits
            putString("layout_NUMBER_ROW", "number_row")
            putString(
                "enabled_subtypes",
                "en-GB§SupportTouchPositionCorrection,TrySuppressingImeSwitcher",
            )
            putString(
                "additional_subtypes",
                "de§KeyboardLayoutSet=MAIN:qwerty;" +
                    "en-IN§KeyboardLayoutSet=EMOJI_BOTTOM:emoji_bottom_row_with_action" +
                    "|CLIPBOARD_BOTTOM:clip_bottom_row_with_action;" +
                    "fr§KeyboardLayoutSet=MAIN:qwertz;" +
                    "hu§KeyboardLayoutSet=MAIN:qwerty",
            )
            putString(
                "selected_subtype",
                "en-GB§SupportTouchPositionCorrection,TrySuppressingImeSwitcher",
            )

            // ── Special characters ────────────────────────────────────
            putBoolean("remove_redundant_popups", true)         // tidier accent grids
            putBoolean("long_press_symbols_for_numpad", true)
            putBoolean("show_emoji_key", true)                  // dedicated 😊 key
            putBoolean("abc_after_numpad_space", true)
            putBoolean("show_emoji_descriptions", true)
            putBoolean("url_detection", true)

            // ── Toolbar ───────────────────────────────────────────────
            putString("toolbar_mode", "EXPANDABLE")
            putBoolean("auto_hide_toolbar", true)
            putBoolean("auto_show_toolbar", false)
            putBoolean("quick_pin_toolbar_keys", true)
            putBoolean("toolbar_swipe_down_to_hide", false)
            putBoolean("var_toolbar_direction", false)
            putString(
                "toolbar_keys",
                "SETTINGS:true|VOICE:true|CLIPBOARD:true|UNDO:false|REDO:false|" +
                    "SELECT_WORD:false|COPY:true|PASTE:false|LEFT:false|RIGHT:false|" +
                    "NUMPAD:false|SELECT_ALL:false|CUT:false|ONE_HANDED:false|SPLIT:false|" +
                    "INCOGNITO:false|AUTOCORRECT:false|CLEAR_CLIPBOARD:false|EMOJI:true|" +
                    "UP:false|DOWN:false|WORD_LEFT:false|WORD_RIGHT:false|PAGE_UP:false|" +
                    "PAGE_DOWN:false|FULL_LEFT:false|FULL_RIGHT:false|PAGE_START:false|" +
                    "PAGE_END:false",
            )
            putString(
                "pinned_toolbar_keys",
                "VOICE:true|COPY:false|CLIPBOARD:false|NUMPAD:false|UNDO:false|REDO:false|" +
                    "SETTINGS:false|SELECT_ALL:false|SELECT_WORD:false|CUT:false|PASTE:false|" +
                    "ONE_HANDED:false|SPLIT:false|INCOGNITO:false|AUTOCORRECT:false|" +
                    "CLEAR_CLIPBOARD:false|EMOJI:false|LEFT:false|RIGHT:false|UP:false|" +
                    "DOWN:false|WORD_LEFT:false|WORD_RIGHT:false|PAGE_UP:false|PAGE_DOWN:false|" +
                    "FULL_LEFT:false|FULL_RIGHT:false|PAGE_START:false|PAGE_END:false",
            )

            // ── Theme ─────────────────────────────────────────────────
            putString("theme_style", "Material")
            putString("icon_style", "Material")
            putString("theme_colors", "cloudy")
            putString("theme_colors_night", "ocean")
            putBoolean("theme_auto_day_night", true)
            putBoolean("theme_key_borders", true)

            // ── Layout modes ──────────────────────────────────────────
            putBoolean("split_keyboard", false)
            putBoolean("split_keyboard_landscape", false)
            putBoolean("one_handed_mode_enabled_false_false", false)

            // ── Misc ──────────────────────────────────────────────────
            putBoolean("save_subtype_per_app", false)

            // ── Bookkeeping ───────────────────────────────────────────
            putInt(SEED_VERSION_KEY, SEED_VERSION)
        }.apply()
    }
}
