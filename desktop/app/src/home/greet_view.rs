//! Home's header and the written splash in the Voxa window (docs/ui-mocks/
//! v7-home-greeting-and-splash.html, section 3).
//!
//! The header: a doodle band across the page with the subject on the right, then the greeting at
//! 40 on one line. The doodle draws itself once per doodle per day and is drawn finished while an
//! InfoBar shows or with reduced motion.
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

use super::{Home, MUTED, PAGE_BG, TEXT};
use crate::INK;
use crate::greet;
use crate::pen::{self, Art, Splash, SplashColors};

/// The band's height and the greeting's size, as in the mockup.
const BAND_H: f32 = 112.0;

pub(super) struct Greet {
    header: greet::Header,
    /// The drawing for the band's current width.
    art: Rc<RefCell<Option<(f32, Rc<Art>)>>>,
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
    pub(super) fn new(entries: &[crate::store::Entry], name: Option<String>) -> Self {
        let header = greet::header(entries, name);
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
        let (entries, account) = {
            let v = self.voxa.read(cx);
            (v.history.entries.clone(), v.account())
        };
        let first = account.as_ref().and_then(|a| a.name.split_whitespace().next().map(str::to_string)).filter(|n| !n.is_empty());
        self.greet = Some(Greet::new(&entries, first));
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
        let band = pen::doodle(
            move |w| {
                let mut c = cache.borrow_mut();
                if c.as_ref().is_none_or(|(cw, _)| (cw - w).abs() > 0.5) {
                    *c = Some((w, Rc::new(Art::new(greeting.doodle, greet::drawing(&greeting, w)))));
                }
                c.as_ref().map(|(_, a)| a.clone()).unwrap_or_else(|| Rc::new(Art::new(greeting.doodle, greet::drawing(&greeting, w))))
            },
            t.min(1e9),
            rgb(INK).into(),
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
        let colors = SplashColors { paper: rgb(PAGE_BG).into(), ink: rgb(TEXT).into(), nib: rgb(INK).into(), desc: rgb(MUTED).into() };
        let art = pen::splash(s.pen.clone(), if s.reduced { None } else { Some(t.min(sp::TOTAL_MS)) }, colors);
        let shift = (0.62 - 0.5) * (sp::BOX_H + 40.0) * pen::SPLASH_SCALE * 2.0;
        Some(
            div()
                .id("splash")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgb(PAGE_BG))
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
