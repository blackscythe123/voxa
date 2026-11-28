//! Every colour the desktop app draws, in a light and a dark palette (docs/desktop-dark-mode.md has
//! the contrast of each pair). Pages read the current palette with `p()`; `apply` picks it from
//! the Appearance setting and, for "Use system setting", from the OS.
//!
//! Values are 0xRRGGBB for `rgb()` and 0xRRGGBBAA for `rgba()` (the fields ending in `_a`).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::store::Appearance;

pub struct Palette {
    pub dark: bool,
    /// Titles and body text.
    pub text: u32,
    /// Secondary text (times, captions, hints): AAA on the page and on cards.
    pub muted: u32,
    /// Running text a step softer than `text` (status lines, tour copy).
    pub body: u32,
    pub page: u32,
    pub nav: u32,
    pub card: u32,
    /// Hairline round cards and InfoBars.
    pub card_border_a: u32,
    /// Buttons, chips, the tour card.
    pub line_a: u32,
    /// Inputs, drop-downs, keycaps, the strip.
    pub line_input_a: u32,
    /// Tooltips.
    pub line_strong_a: u32,
    /// The search box at rest.
    pub line_soft_a: u32,
    /// The big shortcut keys on Home.
    pub key_line_a: u32,
    /// The sepia: links, selected marks, primary buttons, the ink line.
    pub accent: u32,
    pub on_accent: u32,
    pub accent_hover: u32,
    /// A sepia wash behind an accent icon (Words).
    pub accent_wash: u32,
    /// Hover wash over a chart bar.
    pub accent_hover_a: u32,
    /// Text-selection fill in inputs.
    pub selection_a: u32,
    /// Hover on a white button or a list row.
    pub hover: u32,
    /// Hover over a transparent icon button.
    pub hover_a: u32,
    /// Selected nav item, and hover on the others.
    pub nav_sel_a: u32,
    pub nav_hover_a: u32,
    /// Toggle: the knob when off, and the outline of the off track.
    pub toggle_knob_off: u32,
    pub toggle_line_a: u32,
    /// Progress tracks (playback, app shares) and the tour's steps still to come.
    pub track: u32,
    pub track_strong: u32,
    /// Insights chart: grid, the zero line, ordinary bars.
    pub grid: u32,
    pub grid_zero: u32,
    pub bar: u32,
    /// Success: icons, and success text.
    pub good: u32,
    pub good_text: u32,
    /// The practice box once it heard you.
    pub good_line_a: u32,
    pub warn_text: u32,
    /// A failed dictation's note, the error glyph.
    pub error: u32,
    /// Delete icon button.
    pub danger_text: u32,
    /// Destructive buttons, with white text in both themes.
    pub danger: u32,
    pub danger_hover: u32,
    pub on_danger: u32,
    /// InfoBars: background, the round glyph, the mark inside it.
    pub error_bar: u32,
    pub error_glyph: u32,
    pub warn_bar: u32,
    pub warn_glyph: u32,
    pub bar_mark: u32,
    /// Account pills (plan, signed in).
    pub plan_bg: u32,
    pub plan_text: u32,
    pub signed_bg: u32,
    pub signed_text: u32,
    /// The tour's dimmed backdrop.
    pub scrim_a: u32,
    /// The recording strip.
    pub strip_a: u32,
    pub strip_muted: u32,
    pub strip_paused: u32,
    pub strip_rec: u32,
    /// The quill's body on the ink line.
    pub pen: u32,
    /// The window's caption bar (Windows): its colour and its text.
    pub caption: u32,
    pub caption_text: u32,
}

pub const LIGHT: Palette = Palette {
    dark: false,
    text: 0x1A1A1A,
    muted: 0x555555,
    body: 0x3D3D3D,
    page: 0xFBFAF8,
    nav: 0xF1EEEA,
    card: 0xFFFFFF,
    card_border_a: 0x0000000F,
    line_a: 0x0000001A,
    line_input_a: 0x00000020,
    line_strong_a: 0x00000026,
    line_soft_a: 0x00000014,
    key_line_a: 0x0000002A,
    accent: 0x7A3F22,
    on_accent: 0xFFFFFF,
    accent_hover: 0x66341C,
    accent_wash: 0xF3E7DE,
    accent_hover_a: 0x7A3F220D,
    selection_a: 0x7A3F2238,
    hover: 0xF6F4F1,
    hover_a: 0x0000000F,
    nav_sel_a: 0x0000000E,
    nav_hover_a: 0x00000009,
    toggle_knob_off: 0x5F5F5F,
    toggle_line_a: 0x0000008A,
    track: 0xF0EBE5,
    track_strong: 0xE7E1DA,
    grid: 0xEFEAE4,
    grid_zero: 0xCFC6BD,
    bar: 0xDCCBBE,
    good: 0x0F7B0F,
    good_text: 0x0E5C0E,
    good_line_a: 0x0F7B0F66,
    warn_text: 0x9D5D00,
    error: 0xC42B1C,
    danger_text: 0xB3241B,
    danger: 0xC42B1C,
    danger_hover: 0xA32416,
    on_danger: 0xFFFFFF,
    error_bar: 0xFDE7E9,
    error_glyph: 0xC42B1C,
    warn_bar: 0xFFF4CE,
    warn_glyph: 0x9D5D00,
    bar_mark: 0xFFFFFF,
    plan_bg: 0xE8F5EF,
    plan_text: 0x0E6B4F,
    signed_bg: 0xDFF6DD,
    signed_text: 0x0E5C0E,
    scrim_a: 0x1A120C8C,
    strip_a: 0xFBFAF8F2,
    strip_muted: 0x555555,
    strip_paused: 0x8A8A8A,
    strip_rec: 0xE5483D,
    pen: 0x2D2418,
    caption: 0xF1EEEA,
    caption_text: 0x1A1A1A,
};

/// Warm dark neutrals from the v7 mock-up (docs/ui-mocks/v7-home-greeting-and-splash.html,
/// "Desktop"), with the muted text and the sepia lifted until both clear their targets on the
/// lighter desktop card (#2D2A28): muted AAA, sepia above 6.3:1.
pub const DARK: Palette = Palette {
    dark: true,
    text: 0xF2F0ED,
    muted: 0xC2BCB5,
    body: 0xDAD5CF,
    page: 0x242120,
    nav: 0x1C1A18,
    card: 0x2D2A28,
    card_border_a: 0xFFFFFF12,
    line_a: 0xFFFFFF1F,
    line_input_a: 0xFFFFFF29,
    line_strong_a: 0xFFFFFF33,
    line_soft_a: 0xFFFFFF1A,
    key_line_a: 0xFFFFFF2E,
    accent: 0xE0A07C,
    on_accent: 0x24140C,
    accent_hover: 0xEAB596,
    accent_wash: 0x45322A,
    accent_hover_a: 0xE0A07C17,
    selection_a: 0xE0A07C4D,
    hover: 0x363230,
    hover_a: 0xFFFFFF14,
    nav_sel_a: 0xFFFFFF0F,
    nav_hover_a: 0xFFFFFF0A,
    toggle_knob_off: 0xC2BCB5,
    toggle_line_a: 0xFFFFFF8A,
    track: 0x3D3936,
    track_strong: 0x4A4541,
    grid: 0x3A3633,
    grid_zero: 0x5C5550,
    bar: 0x9A6F57,
    good: 0x45C27C,
    good_text: 0x45C27C,
    good_line_a: 0x45C27C80,
    warn_text: 0xE3A44A,
    error: 0xF07A70,
    danger_text: 0xF07A70,
    danger: 0xC42B1C,
    danger_hover: 0xD6392A,
    on_danger: 0xFFFFFF,
    error_bar: 0x442726,
    error_glyph: 0xFF99A4,
    warn_bar: 0x433519,
    warn_glyph: 0xE3A44A,
    bar_mark: 0x2A1A14,
    plan_bg: 0x1F3A2C,
    plan_text: 0x7FD9A8,
    signed_bg: 0x23391F,
    signed_text: 0x8FD98A,
    scrim_a: 0x0000009E,
    strip_a: 0x2A2725F2,
    strip_muted: 0xC2BCB5,
    strip_paused: 0x9A948E,
    strip_rec: 0xF2555A,
    pen: 0xEDE4DA,
    caption: 0x1C1A18,
    caption_text: 0xF2F0ED,
};

static DARK_ON: AtomicBool = AtomicBool::new(false);

/// The palette in use in this process.
pub fn p() -> &'static Palette {
    if DARK_ON.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

/// Whether the setting, with the OS in `os_dark`, means dark.
pub fn wants_dark(choice: Appearance, os_dark: bool) -> bool {
    match choice {
        Appearance::System => os_dark,
        Appearance::Light => false,
        Appearance::Dark => true,
    }
}

/// Picks the palette; true when it changed (the caller repaints).
pub fn apply(choice: Appearance, os_dark: bool) -> bool {
    let dark = wants_dark(choice, os_dark);
    DARK_ON.swap(dark, Ordering::Relaxed) != dark
}

pub fn os_is_dark(a: gpui::WindowAppearance) -> bool {
    matches!(a, gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: u32) -> f64 {
        let ch = |v: u32| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * ch(c >> 16 & 0xFF) + 0.7152 * ch(c >> 8 & 0xFF) + 0.0722 * ch(c & 0xFF)
    }
    fn ratio(a: u32, b: u32) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
    /// 0xRRGGBBAA over an opaque 0xRRGGBB.
    fn over(fg_a: u32, bg: u32) -> u32 {
        let a = (fg_a & 0xFF) as f64 / 255.0;
        let mix = |s: u32| {
            let (f, b) = ((fg_a >> (s + 8) & 0xFF) as f64, (bg >> s & 0xFF) as f64);
            ((f * a + b * (1.0 - a)).round() as u32) << s
        };
        mix(16) | mix(8) | mix(0)
    }

    /// The pairs in docs/desktop-dark-mode.md, checked so a palette edit can't quietly break them.
    #[test]
    fn text_and_components_meet_their_contrast_targets() {
        for t in [&LIGHT, &DARK] {
            let name = if t.dark { "dark" } else { "light" };
            let strip = over(t.strip_a, t.page);
            // Body text: AAA.
            for (what, fg, bg) in [("text/page", t.text, t.page), ("text/nav", t.text, t.nav), ("text/card", t.text, t.card), ("body/card", t.body, t.card), ("text/strip", t.text, strip), ("text/error bar", t.text, t.error_bar), ("text/warn bar", t.text, t.warn_bar), ("text/hover", t.text, t.hover)] {
                assert!(ratio(fg, bg) >= 7.0, "{name} {what}: {:.2}", ratio(fg, bg));
            }
            // Muted text: AAA on the page, cards and the strip.
            for (what, fg, bg) in [("muted/page", t.muted, t.page), ("muted/card", t.muted, t.card), ("muted/strip", t.strip_muted, strip)] {
                assert!(ratio(fg, bg) >= 7.0, "{name} {what}: {:.2}", ratio(fg, bg));
            }
            // Accent text, status text, button text: AA at least.
            for (what, fg, bg) in [
                ("accent/page", t.accent, t.page),
                ("accent/card", t.accent, t.card),
                ("accent/nav", t.accent, t.nav),
                ("on accent", t.on_accent, t.accent),
                ("on accent hover", t.on_accent, t.accent_hover),
                ("on danger", t.on_danger, t.danger),
                ("on danger hover", t.on_danger, t.danger_hover),
                ("good/card", t.good_text, t.card),
                ("good/page", t.good_text, t.page),
                ("warn/card", t.warn_text, t.card),
                ("warn/page", t.warn_text, t.page),
                ("error/card", t.error, t.card),
                ("danger/card", t.danger_text, t.card),
                ("plan pill", t.plan_text, t.plan_bg),
                ("signed pill", t.signed_text, t.signed_bg),
                ("warn/strip", t.warn_text, strip),
                ("muted/accent wash", t.muted, t.accent_wash),
            ] {
                assert!(ratio(fg, bg) >= 4.5, "{name} {what}: {:.2}", ratio(fg, bg));
            }
            // Icons and component edges: 3:1.
            for (what, fg, bg) in [
                ("good icon/card", t.good, t.card),
                ("error glyph/bar", t.error_glyph, t.error_bar),
                ("warn glyph/bar", t.warn_glyph, t.warn_bar),
                ("mark/error glyph", t.bar_mark, t.error_glyph),
                ("mark/warn glyph", t.bar_mark, t.warn_glyph),
                ("toggle knob off/card", t.toggle_knob_off, t.card),
                ("toggle track off/card", over(t.toggle_line_a, t.card), t.card),
                ("toggle knob on/accent", t.on_accent, t.accent),
                ("accent icon/wash", t.accent, t.accent_wash),
                ("bar emphasised/card", t.accent, t.card),
                ("rec dot/strip", t.strip_rec, strip),
                ("paused glyph/strip", t.strip_paused, strip),
                ("good/strip", t.good, strip),
                ("error/strip", t.error, strip),
                ("accent/strip", t.accent, strip),
                ("pen/strip", t.pen, strip),
                ("caret/card", t.accent, t.card),
            ] {
                assert!(ratio(fg, bg) >= 3.0, "{name} {what}: {:.2}", ratio(fg, bg));
            }
        }
    }

    #[test]
    fn the_setting_overrides_the_system() {
        assert!(wants_dark(Appearance::System, true));
        assert!(!wants_dark(Appearance::System, false));
        assert!(!wants_dark(Appearance::Light, true));
        assert!(wants_dark(Appearance::Dark, false));
    }
}
