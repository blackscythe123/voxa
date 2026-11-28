//! A small SVG path reader for the doodles and the splash wordmark.
//!
//! Reads every path command (M L H V C S Q T A Z, absolute and relative) and normalises it to
//! absolute move, line, cubic and close, so each shell needs only those four: Android hands the
//! normalised string to `PathParser`, the desktop feeds the segments to GPUI's `PathBuilder`.
//! It also flattens paths to polylines, which is how hatching is clipped (GPUI can't clip a path
//! to a shape) and how the splash finds the nib's point and direction along the wordmark.

use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    /// Two control points, then the end point.
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Parses an SVG path. Unknown characters end the parse (the doodle table is checked by tests).
pub fn parse(d: &str) -> Vec<Seg> {
    let mut t = Tokens { s: d.as_bytes(), i: 0 };
    let mut out = Vec::new();
    let (mut cx, mut cy) = (0f32, 0f32); // current point
    let (mut sx, mut sy) = (0f32, 0f32); // subpath start
    // Last control point, for S and T reflections.
    let mut last_c: Option<(f32, f32)> = None;
    let mut last_q: Option<(f32, f32)> = None;
    let mut cmd = b'M';
    loop {
        t.skip_ws();
        if t.i >= t.s.len() {
            break;
        }
        let c = t.s[t.i];
        if c.is_ascii_alphabetic() {
            cmd = c;
            t.i += 1;
        } else if cmd == b'Z' || cmd == b'z' {
            break; // numbers after Z: malformed
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (Some(x), Some(y)) = (t.num(), t.num()) else { break };
                cx = ox + x;
                cy = oy + y;
                sx = cx;
                sy = cy;
                // A move straight after a move replaces it (the doodles' "M x y m -r 0" circles).
                if let Some(Seg::Move(..)) = out.last() {
                    out.pop();
                }
                out.push(Seg::Move(cx, cy));
                // Further pairs after M are line-tos.
                cmd = if rel { b'l' } else { b'L' };
                last_c = None;
                last_q = None;
            }
            b'L' => {
                let (Some(x), Some(y)) = (t.num(), t.num()) else { break };
                cx = ox + x;
                cy = oy + y;
                out.push(Seg::Line(cx, cy));
                last_c = None;
                last_q = None;
            }
            b'H' => {
                let Some(x) = t.num() else { break };
                cx = if rel { cx + x } else { x };
                out.push(Seg::Line(cx, cy));
                last_c = None;
                last_q = None;
            }
            b'V' => {
                let Some(y) = t.num() else { break };
                cy = if rel { cy + y } else { y };
                out.push(Seg::Line(cx, cy));
                last_c = None;
                last_q = None;
            }
            b'C' => {
                let v = t.nums::<6>();
                let Some([x1, y1, x2, y2, x, y]) = v else { break };
                let (x1, y1, x2, y2, x, y) = (ox + x1, oy + y1, ox + x2, oy + y2, ox + x, oy + y);
                out.push(Seg::Cubic(x1, y1, x2, y2, x, y));
                last_c = Some((x2, y2));
                last_q = None;
                cx = x;
                cy = y;
            }
            b'S' => {
                let Some([x2, y2, x, y]) = t.nums::<4>() else { break };
                let (x1, y1) = match last_c {
                    Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                    None => (cx, cy),
                };
                let (x2, y2, x, y) = (ox + x2, oy + y2, ox + x, oy + y);
                out.push(Seg::Cubic(x1, y1, x2, y2, x, y));
                last_c = Some((x2, y2));
                last_q = None;
                cx = x;
                cy = y;
            }
            b'Q' => {
                let Some([qx, qy, x, y]) = t.nums::<4>() else { break };
                let (qx, qy, x, y) = (ox + qx, oy + qy, ox + x, oy + y);
                out.push(quad(cx, cy, qx, qy, x, y));
                last_q = Some((qx, qy));
                last_c = None;
                cx = x;
                cy = y;
            }
            b'T' => {
                let Some([x, y]) = t.nums::<2>() else { break };
                let (qx, qy) = match last_q {
                    Some((px, py)) => (2.0 * cx - px, 2.0 * cy - py),
                    None => (cx, cy),
                };
                let (x, y) = (ox + x, oy + y);
                out.push(quad(cx, cy, qx, qy, x, y));
                last_q = Some((qx, qy));
                last_c = None;
                cx = x;
                cy = y;
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rot), Some(large), Some(sweep), Some(x), Some(y)) =
                    (t.num(), t.num(), t.num(), t.flag(), t.flag(), t.num(), t.num())
                else {
                    break;
                };
                let (x, y) = (ox + x, oy + y);
                arc(&mut out, cx, cy, rx, ry, rot, large, sweep, x, y);
                cx = x;
                cy = y;
                last_c = None;
                last_q = None;
            }
            b'Z' => {
                out.push(Seg::Close);
                cx = sx;
                cy = sy;
                last_c = None;
                last_q = None;
            }
            _ => break,
        }
    }
    out
}

fn quad(x0: f32, y0: f32, qx: f32, qy: f32, x: f32, y: f32) -> Seg {
    Seg::Cubic(x0 + 2.0 / 3.0 * (qx - x0), y0 + 2.0 / 3.0 * (qy - y0), x + 2.0 / 3.0 * (qx - x), y + 2.0 / 3.0 * (qy - y), x, y)
}

/// SVG arc (endpoint form) as cubics, at most a quarter turn each (SVG 1.1 appendix F.6).
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Seg>, x1: f32, y1: f32, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool, x2: f32, y2: f32) {
    let (x1, y1, x2, y2) = (x1 as f64, y1 as f64, x2 as f64, y2 as f64);
    let (mut rx, mut ry) = ((rx as f64).abs(), (ry as f64).abs());
    if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        return;
    }
    if rx < 1e-9 || ry < 1e-9 {
        out.push(Seg::Line(x2 as f32, y2 as f32));
        return;
    }
    let phi = (rot_deg as f64).to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * -(ry * x1p / rx);
    let cx = cos * cxp - sin * cyp + (x1 + x2) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1 + y2) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = ang(1.0, 0.0, ux, uy);
    let mut delta = ang(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let n = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let pt = |t: f64| {
        let (s, c) = t.sin_cos();
        (cx + rx * c * cos - ry * s * sin, cy + rx * c * sin + ry * s * cos)
    };
    let dpt = |t: f64| {
        let (s, c) = t.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    for i in 0..n {
        let a = theta1 + step * i as f64;
        let b = a + step;
        let (p0x, p0y) = pt(a);
        let (p3x, p3y) = if i == n - 1 { (x2, y2) } else { pt(b) };
        let (d0x, d0y) = dpt(a);
        let (d1x, d1y) = dpt(b);
        out.push(Seg::Cubic(
            (p0x + k * d0x) as f32,
            (p0y + k * d0y) as f32,
            (p3x - k * d1x) as f32,
            (p3y - k * d1y) as f32,
            p3x as f32,
            p3y as f32,
        ));
    }
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn num(&mut self) -> Option<f32> {
        self.skip_ws();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'-' || s[i] == b'+') {
            i += 1;
        }
        let mut dot = false;
        let mut digits = false;
        while i < s.len() {
            match s[i] {
                b'0'..=b'9' => digits = true,
                b'.' if !dot => dot = true,
                b'e' | b'E' if digits && i + 1 < s.len() && (s[i + 1].is_ascii_digit() || s[i + 1] == b'-' || s[i + 1] == b'+') => {
                    i += 2;
                    while i < s.len() && s[i].is_ascii_digit() {
                        i += 1;
                    }
                    break;
                }
                _ => break,
            }
            i += 1;
        }
        if !digits {
            return None;
        }
        self.i = i;
        std::str::from_utf8(&s[start..i]).ok()?.parse().ok()
    }

    /// An arc flag: a single 0 or 1, which may be written with no separator ("a5 5 0 1010 10").
    fn flag(&mut self) -> Option<bool> {
        self.skip_ws();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }

    fn nums<const N: usize>(&mut self) -> Option<[f32; N]> {
        let mut a = [0f32; N];
        for v in a.iter_mut() {
            *v = self.num()?;
        }
        Some(a)
    }
}

/// `x' = a x + c y + e`, `y' = b x + d y + f` (the SVG matrix order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Scale by `s` about (ox, oy), then move by (dx, dy).
    pub fn scale_about(s: f32, ox: f32, oy: f32, dx: f32, dy: f32) -> Self {
        Affine([s, 0.0, 0.0, s, ox - ox * s + dx, oy - oy * s + dy])
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
}

pub fn transform(segs: &[Seg], m: &Affine) -> Vec<Seg> {
    segs.iter()
        .map(|s| match *s {
            Seg::Move(x, y) => {
                let (x, y) = m.apply(x, y);
                Seg::Move(x, y)
            }
            Seg::Line(x, y) => {
                let (x, y) = m.apply(x, y);
                Seg::Line(x, y)
            }
            Seg::Cubic(a, b, c, d, e, f) => {
                let (a, b) = m.apply(a, b);
                let (c, d) = m.apply(c, d);
                let (e, f) = m.apply(e, f);
                Seg::Cubic(a, b, c, d, e, f)
            }
            Seg::Close => Seg::Close,
        })
        .collect()
}

/// Absolute M, L, C and Z only, two decimals at most.
pub fn to_string(segs: &[Seg]) -> String {
    let mut s = String::new();
    let n = |s: &mut String, v: f32| {
        let r = (v * 100.0).round() / 100.0;
        let r = if r == 0.0 { 0.0 } else { r }; // no "-0"
        if !s.is_empty() && !s.ends_with(|c: char| c.is_ascii_alphabetic()) && r >= 0.0 {
            s.push(' ');
        }
        let _ = write!(s, "{r}");
    };
    for seg in segs {
        match *seg {
            Seg::Move(x, y) => {
                s.push('M');
                n(&mut s, x);
                n(&mut s, y);
            }
            Seg::Line(x, y) => {
                s.push('L');
                n(&mut s, x);
                n(&mut s, y);
            }
            Seg::Cubic(a, b, c, d, e, f) => {
                s.push('C');
                for v in [a, b, c, d, e, f] {
                    n(&mut s, v);
                }
            }
            Seg::Close => s.push('Z'),
        }
    }
    s
}

/// The path as polylines, one per subpath (closed ones end at their start). Cubics are cut into
/// pieces no longer than about `tol` units.
pub fn flatten(segs: &[Seg], tol: f32) -> Vec<Vec<(f32, f32)>> {
    let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let (mut cx, mut cy, mut sx, mut sy) = (0f32, 0f32, 0f32, 0f32);
    for seg in segs {
        match *seg {
            Seg::Move(x, y) => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                cur.push((x, y));
                (cx, cy, sx, sy) = (x, y, x, y);
            }
            Seg::Line(x, y) => {
                if cur.is_empty() {
                    cur.push((cx, cy));
                }
                cur.push((x, y));
                (cx, cy) = (x, y);
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                if cur.is_empty() {
                    cur.push((cx, cy));
                }
                let hull = dist(cx, cy, x1, y1) + dist(x1, y1, x2, y2) + dist(x2, y2, x, y);
                let n = ((hull / tol.max(0.01)).ceil() as usize).clamp(1, 256);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    let px = u * u * u * cx + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x;
                    let py = u * u * u * cy + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y;
                    cur.push((px, py));
                }
                (cx, cy) = (x, y);
            }
            Seg::Close => {
                if !cur.is_empty() {
                    if cur.last() != Some(&(sx, sy)) {
                        cur.push((sx, sy));
                    }
                    out.push(std::mem::take(&mut cur));
                }
                (cx, cy) = (sx, sy);
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

/// A polyline measured by length: the point and direction at any distance along it.
#[derive(Clone, Debug)]
pub struct Measured {
    pts: Vec<(f32, f32)>,
    /// Cumulative length at each point.
    acc: Vec<f32>,
}

impl Measured {
    pub fn new(pts: Vec<(f32, f32)>) -> Self {
        let mut acc = Vec::with_capacity(pts.len());
        let mut l = 0.0;
        for i in 0..pts.len() {
            if i > 0 {
                l += dist(pts[i - 1].0, pts[i - 1].1, pts[i].0, pts[i].1);
            }
            acc.push(l);
        }
        Self { pts, acc }
    }

    pub fn length(&self) -> f32 {
        self.acc.last().copied().unwrap_or(0.0)
    }

    /// The point at distance `at` (clamped to the ends).
    pub fn point_at(&self, at: f32) -> (f32, f32) {
        if self.pts.is_empty() {
            return (0.0, 0.0);
        }
        let at = at.clamp(0.0, self.length());
        let i = self.acc.partition_point(|&a| a < at).clamp(1, self.pts.len().max(2) - 1);
        if self.pts.len() == 1 {
            return self.pts[0];
        }
        let (a, b) = (self.acc[i - 1], self.acc[i]);
        let t = if b > a { (at - a) / (b - a) } else { 0.0 };
        let (p, q) = (self.pts[i - 1], self.pts[i]);
        (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_command_and_keeps_only_absolute_m_l_c_z() {
        let s = to_string(&parse("M10 20 h5 v5 H0 V0 l1 1 q2 -3 4 0 t4 0 c1 1 2 2 3 3 s1 1 2 2 C1 2 3 4 5 6 S7 8 9 10 Q1 1 2 2 T3 3 z"));
        assert!(s.chars().filter(|c| c.is_ascii_alphabetic()).all(|c| "MLCZ".contains(c)), "{s}");
        assert!(s.starts_with("M10 20L15 20L15 25L0 25L0 0L1 1C"), "{s}");
    }

    #[test]
    fn relative_moves_and_implicit_line_tos() {
        let s = to_string(&parse("m10 10 5 0 0 5z m2 2 l1 0"));
        assert_eq!(s, "M10 10L15 10L15 15ZM12 12L13 12");
    }

    #[test]
    fn circle_from_two_arcs_has_the_right_size() {
        // The doodles' circle: M x y m -r 0 a r r 0 1 0 2r 0 a r r 0 1 0 -2r 0
        let segs = parse("M50 50 m-10 0 a10 10 0 1 0 20 0 a10 10 0 1 0 -20 0");
        let poly = &flatten(&segs, 0.2)[0];
        for &(x, y) in poly {
            let r = dist(50.0, 50.0, x, y);
            assert!((r - 10.0).abs() < 0.05, "point {x},{y} is {r} from the centre");
        }
        let len = Measured::new(poly.clone()).length();
        assert!((len - std::f32::consts::TAU * 10.0).abs() < 0.3, "{len}");
    }

    #[test]
    fn compact_arc_flags() {
        // "a5 5 0 1010 0": flags 1 and 0 written together, then x=10.
        let a = to_string(&parse("M0 0 a5 5 0 1010 0"));
        let b = to_string(&parse("M0 0 a5 5 0 1 0 10 0"));
        assert_eq!(a, b);
    }

    #[test]
    fn half_circle_arc_goes_over_the_top() {
        // The sunrise: M118 92 A34 34 0 0 1 186 92 bulges upward (smaller y).
        let poly = &flatten(&parse("M118 92 A34 34 0 0 1 186 92"), 0.5)[0];
        let top = poly.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        assert!((top - 58.0).abs() < 0.1, "{top}");
    }

    #[test]
    fn measured_points() {
        let m = Measured::new(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert_eq!(m.length(), 20.0);
        assert_eq!(m.point_at(5.0), (5.0, 0.0));
        assert_eq!(m.point_at(15.0), (10.0, 5.0));
        assert_eq!(m.point_at(99.0), (10.0, 10.0));
    }
}
