//! Home's doodle and the written splash, painted with GPUI paths (docs/ui-mocks/
//! v7-home-greeting-and-splash.html). The strokes, timing and nib come from voxa-core, so the
//! window draws what the phone draws. A stroke is revealed along its length with a dash, the
//! hatching arrives already clipped, and nothing here asks for frames by itself: the Home view
//! requests animation frames only while something moves.

use std::rc::Rc;

use gpui::{AnyElement, Bounds, Hsla, PathBuilder, Pixels, Transform, canvas, point, prelude::*, px};
use gpui::{PathStyle, StrokeOptions};
use lyon::tessellation::{LineCap, LineJoin};
use voxa_core::doodle::{self, Drawing, Kind, Tag};
use voxa_core::greeting::Doodle;
use voxa_core::splash;
use voxa_core::svgpath::{self, Measured, Seg};

/// A doodle's strokes parsed and measured once.
pub struct Art {
    pub doodle: Doodle,
    pub drawing: Drawing,
    strokes: Vec<(Vec<Seg>, f32)>,
}

impl Art {
    pub fn new(doodle: Doodle, drawing: Drawing) -> Self {
        let strokes = drawing
            .strokes
            .iter()
            .map(|s| {
                let segs = s.segs();
                let len = svgpath::flatten(&segs, 0.25).into_iter().map(|p| Measured::new(p).length()).sum();
                (segs, len)
            })
            .collect();
        Self { doodle, drawing, strokes }
    }
}

fn add(b: &mut PathBuilder, segs: &[Seg]) {
    for s in segs {
        match *s {
            Seg::Move(x, y) => b.move_to(point(px(x), px(y))),
            Seg::Line(x, y) => b.line_to(point(px(x), px(y))),
            Seg::Cubic(a, c, d, e, x, y) => b.cubic_bezier_to(point(px(x), px(y)), point(px(a), px(c)), point(px(d), px(e))),
            Seg::Close => b.close(),
        }
    }
}

fn round(width: f32) -> PathStyle {
    PathStyle::Stroke(StrokeOptions::default().with_line_width(width).with_line_cap(LineCap::Round).with_line_join(LineJoin::Round))
}

/// `m` (band units) then scale `k` and move to `origin`.
fn place(m: [f32; 6], k: f32, origin: gpui::Point<Pixels>) -> Transform {
    Transform::new(m[0], m[1], m[2], m[3], m[4], m[5]).then_scale(k, k).then_translate(lyon::math::vector(f32::from(origin.x), f32::from(origin.y)))
}

/// Whether `s` is the long horizon (the phone band's ground line, "M6 92 ..."): Home's hero draws
/// its own line across the content instead.
pub fn is_horizon(s: &doodle::Stroke) -> bool {
    s.kind == Kind::Ground && s.d.starts_with("M6 92")
}

/// Paints `art` at `t` ms, band units scaled by `k` with the band's (0, 0) at `origin`.
fn paint_strokes(art: &Art, t: f32, ink: Hsla, dark: bool, k: f32, origin: gpui::Point<Pixels>, skip_horizon: bool, window: &mut gpui::Window) {
    for (s, (segs, len)) in art.drawing.strokes.iter().zip(&art.strokes) {
        let t0 = s.start_ms as f32;
        if t < t0 || (skip_horizon && is_horizon(s)) {
            continue;
        }
        let p = doodle::progress(s, t);
        let flourish = if s.tag != Tag::None { doodle::flourish_at(art.doodle, s, art.drawing.scale, t) } else { None };
        let (m, fa) = flourish.unwrap_or(([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 1.0));
        let tr = place(m, k, origin);
        let (style, alpha) = match s.kind {
            Kind::Ink => (Some(doodle::INK_W), 1.0),
            Kind::Ground => (Some(doodle::GROUND_W), doodle::ground_alpha(dark)),
            Kind::Hatch => (Some(doodle::HATCH_W), doodle::HATCH_ALPHA),
            Kind::Wash => (None, doodle::wash_alpha(dark) * p),
            Kind::Dot => (None, p),
        };
        let mut pb = match style {
            Some(w) => {
                let pb = PathBuilder::default().with_style(round(w * stroke_scale(k)));
                if p < 1.0 { pb.dash_array(&[px(len * k * p), px(len * k + 1.0)]) } else { pb }
            }
            None => PathBuilder::fill(),
        };
        if style.is_some() && p <= 0.0 {
            continue;
        }
        add(&mut pb, segs);
        pb.transform(tr);
        if let Ok(path) = pb.build() {
            window.paint_path(path, Hsla { a: ink.a * alpha * fa, ..ink });
        }
    }
}

/// The doodle at `t` ms, in a band `height` tall (the band's 112 units fit the height).
/// `art_for` gives the drawing for the band's width in band units (cache it: it is asked each frame).
pub fn doodle(art_for: impl Fn(f32) -> Rc<Art> + 'static, t: f32, ink: Hsla, dark: bool, height: f32) -> AnyElement {
    canvas(
        move |b: Bounds<Pixels>, _, _| art_for(f32::from(b.size.width) / (height / doodle::BAND_H)),
        move |b: Bounds<Pixels>, art: Rc<Art>, window, _| paint_strokes(&art, t, ink, dark, height / doodle::BAND_H, b.origin, false, window),
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}

/// Home's hero: the doodle alone, cropped to `crop` (x0, y0, x1, y1 in band units: its drawn
/// bounds plus padding), drawn `k` times its band size, without the long horizon.
pub fn doodle_fit(art: Rc<Art>, t: f32, ink: Hsla, dark: bool, k: f32, crop: [f32; 4]) -> AnyElement {
    let (w, h) = ((crop[2] - crop[0]) * k, (crop[3] - crop[1]) * k);
    canvas(
        |_, _, _| (),
        move |b: Bounds<Pixels>, _, window, _| {
            let origin = point(b.origin.x - px(crop[0] * k), b.origin.y - px(crop[1] * k));
            paint_strokes(&art, t, ink, dark, k, origin, true, window)
        },
    )
    .flex_none()
    .w(px(w))
    .h(px(h))
    .into_any_element()
}

/// How much thicker the pen gets when the doodle is drawn `k` times its band size. Up to the band
/// (k <= 1) the pen scales with the drawing, as before; above it, with the square root, so the
/// bigger Home hero reads at the band's weight, not a felt marker.
pub fn stroke_scale(k: f32) -> f32 {
    if k <= 1.0 { k } else { k.sqrt() }
}

/// The wordmark measured once, for the nib.
pub struct Splash {
    word: splash::Wordmark,
    segs: Vec<Seg>,
    nib: Vec<Seg>,
    slit: Vec<Seg>,
}

impl Default for Splash {
    fn default() -> Self {
        Self { word: splash::Wordmark::new(), segs: svgpath::parse(splash::WORDMARK), nib: svgpath::parse(splash::NIB), slit: svgpath::parse(splash::NIB_SLIT) }
    }
}

/// Splash colours: the paper, the ink, the nib, the tagline.
pub struct SplashColors {
    pub paper: Hsla,
    pub ink: Hsla,
    pub nib: Hsla,
    pub desc: Hsla,
}

/// The wordmark box at 1.25 times the phone size, centred a little above the middle.
pub const SPLASH_SCALE: f32 = 1.25;

/// The written splash at `t` ms (None: the still end frame, reduced motion).
pub fn splash(sp: Rc<Splash>, t: Option<f32>, c: SplashColors) -> AnyElement {
    let f = match t {
        Some(t) => splash::frame(&sp.word, t),
        None => splash::end_frame(&sp.word),
    };
    let k = SPLASH_SCALE;
    let (w, h) = (splash::BOX_W * k, splash::BOX_H * k);
    let stage = canvas(
        |_, _, _| {},
        move |b: Bounds<Pixels>, _, window, _| {
            if f.ink_alpha > 0.0 {
                let len = sp.word.length() * k;
                let pb = PathBuilder::default().with_style(round(f.ink_width * k));
                let mut pb = if f.ink < 1.0 { pb.dash_array(&[px(len * f.ink), px(len + 1.0)]) } else { pb };
                add(&mut pb, &sp.segs);
                pb.transform(place([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], k, b.origin));
                if let Ok(p) = pb.build() {
                    window.paint_path(p, c.ink);
                }
            }
            if f.nib_alpha > 0.001 {
                // Tip at the head of the line, a writer's slant, then the icon's 0.9.
                let (s, co) = f.nib_rot.to_radians().sin_cos();
                let sc = f.nib_scale * splash::NIB_INNER_SCALE;
                let m = [co * sc, s * sc, -s * sc, co * sc, f.nib_x, f.nib_y];
                let tr = place(m, k, b.origin);
                let a = f.nib_alpha;
                // Paint the cut-outs in the paper colour, blended to the nib's opacity.
                let paper = Hsla { a, ..c.paper };
                let mut body = PathBuilder::fill();
                add(&mut body, &sp.nib);
                body.transform(tr);
                if let Ok(p) = body.build() {
                    window.paint_path(p, Hsla { a: c.nib.a * a, ..c.nib });
                }
                let (hx, hy, hr) = splash::NIB_HOLE;
                let mut hole = PathBuilder::fill();
                for i in 0..=24 {
                    let an = i as f32 / 24.0 * std::f32::consts::TAU;
                    let q = point(px(hx + hr * an.cos()), px(hy + hr * an.sin()));
                    if i == 0 { hole.move_to(q) } else { hole.line_to(q) }
                }
                hole.close();
                hole.transform(tr);
                if let Ok(p) = hole.build() {
                    window.paint_path(p, paper);
                }
                let (rx, ry, rw, rh) = splash::NIB_SHOULDER;
                let mut sh = PathBuilder::fill();
                sh.move_to(point(px(rx), px(ry)));
                sh.line_to(point(px(rx + rw), px(ry)));
                sh.line_to(point(px(rx + rw), px(ry + rh)));
                sh.line_to(point(px(rx), px(ry + rh)));
                sh.close();
                sh.transform(tr);
                if let Ok(p) = sh.build() {
                    window.paint_path(p, paper);
                }
                let mut sl = PathBuilder::default().with_style(round(splash::NIB_SLIT_W * sc * k));
                add(&mut sl, &sp.slit);
                sl.transform(tr);
                if let Ok(p) = sl.build() {
                    window.paint_path(p, paper);
                }
            }
        },
    )
    .w(px(w))
    .h(px(h));
    let desc = gpui::div()
        .absolute()
        .left(px(-80.0))
        .w(px(w + 160.0))
        .top(px(splash::TAGLINE_TOP * k + f.desc_dy * k))
        .flex()
        .justify_center()
        .text_size(px(15.0 * k))
        .text_color(Hsla { a: c.desc.a * f.desc_alpha, ..c.desc })
        .child(splash::TAGLINE);
    gpui::div().relative().w(px(w)).h(px(h)).child(stage).child(desc).into_any_element()
}
