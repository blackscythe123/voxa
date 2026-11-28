//! Home's header and the written splash in the Voxa window (docs/ui-mocks/
//! v7-home-greeting-and-splash.html, section 3).
//!
//! The header (locked by the owner on 2026-11-28, docs/ui-mocks/v7-home-fullscreen-compare.html,
//! "Desktop, new"): the greeting on the left and the doodle on the right of one row, both standing
//! on one ink line across the content. The doodle is cropped to its own drawn bounds (every stroke
//! but the long horizon, plus [`CROP_PAD`]) so none is cut off and none carries empty margins; it
//! is about 150 px tall at a 1280 px window, its width capped near 420 px there (a wide doodle
//! shrinks instead of pushing the greeting onto two lines). No fixed height: the row is as tall as
//! the taller of the two. A window too narrow for even a small greeting beside the doodle falls
//! back to the band above the greeting ([`hero_layout`] returns None).
//! The doodle draws itself once per doodle per day and is drawn finished while an InfoBar shows or
//! with reduced motion.
//!
//! The splash: only on a cold open of the window by the user (the tray passes `--splash` the
//! first time it opens the window in a run), inside the window's own content area, never over
//! other apps and never taking focus. Skipped if the window isn't the active window; any click or
//! key skips to Home with the same 300 ms crossfade. Reduced motion: the still wordmark for 400 ms.
//! Animation frames are requested only while something moves.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px, rgb};
use voxa_core::splash as sp;

use super::Home;
use crate::theme::p;
use crate::greet;
use crate::pen::{self, Art, Splash, SplashColors};

/// The band's height, as in the mockup (the stacked fallback).
const BAND_H: f32 = 112.0;
/// The nav column and the page's side padding (home.rs render): the content width is the
/// window's width less these.
const NAV_W: f32 = 240.0;
const PAGE_PAD_X: f32 = 80.0;
/// The hero at a 1280 px window (content 960): the doodle 150 tall and at most 420 wide, the
/// greeting at 46; both grow with the content, up to the max.
const REF_CONTENT: f32 = 1280.0 - NAV_W - PAGE_PAD_X;
const DOODLE_H: f32 = 150.0;
const DOODLE_H_MAX: f32 = 240.0;
const DOODLE_W_CAP: f32 = 420.0;
const TEXT: f32 = 46.0;
const TEXT_MAX: f32 = 64.0;
/// Below this the greeting would be too small to read as a greeting: stack instead.
const TEXT_MIN: f32 = 30.0;
/// Space between the greeting's end and the doodle.
const GAP: f32 = 32.0;
/// Padding around the doodle's drawn bounds, in band units.
pub(super) const CROP_PAD: f32 = 4.0;
/// The ground the doodles stand on, in band units.
const GROUND_Y: f32 = 92.0;

/// The doodle's drawn bounds (x0, y0, x1, y1, band units) plus [`CROP_PAD`]: every stroke, wash
/// and dot except the long horizon.
pub(super) fn crop(d: &voxa_core::doodle::Drawing) -> [f32; 4] {
    use voxa_core::svgpath;
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for s in d.strokes.iter().filter(|s| !pen::is_horizon(s)) {
        for (x, y) in svgpath::flatten(&s.segs(), 0.25).into_iter().flatten() {
            b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        }
    }
    [b[0] - CROP_PAD, b[1] - CROP_PAD, b[2] + CROP_PAD, b[3].max(GROUND_Y) + CROP_PAD]
}

/// The hero's sizes for a content `content_w` px wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Hero {
    /// The doodle's scale (px per band unit).
    pub k: f32,
    /// The greeting's font size.
    pub text: f32,
}

/// Lays out the hero, or None when even a small greeting won't fit beside the doodle (stacked).
/// `crop`: the doodle's cropped box in band units.
pub(super) fn hero_layout(content_w: f32, greeting: &str, crop: [f32; 4]) -> Option<Hero> {
    let (cw, ch) = (crop[2] - crop[0], crop[3] - crop[1]);
    if content_w <= 0.0 || cw <= 0.0 || ch <= 0.0 || greeting.is_empty() {
        return None;
    }
    let s = content_w / REF_CONTENT;
    let k = ((DOODLE_H * s).clamp(DOODLE_H, DOODLE_H_MAX) / ch).min(DOODLE_W_CAP * s / cw);
    let room = content_w - cw * k - GAP;
    let mut text = (TEXT * s).clamp(TEXT, TEXT_MAX);
    let w = text_w(greeting, text);
    if w > room {
        text *= room / w;
    }
    (text >= TEXT_MIN).then_some(Hero { k, text })
}

/// The greeting's width at `size` px, Segoe UI Semibold, generous so a long name never runs into
/// the doodle (wide letters average about 0.6 em).
pub(super) fn text_w(s: &str, size: f32) -> f32 {
    s.chars().map(|c| if c.is_uppercase() || "mwMW".contains(c) { 0.72 } else if " ,.'ilj".contains(c) { 0.3 } else { 0.56 }).sum::<f32>() * size
}

pub(super) struct Greet {
    header: greet::Header,
    /// The drawing for the band's current width, and whether it was placed for the hero.
    art: Rc<RefCell<Option<(f32, bool, Rc<Art>)>>>,
    /// When the drawing started (None: not started, or drawn finished).
    t0: Option<Instant>,
    still: bool,
}

pub(super) struct SplashRun {
    pen: Rc<Splash>,
    opened: Instant,
    t0: Option<Instant>,
    leaving: Option<Instant>,
    reduced: bool,
}

impl Greet {
    pub(super) fn new(entries: &[crate::store::Entry], name: Option<String>, birthday: Option<voxa_core::greeting::MonthDay>) -> Self {
        let header = greet::header(entries, name, birthday);
        Self { header, art: Default::default(), t0: None, still: false }
    }
}

impl SplashRun {
    pub(super) fn new() -> Self {
        Self { pen: Rc::new(Splash::default()), opened: Instant::now(), t0: None, leaving: None, reduced: greet::reduced_motion() }
    }
}

fn ms_since(t: Instant) -> f32 {
    t.elapsed().as_secs_f32() * 1000.0
}

impl Home {
    /// The tray opened the window cold: play the splash.
    pub fn start_splash(&mut self, cx: &mut Context<Self>) {
        self.splash = Some(SplashRun::new());
        cx.notify();
    }

    /// Pick the greeting and its doodle (when Home opens).
    pub(super) fn pick_greeting(&mut self, cx: &mut Context<Self>) {
        let (entries, account, birthday) = {
            let v = self.voxa.read(cx);
            (v.history.entries.clone(), v.account(), v.settings.birthday())
        };
        let first = account.as_ref().and_then(|a| a.name.split_whitespace().next().map(str::to_string)).filter(|n| !n.is_empty());
        self.greet = Some(Greet::new(&entries, first, birthday));
    }

    /// Any click or key during the splash: straight to the crossfade.
    pub(super) fn skip_splash(&mut self, cx: &mut Context<Self>) -> bool {
        match &mut self.splash {
            Some(s) if s.leaving.is_none() => {
                s.leaving = Some(Instant::now());
                cx.notify();
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// The doodle band and the greeting. `problem`: an InfoBar is showing, so nothing moves.
    pub(super) fn greeting_header(&mut self, problem: bool, cx: &mut Context<Self>) -> AnyElement {
        if self.greet.is_none() {
            self.pick_greeting(cx);
        }
        let splashing = self.splash.is_some();
        let reduced = self.splash.as_ref().map(|s| s.reduced).unwrap_or_else(greet::reduced_motion);
        let account = self.voxa.read(cx).account();
        let first = account.as_ref().and_then(|a| a.name.split_whitespace().next().map(str::to_string)).filter(|n| !n.is_empty());
        let Some(g) = self.greet.as_mut() else { return div().into_any_element() };
        let line = match &first {
            Some(n) => format!("{}, {n}", g.header.greeting.lead),
            None => g.header.greeting.lead.clone(),
        };
        // Start the drawing once the splash is gone; finished at once when there's a problem.
        if !g.still && (problem || reduced || !g.header.play) {
            g.still = true;
            g.t0 = None;
        }
        if !g.still && g.t0.is_none() && !splashing {
            g.t0 = Some(Instant::now());
            greet::played(&g.header.greeting);
        }
        let t = match greet::test_freeze("VOXA_TEST_DOODLE_AT") {
            Some(t) => t,
            None => match (g.still, g.t0) {
                (false, Some(t0)) => ms_since(t0),
                (false, None) => 0.0,
                _ => f32::MAX,
            },
        };
        let cache = g.art.clone();
        let greeting = g.header.greeting.clone();
        // The hero: the phone band's drawing (no shift), cropped to its drawn bounds.
        let content_w = (self.view_w - NAV_W - PAGE_PAD_X).max(0.0);
        let art = {
            let mut c = cache.borrow_mut();
            if c.as_ref().is_none_or(|(w, hero, _)| *w != voxa_core::doodle::BAND_W || !*hero) {
                *c = Some((voxa_core::doodle::BAND_W, true, Rc::new(Art::new(greeting.doodle, greet::drawing(&greeting, voxa_core::doodle::BAND_W)))));
            }
            c.as_ref().map(|(_, _, a)| a.clone()).expect("just set")
        };
        let bounds = crop(&art.drawing);
        if let Some(h) = hero_layout(content_w, &line, bounds) {
            let ink = rgb(p().accent);
            // The line sits where the doodle stands (y 92); the greeting's descenders clear it.
            let line_y = (bounds[3] - GROUND_Y) * h.k;
            let mut ground: gpui::Hsla = ink.into();
            ground.a = voxa_core::doodle::ground_alpha(p().dark);
            return div()
                .id("hero")
                .relative()
                .pt(px(8.))
                .mb(px(13.))
                .flex()
                .items_end()
                .justify_between()
                .gap(px(GAP))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .pb(px(line_y + 6.0))
                        .text_size(px(h.text))
                        .line_height(px(h.text * 1.2))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(line),
                )
                .child(pen::doodle_fit(art, t.min(1e9), ink.into(), false, h.k, bounds))
                .child(div().absolute().left_0().right_0().bottom(px(line_y - 0.75)).h(px(1.5)).bg(ground))
                .into_any_element();
        }
        let band = pen::doodle(
            move |w| {
                let mut c = cache.borrow_mut();
                if c.as_ref().is_none_or(|(cw, hero, _)| (cw - w).abs() > 0.5 || *hero) {
                    *c = Some((w, false, Rc::new(Art::new(greeting.doodle, greet::drawing(&greeting, w)))));
                }
                c.as_ref().map(|(_, _, a)| a.clone()).unwrap_or_else(|| Rc::new(Art::new(greeting.doodle, greet::drawing(&greeting, w))))
            },
            t.min(1e9),
            rgb(p().accent).into(),
            false,
            BAND_H,
        );
        div()
            .relative()
            .child(div().mb(px(4.)).child(band))
            .child(div().text_size(px(40.)).line_height(px(48.)).font_weight(FontWeight::SEMIBOLD).whitespace_nowrap().overflow_hidden().text_ellipsis().child(line))
            .into_any_element()
    }

    /// Whether the doodle is still drawing (so the window keeps asking for frames).
    pub(super) fn doodle_moving(&self) -> bool {
        let done = 1400.0;
        self.greet.as_ref().is_some_and(|g| !g.still && g.t0.is_none_or(|t0| ms_since(t0) < done + 50.0)) && greet::test_freeze("VOXA_TEST_DOODLE_AT").is_none()
    }

    /// The splash over the whole content area, while it runs. Returns None once it's over.
    pub(super) fn splash_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.splash.as_mut()?;
        let freeze = greet::test_freeze("VOXA_TEST_SPLASH_AT");
        // Only when the user is looking: if the window didn't become active, skip to Home.
        if s.t0.is_none() {
            if window.is_window_active() || freeze.is_some() {
                s.t0 = Some(Instant::now());
            } else if ms_since(s.opened) > 200.0 && s.leaving.is_none() {
                s.leaving = Some(Instant::now());
            }
        }
        let hold = if s.reduced { sp::REDUCED_HOLD_MS } else { sp::TOTAL_MS };
        let t = freeze.unwrap_or_else(|| s.t0.map(ms_since).unwrap_or(0.0));
        if s.leaving.is_none() && t >= hold && freeze.is_none() {
            s.leaving = Some(Instant::now());
        }
        let fade = s.leaving.map(|l| sp::standard((ms_since(l) / sp::CROSSFADE_MS).min(1.0))).unwrap_or(0.0);
        if fade >= 1.0 {
            self.splash = None;
            cx.notify();
            return None;
        }
        window.request_animation_frame();
        // Dark: the icon's tile, light ink and the copper nib (approved splash B); light: the page.
        let (paper, ink, nib) = if p().dark { (0x211B17, 0xF2EDE8, 0xD99366) } else { (p().page, p().text, p().accent) };
        let colors = SplashColors { paper: rgb(paper).into(), ink: rgb(ink).into(), nib: rgb(nib).into(), desc: rgb(p().muted).into() };
        let art = pen::splash(s.pen.clone(), if s.reduced { None } else { Some(t.min(sp::TOTAL_MS)) }, colors);
        let shift = (0.62 - 0.5) * (sp::BOX_H + 40.0) * pen::SPLASH_SCALE * 2.0;
        Some(
            div()
                .id("splash")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgb(if p().dark { 0x211B17 } else { p().page }))
                .opacity(1.0 - fade)
                .flex()
                .items_center()
                .justify_center()
                .pb(px(shift))
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.skip_splash(cx);
                }))
                .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| {
                    this.skip_splash(cx);
                }))
                .child(art)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxa_core::doodle;
    use voxa_core::greeting::{Date, Doodle};
    use voxa_core::svgpath;

    /// Every doodle as Home can draw it: the 11 kinds, the streak tally at 3 and at 30 marks (the
    /// widest), on a few dates (the sky's variations and the moon move with the date).
    fn every_doodle() -> Vec<(String, doodle::Drawing)> {
        let mut out = Vec::new();
        for date in [Date { y: 2026, m: 11, d: 28 }, Date { y: 2027, m: 3, d: 14 }, Date { y: 2027, m: 7, d: 2 }] {
            for d in Doodle::ALL {
                for n in if d == Doodle::Streak { vec![3, 30] } else { vec![5] } {
                    out.push((format!("{d:?} {n} on {}-{}-{}", date.y, date.m, date.d), doodle::draw(d, date, n, doodle::BAND_W, 0.0)));
                }
            }
        }
        out
    }

    /// Locked layout rule 2: each doodle is cropped to its own drawn bounds, so none is cut off
    /// (earlier fixed boxes cut the evening hills, the night's stars, the weekend mug and book and
    /// the 30 day tally in half) and none carries empty margins.
    #[test]
    fn every_doodle_is_cropped_to_its_own_drawn_bounds() {
        for (name, d) in every_doodle() {
            assert_eq!(d.strokes.iter().filter(|s| pen::is_horizon(s)).count(), 1, "{name}: one long horizon, left out of the crop");
            let c = crop(&d);
            let (mut x0, mut y0, mut x1) = (f32::MAX, f32::MAX, f32::MIN);
            for s in d.strokes.iter().filter(|s| !pen::is_horizon(s)) {
                for (x, y) in svgpath::flatten(&s.segs(), 0.25).into_iter().flatten() {
                    assert!(x >= c[0] + CROP_PAD - 0.01 && x <= c[2] - CROP_PAD + 0.01 && y >= c[1] + CROP_PAD - 0.01 && y <= c[3] - CROP_PAD + 0.01, "{name}: ({x}, {y}) outside {c:?}");
                    (x0, y0, x1) = (x0.min(x), y0.min(y), x1.max(x));
                }
            }
            assert!((c[0] - (x0 - CROP_PAD)).abs() < 0.01 && (c[1] - (y0 - CROP_PAD)).abs() < 0.01 && (c[2] - (x1 + CROP_PAD)).abs() < 0.01, "{name}: no empty margin");
            assert!(c[3] >= GROUND_Y + CROP_PAD - 0.01, "{name}: the doodle's ground is inside the box");
        }
    }

    /// Locked layout rules 3 and 1: about 150 px tall at a 1280 px window, never wider than the
    /// cap (the widest doodle shrinks instead), and the greeting stays on one line beside it.
    #[test]
    fn the_hero_caps_the_doodle_and_keeps_the_greeting_on_one_line() {
        let content = |w: f32| w - NAV_W - PAGE_PAD_X;
        let leads = ["Good afternoon", "Saturday morning", "Happy New Year", "One year with Voxa", "Nearly weekend", "First words", "Happy birthday", "Early start"];
        let mut widest = (String::new(), 0.0f32);
        for (name, d) in every_doodle() {
            let c = crop(&d);
            for lead in leads {
                for who in ["Sam", "Alexandria"] {
                    let g = format!("{lead}, {who}");
                    let h = hero_layout(content(1280.0), &g, c).unwrap_or_else(|| panic!("{name} at 1280 px: side by side for {g}"));
                    let (w, ht) = ((c[2] - c[0]) * h.k, (c[3] - c[1]) * h.k);
                    assert!(w <= DOODLE_W_CAP + 0.01, "{name}: {w} px wide at 1280");
                    assert!(ht <= DOODLE_H + 0.01, "{name}: {ht} px tall at 1280");
                    assert!(text_w(&g, h.text) + GAP + w <= content(1280.0) + 0.01, "{name}: {g} on one line");
                    if w > widest.1 {
                        widest = (name.clone(), w);
                    }
                    if (c[2] - c[0]) / (c[3] - c[1]) < DOODLE_W_CAP / DOODLE_H {
                        assert!((ht - DOODLE_H).abs() < 0.01, "{name}: 150 px tall when not capped");
                    }
                }
            }
        }
        assert!((widest.1 - DOODLE_W_CAP).abs() < 0.01, "the widest doodle ({}) is held at the cap, not past it", widest.0);
        let tally = crop(&doodle::draw(Doodle::Streak, Date { y: 2026, m: 11, d: 28 }, 30, doodle::BAND_W, 0.0));
        let h = hero_layout(content(1280.0), "30 days running, Sam", tally).unwrap();
        assert!((tally[2] - tally[0]) * h.k <= DOODLE_W_CAP + 0.01, "the 30 day tally fits the cap");
        // Wider windows grow the hero; a window too narrow for a readable greeting stacks.
        let big = hero_layout(content(1600.0), "Happy birthday, Sam", crop(&doodle::draw(Doodle::Birthday, Date { y: 2026, m: 11, d: 28 }, 0, doodle::BAND_W, 0.0))).unwrap();
        assert!(big.text > TEXT && big.k > 1.0);
        assert_eq!(hero_layout(content(760.0), "Happy birthday, Alexandria", tally), None);
        assert_eq!(hero_layout(content(1280.0), "", tally), None);
    }

    /// The hero's pen reads at the band's weight, not a felt marker.
    #[test]
    fn the_hero_pen_is_not_a_felt_marker() {
        assert_eq!(pen::stroke_scale(1.0), 1.0);
        assert_eq!(pen::stroke_scale(0.5), 0.5);
        assert!(pen::stroke_scale(2.6) < 1.7);
    }
}

