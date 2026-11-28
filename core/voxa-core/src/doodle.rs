//! The doodles above Home's greeting (docs/ui-mocks/v7-home-greeting-and-splash.html, "Every
//! doodle"): one pen, one sepia ink, drawn stroke by stroke in a 343 x 112 band.
//!
//! This is the one source both apps draw from. [`draw`] turns a doodle, its date and its count
//! into a flat list of [`Stroke`]s: an absolute path (M, L, C, Z only), when it starts, how long it
//! takes, and how it is painted. Hatching comes out already clipped to its shape, because GPUI
//! can't clip a path. The daily variations (birds, clouds, star layouts) are picked from short
//! lists of safe spots with the date as the seed, and the night's moon is the real phase.
//!
//! Timing: the horizon draws 0 to 360 ms, the subject from 240 ms, everything is drawn by about
//! 1,300 ms, and one short flourish plays 1,000 to 1,400 ms.

use crate::greeting::{Date, Doodle, Rng, SYNODIC, moon_age, moon_lit, seed_of};
use crate::svgpath::{self, Affine, Seg};

/// The phone band: 343 x 112, horizon at y 92.
pub const BAND_W: f32 = 343.0;
pub const BAND_H: f32 = 112.0;
/// Pen widths in band units: the subject, the horizon and the hatching.
pub const INK_W: f32 = 2.0;
pub const GROUND_W: f32 = 1.5;
pub const HATCH_W: f32 = 1.2;
/// Hatch lines are drawn at this opacity of the ink; the horizon at [`ground_alpha`].
pub const HATCH_ALPHA: f32 = 0.7;

pub fn ground_alpha(dark: bool) -> f32 {
    if dark { 0.5 } else { 0.55 }
}

/// The wash behind anything that glows (the candle flame).
pub fn wash_alpha(dark: bool) -> f32 {
    if dark { 0.16 } else { 0.12 }
}

/// The flourish window: every doodle is finished by 1,400 ms.
pub const FLOURISH_START: u32 = 1000;
pub const FLOURISH_MS: u32 = 400;

/// How a stroke is painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The pen line, [`INK_W`], revealed along its length.
    Ink,
    /// The horizon (and the faint outline of a new moon): [`GROUND_W`] at [`ground_alpha`].
    Ground,
    /// One pen hatch line: [`HATCH_W`] at [`HATCH_ALPHA`].
    Hatch,
    /// A filled glow at [`wash_alpha`], faded in.
    Wash,
    /// A filled ink dot, faded in.
    Dot,
}

/// Which strokes move in the flourish.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    None,
    /// The night's big star: grows and turns, once.
    Twinkle,
    /// The candle flame: flickers, once.
    Flame,
    /// The mug's steam: rises 4 units and fades a little, once.
    Steam,
    /// The midday sun's rays: turn by one ray.
    Ray,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub kind: Kind,
    /// Absolute M, L, C and Z, in band units.
    pub d: String,
    pub start_ms: u32,
    pub dur_ms: u32,
    pub tag: Tag,
    /// What the flourish turns or scales about (band units).
    pub pivot: (f32, f32),
}

impl Stroke {
    pub fn segs(&self) -> Vec<Seg> {
        svgpath::parse(&self.d)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub strokes: Vec<Stroke>,
    /// The subject's scale (the flourish's 4 unit rise is in the subject's units).
    pub scale: f32,
    /// When the last stroke lands or the flourish ends.
    pub done_ms: u32,
}

// ── the parts, as in the mockup ─────────────────────────────────────────────────────────────

enum Part {
    /// Stroke, start, duration, tag.
    S(String, u32, u32, Tag),
    /// The horizon or a faint outline: not scaled when it is the horizon.
    H(String, u32, u32),
    W(String, u32, u32, Tag),
    Dot(String, u32, u32),
    /// Pen shading: clip shape, box [x0, y0, x1, y1], start, ms per line, gap between lines.
    Hatch(String, [f32; 4], u32, u32, f32),
}

use Part::*;

const HORIZON: &str = "M6 92 C 70 90.5, 130 93, 190 91.8 S 300 92.6, 337 91.6";

fn h() -> Part {
    H(HORIZON.into(), 0, 360)
}

fn s(d: impl Into<String>, st: u32, du: u32) -> Part {
    S(d.into(), st, du, Tag::None)
}

/// JavaScript's number to string for the values the mockup writes (one or two decimals at most).
fn n(v: f32) -> String {
    let r = (v as f64 * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

fn fx1(v: f64) -> String {
    format!("{:.1}", v)
}

#[allow(clippy::too_many_arguments)]
fn rays(cx: f64, cy: f64, r1: f64, r2: f64, a0: f64, a1: f64, count: u32, t0: u32, step: u32, tag: Tag) -> Vec<Part> {
    (0..count)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f64 / (count - 1) as f64).to_radians();
            S(
                format!("M{} {} L{} {}", fx1(cx + r1 * a.cos()), fx1(cy - r1 * a.sin()), fx1(cx + r2 * a.cos()), fx1(cy - r2 * a.sin())),
                t0 + i * step,
                160,
                tag,
            )
        })
        .collect()
}

fn spark(x: f32, y: f32, r: f32) -> String {
    let (x_, y_) = (n(x), n(y));
    format!(
        "M{x_} {} Q{x_} {y_} {} {y_} Q{x_} {y_} {x_} {} Q{x_} {y_} {} {y_} Q{x_} {y_} {x_} {} Z",
        n(y - r),
        n(x + r),
        n(y + r),
        n(x - r),
        n(y - r)
    )
}

fn bird(x: f32, y: f32, k: f32) -> String {
    format!("M{} {} q{} {} {} 0 q{} {} {} 0", n(x), n(y), n(5.0 * k), n(-6.0 * k), n(10.0 * k), n(5.0 * k), n(-6.0 * k), n(10.0 * k))
}

fn circ(x: f32, y: f32, r: f32) -> String {
    format!("M{} {} m-{} 0 a{} {} 0 1 0 {} 0 a{} {} 0 1 0 -{} 0", n(x), n(y), n(r), n(r), n(r), n(2.0 * r), n(r), n(r), n(2.0 * r))
}

fn dot(x: f32, y: f32) -> String {
    circ(x, y, 1.6)
}

fn burst(cx: f64, cy: f64, r1: f64, r2: f64, count: u32, t0: u32) -> Vec<Part> {
    let mut o = Vec::new();
    for i in 0..count {
        let a = i as f64 / count as f64 * std::f64::consts::TAU + 0.2;
        o.push(s(
            format!("M{} {} L{} {}", fx1(cx + r1 * a.cos()), fx1(cy + r1 * a.sin()), fx1(cx + r2 * a.cos()), fx1(cy + r2 * a.sin())),
            t0 + (i % 3) * 30,
            200,
        ));
        let (dx, dy) = (fx1(cx + (r2 + 4.0) * a.cos()), fx1(cy + (r2 + 4.0) * a.sin()));
        o.push(Dot(circ(dx.parse().unwrap_or(0.0), dy.parse().unwrap_or(0.0), 1.4), t0 + 220, 200));
    }
    o
}

fn birds(list: &[(f32, f32, f32)], t0: u32) -> Vec<Part> {
    list.iter().enumerate().map(|(i, &(x, y, k))| s(bird(x, y, k), t0 + i as u32 * 70, 200)).collect()
}

fn wave(x: f32, y: f32, count: usize) -> String {
    format!("M{} {}", n(x), n(y)) + " q2.5 -3.5 5 0" + &" t5 0".repeat(count - 1)
}

// Safe spots for the daily variations; every one keeps clear of the avatar corner (x > 287, y < 52).
const SUNRISE_BIRDS: [(f32, f32, f32); 5] = [(196.0, 52.0, 1.0), (212.0, 40.0, 0.8), (92.0, 50.0, 0.8), (72.0, 60.0, 0.9), (226.0, 60.0, 0.7)];
const MORNING_BIRDS: [(f32, f32, f32); 4] = [(206.0, 36.0, 0.8), (74.0, 40.0, 0.8), (236.0, 52.0, 0.7), (92.0, 56.0, 0.7)];
const MORNING_CLOUDS: [&[(&str, u32, u32)]; 3] = [
    &[("M176 50 C 182 43, 194 43, 200 47 C 206 42, 218 44, 222 50", 760, 220), ("M194 58 C 206 55, 226 55, 244 58", 830, 240)],
    &[("M44 44 C 50 37, 62 37, 68 41 C 74 36, 86 38, 90 44", 760, 220), ("M30 52 C 44 49, 64 49, 82 52", 830, 240)],
    &[("M178 66 C 184 59, 196 59, 202 63 C 208 58, 222 60, 228 66", 760, 240)],
];
const MIDDAY_CLOUDS: [f32; 3] = [176.0, 30.0, 196.0];
const MIDDAY_BIRDS: [(f32, f32, f32); 3] = [(200.0, 40.0, 0.8), (84.0, 70.0, 0.8), (226.0, 46.0, 0.7)];
const EVENING_BIRDS: [(f32, f32, f32); 4] = [(150.0, 56.0, 0.9), (104.0, 40.0, 0.8), (244.0, 58.0, 0.8), (128.0, 30.0, 0.7)];

struct Stars {
    tw: (f32, f32, f32),
    sp: &'static [(f32, f32, f32)],
    dots: &'static [(f32, f32)],
}

const NIGHT_STARS: [Stars; 4] = [
    Stars { tw: (204.0, 34.0, 7.0), sp: &[(236.0, 64.0, 5.0), (86.0, 44.0, 5.0)], dots: &[(60.0, 84.0), (260.0, 86.0)] },
    Stars { tw: (92.0, 30.0, 7.0), sp: &[(214.0, 48.0, 5.0), (250.0, 74.0, 4.0)], dots: &[(40.0, 70.0), (196.0, 84.0)] },
    Stars { tw: (222.0, 40.0, 7.0), sp: &[(70.0, 56.0, 5.0)], dots: &[(100.0, 24.0), (250.0, 82.0), (36.0, 80.0)] },
    Stars { tw: (60.0, 40.0, 6.0), sp: &[(206.0, 28.0, 5.0), (242.0, 66.0, 4.0)], dots: &[(92.0, 78.0), (224.0, 86.0)] },
];

/// The pen tally: gates of five (four strokes and a slash), the remainder, then one spark.
fn tally(count: u32) -> Vec<Part> {
    let big = count <= 10;
    let sp: f32 = if big { 14.0 } else { 7.5 };
    let top: f32 = if big { 50.0 } else { 58.0 };
    let gate_w = 3.0 * sp;
    let gap: f32 = if big { 22.0 } else { 15.0 };
    let over: f32 = if big { 8.0 } else { 3.0 };
    let groups = count / 5;
    let rest = count % 5;
    let marks = count + groups;
    let step = (90.0f32).min(760.0 / marks.max(1) as f32);
    let width = groups as f32 * (gate_w + gap) + if rest > 0 { (rest - 1) as f32 * sp } else { -gap };
    let mut x = 165.0 - width / 2.0;
    let mut t = 260.0f32;
    let mut out = vec![h()];
    let mark = |x: f32, i: u32, t: f32| s(format!("M{} 88 L{} {}", fx1((x + i as f32 * sp + 2.0) as f64), fx1((x + i as f32 * sp) as f64), n(top)), t.round() as u32, 110);
    for _ in 0..groups {
        for i in 0..4 {
            out.push(mark(x, i, t));
            t += step;
        }
        let (y0, y1) = if big { (80, 56) } else { (82, 62) };
        out.push(s(format!("M{} {y0} L{} {y1}", fx1((x - over) as f64), fx1((x + gate_w + over) as f64)), t.round() as u32, 140));
        t += step;
        x += gate_w + gap;
    }
    for i in 0..rest {
        out.push(mark(x, i, t));
        t += step;
    }
    let sx = x + if rest > 0 { (rest - 1) as f32 * sp } else { -gap } + 22.0;
    let sx: f32 = fx1(sx as f64).parse().unwrap_or(sx);
    out.push(s(spark(sx, if big { 44.0 } else { 50.0 }, 6.0), (t + 40.0).round() as u32, 160));
    out
}

const NOTEBOOK: &str = "M108 86 C 126 82, 148 82, 165 87 C 182 82, 204 82, 222 86 L222 42 C 204 38, 182 38, 165 43 C 148 38, 126 38, 108 42 Z";

fn loop_end(x: f32, y: f32, t: u32) -> Part {
    s(
        format!(
            "M{x} {y} q2.5 -3.5 5 0 t5 0 t5 0 C {} {}, {} {}, {} {} C {} {}, {} {}, {} {} C {} {}, {} {}, {} {}",
            x + 19.0,
            y - 4.0,
            x + 27.0,
            y - 8.0,
            x + 31.0,
            y - 2.0,
            x + 34.0,
            y + 4.0,
            x + 24.0,
            y + 6.0,
            x + 26.0,
            y - 1.0,
            x + 28.0,
            y - 6.0,
            x + 35.0,
            y - 4.0,
            x + 41.0,
            y - 2.0
        ),
        t,
        220,
    )
}

fn notebook(full: bool) -> Vec<Part> {
    let mut v = vec![h(), s(NOTEBOOK, 240, 520), s("M165 43 V87", 560, 140)];
    if full {
        for (i, y) in [50.0, 58.0, 66.0, 74.0].into_iter().enumerate() {
            v.push(s(wave(116.0, y, 8), 640 + i as u32 * 70, 150));
        }
        for (i, y) in [50.0, 58.0, 66.0].into_iter().enumerate() {
            v.push(s(wave(173.0, y, 8), 920 + i as u32 * 70, 150));
        }
        v.push(loop_end(173.0, 74.0, 1130));
    } else {
        v.push(loop_end(118.0, 52.0, 680));
    }
    v
}

/// The lit part of the moon: the limb on the lit side, then the terminator back up.
fn moon_path(cx: f32, cy: f32, r: f32, age: f64) -> String {
    let f = age / SYNODIC;
    let k = (std::f64::consts::TAU * f).cos();
    let rx = (k.abs() * r as f64).max(0.01);
    let waxing = f < 0.5;
    let crescent = k > 0.0;
    let (top, bot) = (format!("{} {}", n(cx), n(cy - r)), format!("{} {}", n(cx), n(cy + r)));
    let sweep2 = if waxing { if crescent { 0 } else { 1 } } else if crescent { 1 } else { 0 };
    format!("M{top} A{} {} 0 0 {} {bot} A{:.2} {} 0 0 {sweep2} {top} Z", n(r), n(r), if waxing { 1 } else { 0 }, rx, n(r))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flourish {
    None,
    Twinkle,
    Flicker,
    Rise,
    Turn,
}

struct Spec {
    scale: f32,
    origin: (f32, f32),
    pivot: (f32, f32),
    parts: Vec<Part>,
}

fn spec(doodle: Doodle, date: Date, count: u32) -> Spec {
    let mut r = Rng::new(seed_of(&format!("{date}{}", doodle.key())));
    let mk = |scale, origin, _flourish: Flourish, parts| Spec { scale, origin, pivot: (0.0, 0.0), parts };
    match doodle {
        Doodle::Sunrise => {
            let mut p = vec![h(), s("M118 92 A34 34 0 0 1 186 92", 260, 420), Hatch("M118 92 A34 34 0 0 1 186 92 Z".into(), [118.0, 58.0, 186.0, 92.0], 600, 22, 6.0)];
            p.extend(rays(152.0, 92.0, 42.0, 52.0, 160.0, 20.0, 7, 520, 55, Tag::None));
            let k = r.pick(&[0usize, 1, 2, 2, 3]);
            p.extend(birds(&r.pick_n(&SUNRISE_BIRDS, k), 880));
            mk(1.3, (152.0, 92.0), Flourish::None, p)
        }
        Doodle::Morning => {
            let mut p = vec![h(), s(circ(126.0, 56.0, 19.0), 260, 400), Hatch(circ(126.0, 56.0, 19.0), [107.0, 37.0, 145.0, 75.0], 600, 20, 6.0)];
            p.extend(rays(126.0, 56.0, 25.0, 32.0, 200.0, -20.0, 6, 500, 50, Tag::None));
            for &(d, st, du) in r.pick(&MORNING_CLOUDS) {
                p.push(s(d, st, du));
            }
            let k = r.pick(&[0usize, 1, 1, 2]);
            p.extend(birds(&r.pick_n(&MORNING_BIRDS, k), 940));
            mk(1.15, (150.0, 92.0), Flourish::None, p)
        }
        Doodle::Midday => {
            let cx = r.pick(&MIDDAY_CLOUDS);
            let mut p = vec![h(), s(circ(122.0, 50.0, 21.0), 260, 420), Hatch(circ(122.0, 50.0, 21.0), [101.0, 29.0, 143.0, 71.0], 620, 20, 6.0)];
            p.extend(rays(122.0, 50.0, 28.0, 36.0, 90.0, 405.0, 8, 500, 45, Tag::Ray));
            let cloud = if cx < 120.0 {
                format!("M{} 86 h52 a10 10 0 0 0 -4 -19 a13 13 0 0 0 -24 -3 a10 10 0 0 0 -17 8 a7 7 0 0 0 -7 14", n(cx))
            } else {
                format!("M{} 86 h60 a11 11 0 0 0 -4 -21 a15 15 0 0 0 -28 -4 a11 11 0 0 0 -20 9 a8 8 0 0 0 -8 16", n(cx))
            };
            p.push(s(cloud, 720, 380));
            let k = r.pick(&[0usize, 0, 1]);
            p.extend(birds(&r.pick_n(&MIDDAY_BIRDS, k), 900));
            Spec { pivot: (122.0, 50.0), ..mk(1.15, (150.0, 92.0), Flourish::Turn, p) }
        }
        Doodle::Evening => {
            let mut p = vec![
                H("M6 92 C 40 92, 56 66, 96 66 S 140 92, 164 92 S 300 92.6, 337 91.6".into(), 0, 380),
                s("M180 92 A28 28 0 0 1 236 92", 300, 380),
                Hatch("M180 92 A28 28 0 0 1 236 92 Z".into(), [180.0, 64.0, 236.0, 92.0], 600, 22, 6.0),
                s("M186 98 H230", 640, 160),
                s("M194 103 H222", 720, 140),
                s("M202 108 H214", 790, 120),
            ];
            let k = r.pick(&[0usize, 1, 1, 2]);
            p.extend(birds(&r.pick_n(&EVENING_BIRDS, k), 860));
            mk(1.3, (208.0, 92.0), Flourish::None, p)
        }
        Doodle::Night => {
            let l = &NIGHT_STARS[((r.next() * 4.0) as usize).min(3)];
            let age = moon_age(date);
            let lit = moon_lit(age);
            let m = moon_path(138.0, 47.0, 22.0, age);
            let mut p = vec![h()];
            if lit < 0.03 {
                p.push(H(circ(138.0, 47.0, 22.0), 260, 520));
            } else if lit > 0.97 {
                p.push(s(circ(138.0, 47.0, 22.0), 260, 520));
                p.push(Hatch(circ(138.0, 47.0, 22.0), [116.0, 25.0, 160.0, 69.0], 700, 20, 5.0));
            } else {
                p.push(s(m.clone(), 260, 520));
                p.push(Hatch(m, [116.0, 25.0, 160.0, 69.0], 700, 20, 5.0));
            }
            p.push(S(spark(l.tw.0, l.tw.1, l.tw.2), 620, 220, Tag::Twinkle));
            for (i, &(x, y, k)) in l.sp.iter().enumerate() {
                p.push(s(spark(x, y, k), 720 + i as u32 * 80, 200));
            }
            for (i, &(x, y)) in l.dots.iter().enumerate() {
                p.push(Dot(dot(x, y), 900 + i as u32 * 40, 200));
            }
            mk(1.2, (160.0, 92.0), Flourish::Twinkle, p)
        }
        Doodle::Weekend => {
            let waves = [wave(183.0, 76.0, 4), wave(183.0, 82.0, 3), wave(212.0, 76.0, 4), wave(212.0, 82.0, 4)].join(" ");
            let p = vec![
                h(),
                s("M124 56 H156 V84 C156 89, 152 92, 147 92 H133 C128 92, 124 89, 124 84 Z", 260, 420),
                Hatch("M124 70 H156 V84 C156 89, 152 92, 147 92 H133 C128 92, 124 89, 124 84 Z".into(), [124.0, 70.0, 156.0, 92.0], 640, 18, 5.0),
                s("M156 62 C 168 62, 168 80, 156 80", 560, 200),
                s("M174 90 L178 70 C 188 66, 200 67, 207 73 C 214 67, 226 66, 236 70 L240 90 C 230 86, 216 86, 207 91 C 198 86, 184 86, 174 90 Z", 620, 360),
                s("M207 73 V91", 820, 100),
                s(waves, 880, 220),
                S("M134 49 C 128 43, 140 38, 134 30".into(), 820, 240, Tag::Steam),
                S("M147 49 C 141 43, 153 38, 147 28".into(), 880, 240, Tag::Steam),
            ];
            mk(1.3, (165.0, 92.0), Flourish::Rise, p)
        }
        Doodle::Streak => mk(1.0, (165.0, 92.0), Flourish::None, tally(count.max(1))),
        Doodle::Words => mk(1.15, (165.0, 92.0), Flourish::None, notebook(true)),
        Doodle::First => mk(1.15, (165.0, 92.0), Flourish::None, notebook(false)),
        Doodle::Birthday => {
            let flame = "M159 44 C 153 39, 155 31, 159 24 C 163 31, 165 39, 159 44 Z";
            let p = vec![
                h(),
                s("M120 92 H198", 220, 180),
                s("M128 92 V64 H190 V92", 300, 380),
                s("M128 71 C 134 78, 140 64, 147 71 S 160 78, 166 71 S 180 64, 190 71", 560, 240),
                Hatch("M128 76 H190 V92 H128 Z".into(), [128.0, 76.0, 190.0, 92.0], 700, 16, 5.0),
                s("M159 64 V48", 680, 120),
                W(flame.into(), 900, 240, Tag::Flame),
                S(flame.into(), 760, 240, Tag::Flame),
                s(spark(108.0, 50.0, 5.0), 900, 180),
                s(spark(212.0, 40.0, 5.0), 960, 180),
                Dot(dot(96.0, 72.0), 1000, 160),
                Dot(dot(224.0, 70.0), 1040, 160),
            ];
            mk(1.15, (159.0, 92.0), Flourish::Flicker, p)
        }
        Doodle::NewYear => {
            let mut p = vec![h(), s("M118 92 C 116 78, 122 64, 120 48", 240, 260), s("M206 92 C 208 82, 204 74, 206 64", 360, 220)];
            p.extend(burst(120.0, 38.0, 8.0, 24.0, 10, 500));
            p.extend(burst(206.0, 56.0, 6.0, 16.0, 8, 640));
            mk(1.1, (165.0, 92.0), Flourish::None, p)
        }
    }
}

/// The flourish a doodle plays once, 1,000 to 1,400 ms.
pub fn flourish_of(doodle: Doodle) -> Flourish {
    match doodle {
        Doodle::Night => Flourish::Twinkle,
        Doodle::Birthday => Flourish::Flicker,
        Doodle::Weekend => Flourish::Rise,
        Doodle::Midday => Flourish::Turn,
        _ => Flourish::None,
    }
}

/// Draws `doodle` for `date` (the seed and the moon) with `count` marks for the tally, in a band
/// `width` wide with the subject moved right by `dx` (0 on the phone; the desktop's wider band
/// puts the subject on the right and runs the horizon edge to edge).
pub fn draw(doodle: Doodle, date: Date, count: u32, width: f32, dx: f32) -> Drawing {
    let sp = spec(doodle, date, count);
    let sc = sp.scale;
    let subject = Affine::scale_about(sc, sp.origin.0, sp.origin.1, dx, 0.0);
    let shift = Affine::scale_about(1.0, 0.0, 0.0, dx, 0.0);
    let pivot_t = subject.apply(sp.pivot.0, sp.pivot.1);
    let mut strokes = Vec::new();
    let mut done = 0u32;
    let mut add = |kind: Kind, segs: Vec<Seg>, st: u32, du: u32, tag: Tag, strokes: &mut Vec<Stroke>| {
        done = done.max(st + du);
        let pivot = match tag {
            Tag::Ray => pivot_t,
            Tag::None | Tag::Steam => (0.0, 0.0),
            Tag::Twinkle | Tag::Flame => {
                let pts: Vec<(f32, f32)> = svgpath::flatten(&segs, 0.5).into_iter().flatten().collect();
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (x, y) in pts {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
                if tag == Tag::Flame { ((x0 + x1) / 2.0, y1) } else { ((x0 + x1) / 2.0, (y0 + y1) / 2.0) }
            }
        };
        strokes.push(Stroke { kind, d: svgpath::to_string(&segs), start_ms: st, dur_ms: du, tag, pivot });
    };
    for part in sp.parts {
        match part {
            H(d, st, du) => {
                if d.starts_with("M6 92") {
                    // The horizon is not scaled; on a wider band it runs edge to edge.
                    let mut segs = svgpath::transform(&svgpath::parse(&d), &shift);
                    if dx > 0.0 || width > BAND_W {
                        segs.insert(0, Seg::Move(6.0, 92.0));
                        if let Some(Seg::Move(x, y)) = segs.get(1).copied() {
                            segs[1] = Seg::Line(x, y);
                        }
                        segs.push(Seg::Line(width - 6.0, 91.6));
                    }
                    add(Kind::Ground, segs, st, du, Tag::None, &mut strokes);
                } else {
                    add(Kind::Ground, svgpath::transform(&svgpath::parse(&d), &subject), st, du, Tag::None, &mut strokes);
                }
            }
            S(d, st, du, tag) => add(Kind::Ink, svgpath::transform(&svgpath::parse(&d), &subject), st, du, tag, &mut strokes),
            W(d, st, du, tag) => add(Kind::Wash, svgpath::transform(&svgpath::parse(&d), &subject), st, du, tag, &mut strokes),
            Dot(d, st, du) => add(Kind::Dot, svgpath::transform(&svgpath::parse(&d), &subject), st, du, Tag::None, &mut strokes),
            Hatch(clip, [x0, y0, x1, y1], st, per, gap) => {
                let poly = svgpath::flatten(&svgpath::parse(&clip), 0.25);
                let step = gap * std::f32::consts::SQRT_2;
                let mut k = x0 + y0 + step / 2.0;
                let mut i = 0u32;
                while k < x1 + y1 {
                    let (a, b) = ((k - y1, y1), (k - y0, y0));
                    for (p, q) in clip_line(a, b, &poly) {
                        let segs = svgpath::transform(&[Seg::Move(p.0, p.1), Seg::Line(q.0, q.1)], &subject);
                        add(Kind::Hatch, segs, st + i * per, 110, Tag::None, &mut strokes);
                    }
                    k += step;
                    i += 1;
                }
            }
        }
    }
    if flourish_of(doodle) != Flourish::None {
        done = done.max(FLOURISH_START + FLOURISH_MS);
    }
    Drawing { strokes, scale: sc, done_ms: done }
}

/// The parts of the segment a to b inside the polygons (even-odd), shorter than half a unit dropped.
fn clip_line(a: (f32, f32), b: (f32, f32), polys: &[Vec<(f32, f32)>]) -> Vec<((f32, f32), (f32, f32))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let mut ts: Vec<f32> = vec![];
    for poly in polys {
        let mut ring = poly.clone();
        if ring.first() != ring.last() {
            if let Some(&f) = ring.first() {
                ring.push(f);
            }
        }
        for w in ring.windows(2) {
            let (p, q) = (w[0], w[1]);
            let (ex, ey) = (q.0 - p.0, q.1 - p.1);
            let den = dx * ey - dy * ex;
            if den.abs() < 1e-9 {
                continue;
            }
            let t = ((p.0 - a.0) * ey - (p.1 - a.1) * ex) / den;
            let u = ((p.0 - a.0) * dy - (p.1 - a.1) * dx) / den;
            // Half-open on the edge so a vertex is counted once.
            if (0.0..1.0).contains(&u) && (-1e-6..=1.0 + 1e-6).contains(&t) {
                ts.push(t.clamp(0.0, 1.0));
            }
        }
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let at = |t: f32| (a.0 + dx * t, a.1 + dy * t);
    let mut inside = polys.iter().filter(|p| contains(p, at(0.0))).count() % 2 == 1;
    let mut out = vec![];
    let mut from = 0.0f32;
    for t in ts.into_iter().chain(std::iter::once(1.0)) {
        if inside && (t - from) * (dx * dx + dy * dy).sqrt() > 0.5 {
            out.push((at(from), at(t)));
        }
        if t < 1.0 {
            inside = !inside;
        }
        from = t;
    }
    out
}

fn contains(poly: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
    }
    c
}

/// Material's standard curve, cubic-bezier(0.4, 0, 0.2, 1): every stroke and fade.
pub fn standard(t: f32) -> f32 {
    crate::splash::cubic(0.4, 0.0, 0.2, 1.0, t)
}

/// How much of a stroke is drawn at `t_ms` (0 to 1).
pub fn progress(stroke: &Stroke, t_ms: f32) -> f32 {
    if stroke.dur_ms == 0 {
        return if t_ms >= stroke.start_ms as f32 { 1.0 } else { 0.0 };
    }
    standard(((t_ms - stroke.start_ms as f32) / stroke.dur_ms as f32).clamp(0.0, 1.0))
}

/// The flourish's transform for a tagged stroke at `t_ms`, as an SVG matrix [a, b, c, d, e, f] in
/// band units, and an opacity. None outside the flourish window or for an untagged stroke.
pub fn flourish_at(doodle: Doodle, stroke: &Stroke, scale: f32, t_ms: f32) -> Option<([f32; 6], f32)> {
    let f = flourish_of(doodle);
    let raw = (t_ms - FLOURISH_START as f32) / FLOURISH_MS as f32;
    if !(0.0..1.0).contains(&raw) || stroke.tag == Tag::None {
        return None;
    }
    let p = standard(raw);
    let (px_, py) = stroke.pivot;
    let about = |a: f32, b: f32, c: f32, d: f32| [a, b, c, d, px_ - (a * px_ + c * py), py - (b * px_ + d * py)];
    // Linear between evenly spaced keyframes, as the Web Animations API does.
    let keys = |k: &[f32]| {
        let segs = (k.len() - 1) as f32;
        let i = ((p * segs) as usize).min(k.len() - 2);
        let local = p * segs - i as f32;
        k[i] + (k[i + 1] - k[i]) * local
    };
    match (f, stroke.tag) {
        (Flourish::Twinkle, Tag::Twinkle) => {
            let s = keys(&[1.0, 1.5, 1.0]);
            let r = keys(&[0.0, 20.0, 0.0]).to_radians();
            let (sin, cos) = r.sin_cos();
            Some((about(s * cos, s * sin, -s * sin, s * cos), 1.0))
        }
        (Flourish::Flicker, Tag::Flame) => {
            let sy = keys(&[1.0, 1.14, 0.94, 1.0]);
            let sk = keys(&[0.0, -5.0, 3.0, 0.0]).to_radians().tan();
            Some((about(1.0, 0.0, sk, sy), 1.0))
        }
        (Flourish::Rise, Tag::Steam) => {
            let up = keys(&[0.0, 1.0, 0.0]);
            Some(([1.0, 0.0, 0.0, 1.0, 0.0, -4.0 * scale * up], 1.0 - 0.5 * up))
        }
        (Flourish::Turn, Tag::Ray) => {
            // One ray step (45 degrees), so the sun ends looking as it started.
            let r = (45.0 * p).to_radians();
            let (sin, cos) = r.sin_cos();
            Some((about(cos, sin, -sin, cos), 1.0))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::greeting::Date;

    fn all_doodles(date: Date) -> Vec<(Doodle, Drawing)> {
        Doodle::ALL.iter().map(|&d| (d, draw(d, date, if d == Doodle::Streak { 7 } else { 0 }, BAND_W, 0.0))).collect()
    }

    fn bounds(st: &Stroke) -> (f32, f32, f32, f32) {
        let pts: Vec<(f32, f32)> = svgpath::flatten(&st.segs(), 0.5).into_iter().flatten().collect();
        pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, &(x, y)| (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y)))
    }

    #[test]
    fn every_doodle_draws_and_finishes_by_1400_ms() {
        for (d, dr) in all_doodles(Date::new(2026, 11, 28)) {
            assert!(dr.strokes.len() >= 4, "{d:?}");
            assert!(dr.done_ms <= 1400, "{d:?} done by {}", dr.done_ms);
            for s in &dr.strokes {
                assert!(s.d.starts_with('M'), "{d:?}: {}", s.d);
                assert!(s.d.chars().filter(|c| c.is_ascii_alphabetic()).all(|c| "MLCZ".contains(c)), "{d:?}: {}", s.d);
                assert!(!s.segs().is_empty());
            }
        }
    }

    #[test]
    fn nothing_enters_the_avatar_corner_for_60_days() {
        let mut n = 0;
        for k in 0..60 {
            let date = Date::new(2026, 11, 28).plus(k);
            for d in [Doodle::Sunrise, Doodle::Morning, Doodle::Midday, Doodle::Evening, Doodle::Night] {
                n += 1;
                for s in draw(d, date, 0, BAND_W, 0.0).strokes {
                    let (_, y0, x1, _) = bounds(&s);
                    assert!(!(x1 > 287.0 && y0 < 52.0) || s.kind == Kind::Ground, "{d:?} on {date}: {}", s.d);
                }
            }
        }
        assert_eq!(n, 300);
    }

    #[test]
    fn hatching_stays_inside_its_shape() {
        // The sunrise's hatch sits inside the half disc of radius 34 about (152, 92), scaled 1.3.
        let dr = draw(Doodle::Sunrise, Date::new(2026, 12, 1), 0, BAND_W, 0.0);
        let hatch: Vec<&Stroke> = dr.strokes.iter().filter(|s| s.kind == Kind::Hatch).collect();
        assert!(hatch.len() >= 8, "{}", hatch.len());
        for s in hatch {
            for seg in s.segs() {
                if let Seg::Move(x, y) | Seg::Line(x, y) = seg {
                    let r = ((x - 152.0).powi(2) + (y - 92.0).powi(2)).sqrt();
                    assert!(r <= 34.0 * 1.3 + 0.2 && y <= 92.01, "({x}, {y}) r {r}");
                }
            }
        }
        // The moon's hatch follows the lit part only: a thin crescent has fewer lines than a full moon.
        let lines = |date| draw(Doodle::Night, date, 0, BAND_W, 0.0).strokes.iter().filter(|s| s.kind == Kind::Hatch).count();
        assert!(lines(Date::new(2026, 12, 12)) < lines(Date::new(2026, 11, 24)));
    }

    #[test]
    fn new_moon_is_a_faint_outline_and_full_moon_is_hatched() {
        let new = draw(Doodle::Night, Date::new(2026, 12, 9), 0, BAND_W, 0.0);
        assert!(new.strokes.iter().all(|s| s.kind != Kind::Hatch));
        assert_eq!(new.strokes.iter().filter(|s| s.kind == Kind::Ground).count(), 2, "horizon and the faint moon");
    }

    #[test]
    fn variations_are_deterministic_and_do_vary() {
        let a = draw(Doodle::Morning, Date::new(2026, 12, 1), 0, BAND_W, 0.0);
        assert_eq!(a, draw(Doodle::Morning, Date::new(2026, 12, 1), 0, BAND_W, 0.0));
        let distinct: std::collections::HashSet<String> =
            (0..14).map(|k| draw(Doodle::Morning, Date::new(2026, 12, 1).plus(k), 0, BAND_W, 0.0).strokes.iter().map(|s| s.d.clone()).collect()).collect();
        assert!(distinct.len() >= 4, "{}", distinct.len());
    }

    #[test]
    fn the_tally_counts_for_real() {
        let marks = |n| draw(Doodle::Streak, Date::new(2026, 12, 2), n, BAND_W, 0.0).strokes.iter().filter(|s| s.kind == Kind::Ink).count();
        // n marks (every fifth is the slash across a gate of four), and one spark.
        assert_eq!(marks(7), 7 + 1);
        assert_eq!(marks(30), 30 + 1);
        assert_eq!(marks(52), 52 + 1);
        assert!(draw(Doodle::Streak, Date::new(2026, 12, 2), 52, BAND_W, 0.0).done_ms <= 1400);
    }

    #[test]
    fn the_wide_band_moves_the_subject_and_runs_the_horizon_edge_to_edge() {
        let dr = draw(Doodle::Midday, Date::new(2026, 12, 3), 0, 780.0, 413.0);
        let ground = dr.strokes.iter().find(|s| s.kind == Kind::Ground).unwrap();
        let (x0, _, x1, _) = bounds(ground);
        assert!(x0 <= 6.01 && x1 >= 773.9, "{x0} {x1}");
        let sun = &dr.strokes[1];
        assert!(bounds(sun).0 > 413.0);
    }

    #[test]
    fn flourishes_move_only_in_their_window_and_end_where_they_started() {
        let dr = draw(Doodle::Midday, Date::new(2026, 12, 3), 0, BAND_W, 0.0);
        let ray = dr.strokes.iter().find(|s| s.tag == Tag::Ray).unwrap();
        assert!(flourish_at(Doodle::Midday, ray, dr.scale, 999.0).is_none());
        assert!(flourish_at(Doodle::Midday, ray, dr.scale, 1400.0).is_none());
        let (m, _) = flourish_at(Doodle::Midday, ray, dr.scale, 1200.0).unwrap();
        assert!(m[1].abs() > 0.1, "turning mid way");
        let (m, a) = flourish_at(Doodle::Midday, ray, dr.scale, 1399.9).unwrap();
        // 45 degrees maps the rays onto each other.
        assert!((m[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01 && a == 1.0);
        let wk = draw(Doodle::Weekend, Date::new(2026, 11, 29), 0, BAND_W, 0.0);
        let steam = wk.strokes.iter().find(|s| s.tag == Tag::Steam).unwrap();
        let peak = (1000..1400).filter_map(|t| flourish_at(Doodle::Weekend, steam, wk.scale, t as f32)).map(|(m, a)| (m[5], a)).fold((0f32, 1f32), |p, q| (p.0.min(q.0), p.1.min(q.1)));
        assert!((peak.0 + 4.0 * 1.3).abs() < 0.05 && (peak.1 - 0.5).abs() < 0.01, "{peak:?}");
        let (m, _) = flourish_at(Doodle::Weekend, steam, wk.scale, 1399.9).unwrap();
        assert!(m[5].abs() < 0.05, "back down at the end");
    }

    #[test]
    fn the_data_is_small() {
        // Path data for one day's drawings, as the shells receive it.
        for (d, dr) in all_doodles(Date::new(2026, 11, 28)) {
            let bytes: usize = dr.strokes.iter().map(|s| s.d.len()).sum();
            assert!(bytes < 6000, "{d:?}: {bytes} bytes");
        }
    }
}
