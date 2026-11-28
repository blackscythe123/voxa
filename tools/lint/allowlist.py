"""Known exceptions to the guardrails, each with the reason. Keep this list short: fix the code when
you can, and delete an entry as soon as its code is fixed (the runner warns about entries that no
longer match anything).

An entry matches a violation with the same rule and file whose key (shown with --keys) contains
`match`.
"""

from voxa_lint.core import Allow

ALLOW = [
    # ── 1. contrast ─────────────────────────────────────────────────────────────────────────────
    Allow("contrast", "desktop/app/src/home.rs", "app_badge() text #FFFFFF on computed background",
          "letter badges in each app's own brand colour (WhatsApp, Gmail, ...): white on several of them is under 4.5:1. "
          "Brand marks are a WCAG 1.4.3 logotype exception, but these letters are not the real logos: owner decision "
          "pending (pick the letter colour by contrast, as KbColors.micIcon does)."),
    Allow("contrast", "desktop/app/src/home.rs", "settings() text #FFFFFF on 0x10A37F",
          "Settings account avatar fallback: white initial on ChatGPT green is 3.2:1 at 20 px. Real miss; home.rs is being "
          "edited by the Home greeting work, fix there (darker green such as #0B7A5E, or near-black text)."),
    Allow("contrast", "android/app/src/main/java/com/voxa/android/ui/Theme.kt", "LightVx (light) text40 icon/",
          "text40 tints only decorative chevrons, the empty-History glyph and disabled icons: WCAG 1.4.11 exempts "
          "decoration and inactive components. Use text72 or muted for any icon that carries meaning."),

    # ── 8. robustness ───────────────────────────────────────────────────────────────────────────
    Allow("robust-atomic-write", "desktop/app/src/main.rs", "save_account:",
          "account.json (name, email, picture URL) is written with a bare fs::write; a crash mid-write leaves it empty and "
          "Settings shows no account until the next check. Should use store.rs's writer; main.rs is in the bug-fix area."),
    Allow("robust-atomic-write", "desktop/app/src/notify.rs", "save:",
          "notify.json (notification pacing) is written with a bare fs::write; a torn file only resets pacing. Should use "
          "the atomic writer; notify.rs is in the desktop bug-fix area."),
    Allow("robust-atomic-write", "desktop/app/src/store.rs", "write_atomic: fixed temp",
          "same fixed .tmp name as voxa-store; the desktop's single-instance lock keeps one writer today. Switch to unique names."),
    Allow("robust-atomic-write", "desktop/app/src/common.rs", "write_png: fixed temp",
          "website icon cache, not user data: a raced icon only falls back to the letter badge until the next fetch."),
]
