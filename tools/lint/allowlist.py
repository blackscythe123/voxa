"""Known exceptions to the guardrails, each with the reason. Keep this list short: fix the code when
you can, and delete an entry as soon as its code is fixed (the runner warns about entries that no
longer match anything).

An entry matches a violation with the same rule and file whose key (shown with --keys) contains
`match`.
"""

from voxa_lint.core import Allow

ALLOW = [
    # ── 1. contrast ─────────────────────────────────────────────────────────────────────────────
    Allow("contrast", "android/app/src/main/java/com/voxa/android/ui/Theme.kt", "LightVx (light) text40 icon/",
          "text40 tints only decorative chevrons, the empty-History glyph and disabled icons: WCAG 1.4.11 exempts "
          "decoration and inactive components. Use text72 or muted for any icon that carries meaning."),

    # ── 8. robustness ───────────────────────────────────────────────────────────────────────────
    Allow("robust-atomic-write", "desktop/app/src/common.rs", "write_png: fixed temp",
          "website icon cache, not user data: a raced icon only falls back to the letter badge until the next fetch."),

]
