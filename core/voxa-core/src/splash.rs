//! The written splash (docs/ui-mocks/v7-home-greeting-and-splash.html, "Splash: the pen writes
//! Voxa", variant B, the one the owner chose): the nib drifts in, rests on the paper, writes the
//! wordmark as one monoline stroke, lifts away, the ink settles and "Talk instead of typing" rises
//! in. 2,420 ms, then Home crossfades in over 300 ms.
//!
//! [`frame`] gives everything to draw at a moment, so Android and the desktop move the same way.

use crate::svgpath::{self, Measured};

/// The wordmark: one continuous monoline stroke in a 280 x 140 box (baseline 100).
pub const WORDMARK: &str = "M16 38 C 18 28, 30 24, 36 34 C 42 50, 52 80, 58 100 C 64 82, 74 58, 84 46 C 88 41, 94 40, 95 45 C 96 50, 92 56, 98 60 C 104 64, 112 64, 121 64 C 130 64, 136 72, 136 82 C 136 92, 130 100, 121 100 C 112 100, 106 92, 106 82 C 106 72, 112 64, 121 64 C 130 64, 140 64, 148 64 C 156 66, 160 74, 164 82 C 168 90, 174 98, 182 100 C 188 100, 192 84, 198 74 C 202 68, 208 65, 214 65 C 218 65, 222 66, 223 68 C 216 63, 200 66, 198 82 C 197 94, 204 101, 211 100 C 218 99, 222 90, 223 80 C 223 74, 223 68, 223 66 C 223 78, 222 92, 226 98 C 229 102, 236 101, 242 96 C 250 90, 252 104, 236 110 C 214 116, 176 118, 160 110 C 154 106, 154 102, 156 98 C 164 86, 172 74, 184 64 C 186 62, 190 61, 193 62";
pub const BOX_W: f32 = 280.0;
pub const BOX_H: f32 = 140.0;
/// The stroke weight once the ink has settled.
pub const INK_W: f32 = 5.2;

/// The nib from the icon, tip at 0,0 (drawn at 0.9 inside the nib transform).
pub const NIB: &str = "M-16 -53 H16 V-41 C16 -27 8 -15 0 0 C-8 -15 -16 -27 -16 -41 Z";
/// The breather hole (centre and radius), the slit (stroke 2.2) and the shoulder line (a rect),
/// all in the paper colour.
pub const NIB_HOLE: (f32, f32, f32) = (0.0, -32.0, 4.6);
pub const NIB_SLIT: &str = "M0 -27 L-2.6 -20 L2.6 -14 L-1.8 -8 L0 -1";
pub const NIB_SLIT_W: f32 = 2.2;
pub const NIB_SHOULDER: (f32, f32, f32, f32) = (-16.0, -44.0, 32.0, 2.2);
pub const NIB_INNER_SCALE: f32 = 0.9;

pub const TAGLINE: &str = "Talk instead of typing";
/// The tagline's top, below the box's top.
pub const TAGLINE_TOP: f32 = 148.0;

/// Variant B, in milliseconds.
pub const APPEAR: (f32, f32) = (0.0, 450.0);
pub const HOLD: (f32, f32) = (450.0, 570.0);
pub const WRITE: (f32, f32) = (570.0, 1870.0);
pub const LIFT: (f32, f32) = (1870.0, 2070.0);
pub const SETTLE: (f32, f32) = (1870.0, 2270.0);
pub const DESC: (f32, f32) = (2020.0, 2420.0);
pub const TOTAL_MS: f32 = 2420.0;
pub const FADE_MS: f32 = 320.0;
/// Home crossfades in over this, after [`TOTAL_MS`] (Standard easing).
pub const CROSSFADE_MS: f32 = 300.0;
/// With reduced motion: the finished wordmark, still, for this long, then the crossfade.
pub const REDUCED_HOLD_MS: f32 = 400.0;

/// A CSS cubic-bezier easing (the same as Compose's CubicBezierEasing).
pub fn cubic(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let (mut lo, mut hi, mut u) = (0.0f32, 1.0f32, t);
    for _ in 0..24 {
        u = (lo + hi) / 2.0;
        let x = 3.0 * (1.0 - u) * (1.0 - u) * u * x1 + 3.0 * (1.0 - u) * u * u * x2 + u * u * u;
        if x < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    3.0 * (1.0 - u) * (1.0 - u) * u * y1 + 3.0 * (1.0 - u) * u * u * y2 + u * u * u
}

pub fn standard(t: f32) -> f32 {
    cubic(0.4, 0.0, 0.2, 1.0, t)
}

fn decel(t: f32) -> f32 {
    cubic(0.05, 0.7, 0.1, 1.0, t)
}

fn accel(t: f32) -> f32 {
    cubic(0.3, 0.0, 0.8, 0.15, t)
}

fn drift(t: f32) -> f32 {
    cubic(0.3, 0.0, 0.15, 1.0, t)
}

/// The write: slow at first, then the pen finds its pace.
pub fn write_ease(t: f32) -> f32 {
    cubic(0.64, 0.0, 0.3, 1.0, t)
}

/// The wordmark flattened and measured once.
pub struct Wordmark {
    pub path: Measured,
}

impl Default for Wordmark {
    fn default() -> Self {
        Self::new()
    }
}

impl Wordmark {
    pub fn new() -> Self {
        let pts = svgpath::flatten(&svgpath::parse(WORDMARK), 0.25).into_iter().next().unwrap_or_default();
        Self { path: Measured::new(pts) }
    }

    pub fn length(&self) -> f32 {
        self.path.length()
    }
}

/// Everything to draw at one moment, in the wordmark's 280 x 140 box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// How much of the wordmark is inked, 0 to 1 of its length. Draw nothing when `ink_alpha` is 0.
    pub ink: f32,
    pub ink_alpha: f32,
    /// Stroke weight: a touch heavier while wet.
    pub ink_width: f32,
    /// The nib's tip, its rotation in degrees, its scale and opacity.
    pub nib_x: f32,
    pub nib_y: f32,
    pub nib_rot: f32,
    pub nib_scale: f32,
    pub nib_alpha: f32,
    /// The tagline's opacity and how far below its place it still is.
    pub desc_alpha: f32,
    pub desc_dy: f32,
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn span(t: f32, (a, b): (f32, f32)) -> f32 {
    clamp01((t - a) / (b - a))
}

/// The splash at `t` ms (variant B).
pub fn frame(w: &Wordmark, t: f32) -> Frame {
    let len_total = w.length();
    let p = write_ease(span(t, WRITE));
    let h = span(t, HOLD);
    // The nib touches the paper 40 ms into the hold and the first drop of ink appears under it.
    let touched = t >= HOLD.0 + 40.0;
    let ink = if touched && p == 0.0 { 0.3 / len_total } else { p };
    let s = span(t, SETTLE);
    let ink_width = INK_W * (1.0 + 0.07 * (1.0 - standard(s)) * if p > 0.0 || touched { 1.0 } else { 0.0 });
    let len = len_total * p;
    let pt = w.path.point_at(len);
    let a = w.path.point_at((len - 4.0).max(0.0));
    let b = w.path.point_at((len + 4.0).min(len_total));
    let dir = (b.1 - a.1).atan2(b.0 - a.0);
    // Leans with rightward travel, lifts a little on upstrokes; never spins with the line.
    let sway = 7.0 * dir.cos() - 3.0 * dir.sin();
    let ap = drift(span(t, APPEAR));
    let fade = clamp01(t / FADE_MS);
    let lf = accel(span(t, LIFT));
    // From the centre (where the system splash had its icon) to the start of the V.
    let start = w.path.point_at(0.0);
    let (cx, cy) = (BOX_W / 2.0, BOX_H / 2.0);
    let (mut x, mut y) = pt;
    if t < WRITE.0 {
        x = cx + (start.0 - cx) * ap;
        y = cy - 10.0 + (start.1 - cy + 10.0) * ap;
    }
    let press = (std::f32::consts::PI * h).sin();
    y += 1.6 * press;
    x += 12.0 * lf;
    y -= 18.0 * lf;
    let dsc = decel(span(t, DESC));
    Frame {
        ink,
        ink_alpha: if touched { 1.0 } else { 0.0 },
        ink_width,
        nib_x: x,
        nib_y: y,
        nib_rot: 35.0 + sway + 8.0 * lf - 2.0 * press,
        nib_scale: 0.9 + 0.1 * ap - 0.02 * press,
        nib_alpha: decel(fade) * (1.0 - lf),
        desc_alpha: dsc,
        desc_dy: 8.0 * (1.0 - dsc),
    }
}

/// The finished frame (reduced motion): the whole wordmark, no nib, the tagline in place.
pub fn end_frame(w: &Wordmark) -> Frame {
    Frame { nib_alpha: 0.0, ..frame(w, TOTAL_MS) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beats_land_on_time() {
        let w = Wordmark::new();
        assert!(w.length() > 400.0 && w.length() < 900.0, "{}", w.length());
        let f0 = frame(&w, 0.0);
        assert_eq!((f0.ink_alpha, f0.nib_alpha), (0.0, 0.0), "nothing inked, nib not yet visible");
        assert!((f0.nib_x - 140.0).abs() < 0.01, "the nib starts at the centre");
        let down = frame(&w, 500.0);
        assert!(down.ink_alpha == 1.0 && down.ink < 0.01, "pen down: the first drop of ink");
        let mid = frame(&w, 1220.0);
        assert!(mid.ink > 0.3 && mid.ink < 0.8, "{}", mid.ink);
        assert!((mid.nib_rot - 35.0).abs() <= 8.5, "the nib keeps its slant: {}", mid.nib_rot);
        let end = frame(&w, TOTAL_MS);
        assert_eq!(end.ink, 1.0);
        assert!(end.nib_alpha < 0.001, "the nib has lifted away");
        assert!((end.desc_alpha - 1.0).abs() < 1e-4 && end.desc_dy.abs() < 1e-3);
        assert!((end.ink_width - INK_W).abs() < 1e-3, "the ink has settled");
        assert!(frame(&w, 1900.0).ink_width > INK_W, "still wet just after writing");
    }

    #[test]
    fn the_write_starts_slowly_then_finds_its_pace() {
        let share = |ms: f32| write_ease(ms / (WRITE.1 - WRITE.0));
        assert!(share(150.0) < 0.05);
        assert!(share(600.0) > 0.3);
    }

    #[test]
    fn easing_matches_its_ends() {
        assert_eq!(cubic(0.4, 0.0, 0.2, 1.0, 0.0), 0.0);
        assert_eq!(cubic(0.4, 0.0, 0.2, 1.0, 1.0), 1.0);
        assert!((standard(0.5) - 0.7756).abs() < 0.01);
    }
}
