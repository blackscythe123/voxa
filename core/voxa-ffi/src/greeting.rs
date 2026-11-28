//! Home's greeting, its doodle and the written splash, for Kotlin (voxa-core's `greeting`,
//! `doodle` and `splash`). The phone draws exactly what the desktop draws.

use std::sync::OnceLock;

use voxa_core::doodle::{self, Kind, Tag};
use voxa_core::greeting::{self, Date, Doodle, Ledger, MonthDay};
use voxa_core::splash;
use voxa_store::{History, Status};

use crate::VoxaCore;

const LEDGER_FILE: &str = "greeting.txt";

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum StrokeKind {
    Ink,
    Ground,
    Hatch,
    Wash,
    Dot,
}

#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum StrokeTag {
    None,
    Twinkle,
    Flame,
    Steam,
    Ray,
}

/// One pen stroke of a doodle, in the 343 x 112 band.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DoodleStroke {
    pub kind: StrokeKind,
    /// Absolute M, L, C and Z only (for `PathParser`).
    pub d: String,
    pub start_ms: u32,
    pub dur_ms: u32,
    pub tag: StrokeTag,
    pub pivot_x: f32,
    pub pivot_y: f32,
    /// The long ground line at (6, 92) (voxa-core's `is_horizon`): the phone's Home leaves it out.
    pub horizon: bool,
}

impl From<&doodle::Stroke> for DoodleStroke {
    fn from(s: &doodle::Stroke) -> Self {
        Self {
            kind: match s.kind {
                Kind::Ink => StrokeKind::Ink,
                Kind::Ground => StrokeKind::Ground,
                Kind::Hatch => StrokeKind::Hatch,
                Kind::Wash => StrokeKind::Wash,
                Kind::Dot => StrokeKind::Dot,
            },
            d: s.d.clone(),
            start_ms: s.start_ms,
            dur_ms: s.dur_ms,
            tag: match s.tag {
                Tag::None => StrokeTag::None,
                Tag::Twinkle => StrokeTag::Twinkle,
                Tag::Flame => StrokeTag::Flame,
                Tag::Steam => StrokeTag::Steam,
                Tag::Ray => StrokeTag::Ray,
            },
            pivot_x: s.pivot.0,
            pivot_y: s.pivot.1,
            horizon: doodle::is_horizon(s),
        }
    }
}

impl From<&DoodleStroke> for doodle::Stroke {
    fn from(s: &DoodleStroke) -> Self {
        Self {
            kind: match s.kind {
                StrokeKind::Ink => Kind::Ink,
                StrokeKind::Ground => Kind::Ground,
                StrokeKind::Hatch => Kind::Hatch,
                StrokeKind::Wash => Kind::Wash,
                StrokeKind::Dot => Kind::Dot,
            },
            d: s.d.clone(),
            start_ms: s.start_ms,
            dur_ms: s.dur_ms,
            tag: match s.tag {
                StrokeTag::None => Tag::None,
                StrokeTag::Twinkle => Tag::Twinkle,
                StrokeTag::Flame => Tag::Flame,
                StrokeTag::Steam => Tag::Steam,
                StrokeTag::Ray => Tag::Ray,
            },
            pivot: (s.pivot_x, s.pivot_y),
        }
    }
}

/// What Home's header shows.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HomeGreeting {
    /// "Good afternoon", "7 days running".
    pub lead: String,
    /// The first name, when known.
    pub name: Option<String>,
    /// The doodle's key ("midday", "streak").
    pub doodle: String,
    /// The date-seeded variation for today (stable all day).
    pub seed: u32,
    pub tally: u32,
    pub moon_age: f64,
    pub moon_lit: f64,
    /// "2026-11-28".
    pub date: String,
    /// Whether the drawing should play: once per doodle per day. Call `doodle_played` when it does.
    pub play: bool,
    pub strokes: Vec<DoodleStroke>,
    /// The subject's scale (for the flourish).
    pub scale: f32,
    pub done_ms: u32,
    /// The drawing's own box [x0, y0, x1, y1] in band units, without the horizon and with the
    /// flourish's reach (voxa-core's `drawn_bounds`). The phone crops the doodle to it.
    pub bounds: Vec<f32>,
}

/// One doodle drawn for a day and a count, for tests and previews.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DoodleArt {
    pub strokes: Vec<DoodleStroke>,
    pub scale: f32,
    pub done_ms: u32,
    pub bounds: Vec<f32>,
}

/// The flourish's transform for one stroke: an SVG matrix [a, b, c, d, e, f] and an opacity.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FlourishFrame {
    pub matrix: Vec<f32>,
    pub alpha: f32,
}

/// One moment of the written splash, in the wordmark's 280 x 140 box.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SplashFrame {
    pub ink: f32,
    pub ink_alpha: f32,
    pub ink_width: f32,
    pub nib_x: f32,
    pub nib_y: f32,
    pub nib_rot: f32,
    pub nib_scale: f32,
    pub nib_alpha: f32,
    pub desc_alpha: f32,
    pub desc_dy: f32,
}

impl From<splash::Frame> for SplashFrame {
    fn from(f: splash::Frame) -> Self {
        Self {
            ink: f.ink,
            ink_alpha: f.ink_alpha,
            ink_width: f.ink_width,
            nib_x: f.nib_x,
            nib_y: f.nib_y,
            nib_rot: f.nib_rot,
            nib_scale: f.nib_scale,
            nib_alpha: f.nib_alpha,
            desc_alpha: f.desc_alpha,
            desc_dy: f.desc_dy,
        }
    }
}

/// The splash's shapes and timing.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SplashSpec {
    pub wordmark: String,
    pub nib: String,
    pub nib_slit: String,
    pub tagline: String,
    pub total_ms: f32,
    pub crossfade_ms: f32,
    pub reduced_hold_ms: f32,
}

fn wordmark() -> &'static splash::Wordmark {
    static W: OnceLock<splash::Wordmark> = OnceLock::new();
    W.get_or_init(splash::Wordmark::new)
}

fn doodle_of(key: &str) -> Doodle {
    Doodle::ALL.into_iter().find(|d| d.key() == key).unwrap_or(Doodle::Midday)
}

/// Words per local day from History (written dictations only).
pub(crate) fn words_per_day(entries: &[voxa_store::Entry], offset_ms: i64) -> Vec<(Date, u64)> {
    let mut days: Vec<(Date, u64)> = Vec::new();
    for e in entries.iter().filter(|e| matches!(e.status, Status::Pasted | Status::Ready)) {
        let w = e.words() as u64;
        if w == 0 {
            continue;
        }
        let d = Date::from_days((e.id as i64 + offset_ms).div_euclid(86_400_000));
        match days.iter_mut().find(|x| x.0 == d) {
            Some(x) => x.1 += w,
            None => days.push((d, w)),
        }
    }
    days
}

#[uniffi::export]
impl VoxaCore {
    /// Picks Home's greeting and doodle for the local `date` ("2026-11-28") and time
    /// (`minutes` after midnight). `birthday_month` and `birthday_day` are 0 when not set.
    /// `installed`: the install date, when known. Keeps the greeting ledger in the data folder.
    #[allow(clippy::too_many_arguments)]
    pub fn home_greeting(
        &self,
        date: String,
        minutes: u32,
        utc_offset_minutes: i32,
        name: Option<String>,
        birthday_month: u8,
        birthday_day: u8,
        installed: Option<String>,
        first_open: bool,
    ) -> HomeGreeting {
        let today = Date::parse(&date).unwrap_or(Date::new(2026, 1, 1));
        let history = History::edit(&self.dir, |h| (false, words_per_day(&h.entries, utc_offset_minutes as i64 * 60_000)));
        let path = self.dir.join(LEDGER_FILE);
        let mut ledger = Ledger::parse(&std::fs::read_to_string(&path).unwrap_or_default());
        ledger.merge(today, &history);
        let birthday = Some(MonthDay { month: birthday_month, day: birthday_day }).filter(|b| b.valid());
        let g = greeting::greet(&greeting::Inputs {
            today,
            minutes: (minutes % 1440) as u16,
            name,
            birthday,
            installed: installed.as_deref().and_then(Date::parse),
            first_open,
            usage: ledger.usage(),
        });
        ledger.mark_shown(&g);
        let play = ledger.played.as_ref() != Some(&(g.doodle.key().to_string(), today));
        let _ = voxa_store::write_atomic(&path, ledger.to_text().as_bytes());
        let drawing = doodle::draw(g.doodle, today, g.tally, doodle::BAND_W, 0.0);
        HomeGreeting {
            lead: g.lead.clone(),
            name: g.name.clone(),
            doodle: g.doodle.key().to_string(),
            seed: g.seed,
            tally: g.tally,
            moon_age: g.moon_age,
            moon_lit: g.moon_lit,
            date: today.to_string(),
            play,
            strokes: drawing.strokes.iter().map(DoodleStroke::from).collect(),
            scale: drawing.scale,
            done_ms: drawing.done_ms,
            bounds: doodle::drawn_bounds(g.doodle, &drawing).to_vec(),
        }
    }

    /// The doodle played its drawing today: later visits show it finished.
    pub fn doodle_played(&self, doodle: String, date: String) {
        let Some(today) = Date::parse(&date) else { return };
        let path = self.dir.join(LEDGER_FILE);
        let mut ledger = Ledger::parse(&std::fs::read_to_string(&path).unwrap_or_default());
        if ledger.should_play(doodle_of(&doodle), today) {
            let _ = voxa_store::write_atomic(&path, ledger.to_text().as_bytes());
        }
    }
}

/// `doodle` ("night") drawn for `date` ("2026-11-28") with `count` marks, in the phone band.
#[uniffi::export]
pub fn doodle_art(doodle: String, date: String, count: u32) -> DoodleArt {
    let d = doodle_of(&doodle);
    let drawing = doodle::draw(d, Date::parse(&date).unwrap_or(Date::new(2026, 1, 1)), count, doodle::BAND_W, 0.0);
    DoodleArt {
        strokes: drawing.strokes.iter().map(DoodleStroke::from).collect(),
        scale: drawing.scale,
        done_ms: drawing.done_ms,
        bounds: doodle::drawn_bounds(d, &drawing).to_vec(),
    }
}

/// The flourish transform for `stroke` at `t_ms`, or None when it isn't moving.
#[uniffi::export]
pub fn doodle_flourish(doodle: String, stroke: DoodleStroke, scale: f32, t_ms: f32) -> Option<FlourishFrame> {
    doodle::flourish_at(doodle_of(&doodle), &(&stroke).into(), scale, t_ms).map(|(m, alpha)| FlourishFrame { matrix: m.to_vec(), alpha })
}

#[uniffi::export]
pub fn splash_spec() -> SplashSpec {
    SplashSpec {
        wordmark: splash::WORDMARK.into(),
        nib: splash::NIB.into(),
        nib_slit: splash::NIB_SLIT.into(),
        tagline: splash::TAGLINE.into(),
        total_ms: splash::TOTAL_MS,
        crossfade_ms: splash::CROSSFADE_MS,
        reduced_hold_ms: splash::REDUCED_HOLD_MS,
    }
}

/// The written splash at `t_ms` (variant B).
#[uniffi::export]
pub fn splash_frame(t_ms: f32) -> SplashFrame {
    splash::frame(wordmark(), t_ms).into()
}

/// The finished splash with no nib (reduced motion).
#[uniffi::export]
pub fn splash_end_frame() -> SplashFrame {
    splash::end_frame(wordmark()).into()
}
