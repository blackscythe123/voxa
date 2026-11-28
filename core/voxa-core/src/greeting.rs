//! Home's greeting and which doodle draws above it (docs/ui-mocks/v7-home-greeting-and-splash.html,
//! "The greeting pool").
//!
//! Picked once when Home opens, by the first rule that matches: your birthday, New Year, a
//! milestone (first dictation, words, a streak, a year with Voxa), back after a week away, the first
//! open after setup, the day of the week, then the time of day. One time-of-day table drives both
//! the greeting and the doodle, so a greeting always names the part of the day its doodle draws.
//! Occasions are only New Year and the user's own birthday: nothing tied to a country, a religion
//! or a region.
//!
//! Everything is a pure function of the inputs (no clock, no files), so both shells get the same
//! answer and every rule is tested here. The shells keep a small [`Ledger`] (words per day and the
//! milestones already shown) so milestones survive History being pruned.

use std::fmt;

/// A calendar date (proleptic Gregorian).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub y: i32,
    pub m: u8,
    pub d: u8,
}

impl Date {
    pub const fn new(y: i32, m: u8, d: u8) -> Self {
        Self { y, m, d }
    }

    /// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    pub fn days(&self) -> i64 {
        let y = self.y as i64 - if self.m <= 2 { 1 } else { 0 };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = self.m as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    pub fn from_days(z: i64) -> Self {
        let z = z + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;
        Self { y, m, d }
    }

    pub fn plus(&self, days: i64) -> Self {
        Self::from_days(self.days() + days)
    }

    /// 0 = Sunday ... 6 = Saturday.
    pub fn weekday(&self) -> u8 {
        ((self.days() + 4).rem_euclid(7)) as u8
    }

    pub fn is_leap(y: i32) -> bool {
        (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
    }

    /// The same month and day in another year; 29 February becomes 28 February in other years.
    pub fn in_year(&self, y: i32) -> Self {
        if self.m == 2 && self.d == 29 && !Self::is_leap(y) { Self::new(y, 2, 28) } else { Self::new(y, self.m, self.d) }
    }

    /// "2026-11-28".
    pub fn parse(s: &str) -> Option<Self> {
        let mut it = s.trim().splitn(3, '-');
        let y = it.next()?.parse().ok()?;
        let m: u8 = it.next()?.parse().ok()?;
        let d: u8 = it.next()?.parse().ok()?;
        if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
            return None;
        }
        Some(Self::new(y, m, d))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
}

pub fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        2 if Date::is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A birthday: day and month only, never the year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthDay {
    pub month: u8,
    pub day: u8,
}

impl MonthDay {
    /// Whether `date` is this birthday. 29 February is celebrated on 28 February in other years.
    pub fn is(&self, date: Date) -> bool {
        if self.month == 2 && self.day == 29 && !Date::is_leap(date.y) {
            return date.m == 2 && date.d == 28;
        }
        date.m == self.month && date.d == self.day
    }

    pub fn valid(&self) -> bool {
        (1..=12).contains(&self.month) && self.day >= 1 && self.day <= days_in_month(2024, self.month)
    }
}

/// Which drawing goes above the greeting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Doodle {
    Sunrise,
    Morning,
    Midday,
    Evening,
    Night,
    Weekend,
    Streak,
    Words,
    First,
    Birthday,
    NewYear,
}

impl Doodle {
    pub const ALL: [Doodle; 11] = [
        Doodle::Sunrise,
        Doodle::Morning,
        Doodle::Midday,
        Doodle::Evening,
        Doodle::Night,
        Doodle::Weekend,
        Doodle::Streak,
        Doodle::Words,
        Doodle::First,
        Doodle::Birthday,
        Doodle::NewYear,
    ];

    /// The key the mockup uses; it also salts the daily seed.
    pub fn key(&self) -> &'static str {
        match self {
            Doodle::Sunrise => "sunrise",
            Doodle::Morning => "morning",
            Doodle::Midday => "midday",
            Doodle::Evening => "evening",
            Doodle::Night => "night",
            Doodle::Weekend => "weekend",
            Doodle::Streak => "streak",
            Doodle::Words => "words",
            Doodle::First => "first",
            Doodle::Birthday => "birthday",
            Doodle::NewYear => "newyear",
        }
    }
}

/// One row of the time-of-day table: from and to in minutes after midnight.
pub struct Slot {
    pub from: u16,
    pub to: u16,
    pub doodle: Doodle,
    pub lines: &'static [&'static str],
}

/// The one time-of-day table (the mockup's SLOTS). Sunrise 5 to 8, Morning 8 to 11:30, Midday
/// 11:30 to 4 pm ("Hello" before noon, "Good afternoon" after), Evening 4 to 7 pm, Night 7 pm to
/// 5 am ("Good evening" until 10 pm, then "Night owl", "Late one" or "Hey there").
pub const SLOTS: &[Slot] = &[
    Slot { from: 0, to: 300, doodle: Doodle::Night, lines: &["Still up", "Hey there"] },
    Slot { from: 300, to: 480, doodle: Doodle::Sunrise, lines: &["Early start", "Up early"] },
    Slot { from: 480, to: 690, doodle: Doodle::Morning, lines: &["Good morning", "Morning"] },
    Slot { from: 690, to: 720, doodle: Doodle::Midday, lines: &["Hello", "Hey there"] },
    Slot { from: 720, to: 960, doodle: Doodle::Midday, lines: &["Good afternoon", "Hey there"] },
    Slot { from: 960, to: 1140, doodle: Doodle::Evening, lines: &["Good evening", "Long day"] },
    Slot { from: 1140, to: 1320, doodle: Doodle::Night, lines: &["Good evening", "Evening"] },
    Slot { from: 1320, to: 1440, doodle: Doodle::Night, lines: &["Night owl", "Late one", "Hey there"] },
];

pub fn slot_at(minutes: u16) -> &'static Slot {
    let m = minutes % 1440;
    SLOTS.iter().find(|s| m >= s.from && m < s.to).unwrap_or(&SLOTS[0])
}

/// The daily seed: FNV-1a over the text (the mockup's `seedOf(date + key)`), so a day's drawing is
/// the same all day, on every device.
pub fn seed_of(text: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in text.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// The mockup's tiny generator (mulberry32), so variations match it exactly.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// In [0, 1).
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let s = self.0;
        let mut t = (s ^ (s >> 15)).wrapping_mul(1 | s);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }

    pub fn pick<T: Copy>(&mut self, list: &[T]) -> T {
        list[((self.next() * list.len() as f64) as usize).min(list.len() - 1)]
    }

    /// `n` different items, in the order drawn.
    pub fn pick_n<T: Copy>(&mut self, list: &[T], n: usize) -> Vec<T> {
        let mut a = list.to_vec();
        let mut out = Vec::new();
        for _ in 0..n {
            if a.is_empty() {
                break;
            }
            let i = ((self.next() * a.len() as f64) as usize).min(a.len() - 1);
            out.push(a.remove(i));
        }
        out
    }
}

/// The length of a lunar month in days.
pub const SYNODIC: f64 = 29.530589;

/// The moon's age in days for the night of `date`: counted from the new moon of 6 January 2000,
/// 18:14 UTC, taken at 20:00 UTC (the mockup's `moonAge`).
pub fn moon_age(date: Date) -> f64 {
    let t = date.days() as f64 + 20.0 / 24.0;
    let reference = Date::new(2000, 1, 6).days() as f64 + (18.0 + 14.0 / 60.0) / 24.0;
    (t - reference).rem_euclid(SYNODIC)
}

/// How much of the moon is lit, 0 (new) to 1 (full).
pub fn moon_lit(age: f64) -> f64 {
    (1.0 - (std::f64::consts::TAU * age / SYNODIC).cos()) / 2.0
}

/// What the user has done with Voxa, from the [`Ledger`].
#[derive(Clone, Debug, Default)]
pub struct Usage {
    /// Words per local day, for every day with a dictation (any order).
    pub days: Vec<(Date, u64)>,
    /// Milestones on or before this day are not celebrated (they may predate what the ledger
    /// knows, for example after History was pruned). See [`Ledger::since`].
    pub since: Option<Date>,
    /// Milestones already shown, with the day they were first shown.
    pub shown: Vec<(String, Date)>,
}

/// Everything the greeting depends on.
#[derive(Clone, Debug)]
pub struct Inputs {
    pub today: Date,
    /// Local time, minutes after midnight.
    pub minutes: u16,
    /// First name, if known. Without one the greeting stands alone ("Good afternoon").
    pub name: Option<String>,
    pub birthday: Option<MonthDay>,
    /// When Voxa was installed (for "One year with Voxa").
    pub installed: Option<Date>,
    /// The first time Home opens after setup.
    pub first_open: bool,
    pub usage: Usage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Greeting {
    /// "Good afternoon", "7 days running", "Happy birthday".
    pub lead: String,
    pub name: Option<String>,
    pub doodle: Doodle,
    /// The date-seeded variation: seed_of(date + doodle key).
    pub seed: u32,
    /// For the streak doodle: how many marks the tally draws (days, or weeks above 60 days).
    pub tally: u32,
    /// Moon age in days and how much is lit, for the night drawn on `today`.
    pub moon_age: f64,
    pub moon_lit: f64,
    /// The milestone this greeting celebrates; the shell records it as shown today.
    pub milestone: Option<String>,
    pub date: Date,
}

impl Greeting {
    /// "Good afternoon, Sam", or "Good afternoon" with no name.
    pub fn line(&self) -> String {
        match &self.name {
            Some(n) => format!("{}, {}", self.lead, n),
            None => self.lead.clone(),
        }
    }
}

/// 1,000, 10,000 and 100,000 words.
pub const WORD_MILESTONES: [u64; 3] = [1_000, 10_000, 100_000];

/// A streak worth a greeting: 7, 14 and 21 days, then every 30 (30, 60, 90 ...).
pub fn is_streak_milestone(n: u32) -> bool {
    matches!(n, 7 | 14 | 21) || (n >= 30 && n % 30 == 0)
}

/// A milestone shows on its day, or, if it was missed (Home not opened, or a birthday or New Year
/// took the day), on the first ordinary day within this many days after.
pub const MILESTONE_GRACE_DAYS: i64 = 3;

#[derive(Clone, Debug, PartialEq)]
struct Milestone {
    key: String,
    day: Date,
    lead: String,
    doodle: Doodle,
    tally: u32,
    rank: u8,
}

fn milestones(i: &Inputs) -> Vec<Milestone> {
    let mut days: Vec<(Date, u64)> = i.usage.days.iter().copied().filter(|(_, w)| *w > 0).collect();
    days.sort_by_key(|d| d.0);
    days.dedup_by_key(|d| d.0);
    let since = i.usage.since;
    let after_since = |d: Date| since.is_none_or(|s| d > s);
    let mut out = Vec::new();
    if let Some(&(first, _)) = days.first() {
        if after_since(first) {
            out.push(Milestone { key: format!("first@{first}"), day: first, lead: "First words".into(), doodle: Doodle::First, tally: 0, rank: 0 });
        }
    }
    let mut total = 0u64;
    for &(d, w) in &days {
        let before = total;
        total += w;
        for &m in &WORD_MILESTONES {
            if before < m && total >= m && after_since(d) {
                out.push(Milestone { key: format!("words{m}"), day: d, lead: format!("{} words spoken", thousands(m)), doodle: Doodle::Words, tally: 0, rank: 1 });
            }
        }
    }
    // Streaks: each run of consecutive days, and the day it reached each milestone length.
    let mut run = 0u32;
    let mut prev: Option<Date> = None;
    for &(d, _) in &days {
        run = if prev.is_some_and(|p| p.plus(1) == d) { run + 1 } else { 1 };
        prev = Some(d);
        if is_streak_milestone(run) {
            let tally = if run > 60 { run / 7 } else { run };
            out.push(Milestone { key: format!("streak{run}@{d}"), day: d, lead: format!("{run} days running"), doodle: Doodle::Streak, tally, rank: 2 });
        }
    }
    if let Some(inst) = i.installed {
        let k = i.today.y - inst.y;
        for k in [k - 1, k] {
            if k >= 1 {
                let day = inst.in_year(inst.y + k);
                let lead = if k == 1 { "One year with Voxa".to_string() } else { format!("{k} years with Voxa") };
                out.push(Milestone { key: format!("year{k}"), day, lead, doodle: Doodle::Streak, tally: 52, rank: 3 });
            }
        }
    }
    out
}

/// The milestone to celebrate today, if any: due today or missed within the grace days, not shown
/// on an earlier day. Ties go to the more recent, then first words, words, streak, year.
fn due_milestone(i: &Inputs) -> Option<Milestone> {
    let today = i.today;
    let mut due: Vec<Milestone> = milestones(i)
        .into_iter()
        .filter(|m| m.day <= today && today.days() - m.day.days() <= MILESTONE_GRACE_DAYS)
        .filter(|m| !i.usage.shown.iter().any(|(k, d)| *k == m.key && *d < today))
        .collect();
    due.sort_by(|a, b| b.day.cmp(&a.day).then(a.rank.cmp(&b.rank)));
    due.into_iter().next()
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Picks the greeting and the doodle. The first rule that matches wins.
pub fn greet(i: &Inputs) -> Greeting {
    let today = i.today;
    let minutes = i.minutes % 1440;
    let slot = slot_at(minutes);
    // Rotates by date within a rule with more than one line.
    let rot = |lines: &[&str]| lines[(today.days().rem_euclid(lines.len() as i64)) as usize].to_string();
    let dow = today.weekday();
    let (lead, doodle, tally, milestone) = if i.birthday.is_some_and(|b| b.valid() && b.is(today)) {
        ("Happy birthday".to_string(), Doodle::Birthday, 0, None)
    } else if today.m == 1 && today.d == 1 {
        ("Happy New Year".to_string(), Doodle::NewYear, 0, None)
    } else if let Some(m) = due_milestone(i) {
        (m.lead, m.doodle, m.tally, Some(m.key))
    } else if away_a_week(i) {
        ("Welcome back".to_string(), slot.doodle, 0, None)
    } else if i.first_open {
        ("Hello there".to_string(), slot.doodle, 0, None)
    } else if dow == 6 && (300..720).contains(&minutes) {
        ("Saturday morning".to_string(), Doodle::Weekend, 0, None)
    } else if dow == 0 && (300..720).contains(&minutes) {
        (rot(&["Easy Sunday", "Slow Sunday"]), Doodle::Weekend, 0, None)
    } else if dow == 1 && (300..690).contains(&minutes) {
        ("Fresh week".to_string(), slot.doodle, 0, None)
    } else if dow == 5 && minutes >= 960 {
        ("Nearly weekend".to_string(), slot.doodle, 0, None)
    } else {
        (rot(slot.lines), slot.doodle, 0, None)
    };
    let age = moon_age(today);
    Greeting {
        lead,
        name: i.name.as_ref().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        doodle,
        seed: seed_of(&format!("{today}{}", doodle.key())),
        tally,
        moon_age: age,
        moon_lit: moon_lit(age),
        milestone,
        date: today,
    }
}

/// The last dictation before today was 7 or more days ago.
fn away_a_week(i: &Inputs) -> bool {
    let last = i.usage.days.iter().filter(|(d, w)| *d < i.today && *w > 0).map(|(d, _)| *d).max();
    last.is_some_and(|l| i.today.days() - l.days() >= 7)
}

/// What a shell keeps between runs so milestones stay right after History is pruned: words per
/// day (each day's highest count seen), the day the ledger started, the milestones shown and the
/// day the doodle last played. Saved as plain text, one fact per line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ledger {
    pub since: Option<Date>,
    pub days: Vec<(Date, u64)>,
    pub shown: Vec<(String, Date)>,
    /// "night@2026-11-28": the doodle that last played its drawing, and the day.
    pub played: Option<(String, Date)>,
}

impl Ledger {
    pub fn parse(text: &str) -> Self {
        let mut l = Ledger::default();
        for line in text.lines() {
            let mut p = line.split_whitespace();
            match (p.next(), p.next(), p.next()) {
                (Some("since"), Some(d), None) => l.since = Date::parse(d),
                (Some("day"), Some(d), Some(w)) => {
                    if let (Some(d), Ok(w)) = (Date::parse(d), w.parse()) {
                        l.days.push((d, w));
                    }
                }
                (Some("shown"), Some(k), Some(d)) => {
                    if let Some(d) = Date::parse(d) {
                        l.shown.push((k.to_string(), d));
                    }
                }
                (Some("played"), Some(k), Some(d)) => l.played = Date::parse(d).map(|d| (k.to_string(), d)),
                _ => {}
            }
        }
        l
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        if let Some(d) = self.since {
            s += &format!("since {d}\n");
        }
        let mut days = self.days.clone();
        days.sort_by_key(|d| d.0);
        for (d, w) in days {
            s += &format!("day {d} {w}\n");
        }
        for (k, d) in &self.shown {
            s += &format!("shown {k} {d}\n");
        }
        if let Some((k, d)) = &self.played {
            s += &format!("played {k} {d}\n");
        }
        s
    }

    /// Folds in what History says now. Each day keeps the highest count seen, so days pruned from
    /// History are remembered. A new ledger starts today; milestones it can't vouch for (on or
    /// before that day) are not celebrated, unless History was empty, so a new user's first
    /// dictation today still counts.
    pub fn merge(&mut self, today: Date, history_days: &[(Date, u64)]) {
        if self.since.is_none() {
            self.since = Some(if history_days.iter().all(|(_, w)| *w == 0) { today.plus(-1) } else { today });
        }
        for &(d, w) in history_days {
            match self.days.iter_mut().find(|x| x.0 == d) {
                Some(x) => x.1 = x.1.max(w),
                None if w > 0 => self.days.push((d, w)),
                None => {}
            }
        }
        // Shown records older than the grace window can't matter any more.
        self.shown.retain(|(_, d)| today.days() - d.days() <= MILESTONE_GRACE_DAYS + 1);
    }

    pub fn usage(&self) -> Usage {
        Usage { days: self.days.clone(), since: self.since, shown: self.shown.clone() }
    }

    /// Records the milestone a greeting showed today (first showing only).
    pub fn mark_shown(&mut self, g: &Greeting) {
        if let Some(k) = &g.milestone {
            if !self.shown.iter().any(|(s, _)| s == k) {
                self.shown.push((k.clone(), g.date));
            }
        }
    }

    /// Whether this doodle should play its drawing now (once per doodle per day). Records it.
    pub fn should_play(&mut self, doodle: Doodle, today: Date) -> bool {
        let fresh = self.played.as_ref() != Some(&(doodle.key().to_string(), today));
        if fresh {
            self.played = Some((doodle.key().to_string(), today));
        }
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u8, d: u8, h: u16, min: u16) -> Inputs {
        Inputs { today: Date::new(y, m, d), minutes: h * 60 + min, name: Some("Sam".into()), birthday: None, installed: None, first_open: false, usage: Usage::default() }
    }

    // 2026-12-02 is a Wednesday: no weekday rule applies.
    fn wed(h: u16, min: u16) -> Greeting {
        greet(&at(2026, 12, 2, h, min))
    }

    #[test]
    fn dates_round_trip_and_know_their_weekday() {
        for z in -800_000..800_000i64 {
            if z % 997 == 0 {
                assert_eq!(Date::from_days(z).days(), z);
            }
        }
        assert_eq!(Date::new(1970, 1, 1).days(), 0);
        assert_eq!(Date::new(2026, 11, 28).weekday(), 6, "Saturday");
        assert_eq!(Date::new(2026, 11, 29).weekday(), 0, "Sunday");
        assert_eq!(Date::new(2000, 3, 1).plus(-1), Date::new(2000, 2, 29));
        assert_eq!(Date::parse("2026-02-30"), None);
        assert_eq!(Date::parse("2028-02-29"), Some(Date::new(2028, 2, 29)));
        assert_eq!(Date::new(2026, 3, 4).to_string(), "2026-03-04");
    }

    #[test]
    fn time_slot_boundaries() {
        let lead_doodle = |h, m| {
            let g = wed(h, m);
            (g.doodle, g.lead)
        };
        let day = Date::new(2026, 12, 2).days();
        let rot = |lines: &[&str]| lines[(day % lines.len() as i64) as usize].to_string();
        assert_eq!(lead_doodle(4, 59), (Doodle::Night, rot(&["Still up", "Hey there"])));
        assert_eq!(lead_doodle(5, 0), (Doodle::Sunrise, rot(&["Early start", "Up early"])));
        assert_eq!(lead_doodle(7, 59).0, Doodle::Sunrise);
        assert_eq!(lead_doodle(8, 0), (Doodle::Morning, rot(&["Good morning", "Morning"])));
        assert_eq!(lead_doodle(11, 29).0, Doodle::Morning);
        assert_eq!(lead_doodle(11, 30), (Doodle::Midday, rot(&["Hello", "Hey there"])));
        assert_eq!(lead_doodle(11, 59).0, Doodle::Midday);
        assert_eq!(lead_doodle(12, 0), (Doodle::Midday, rot(&["Good afternoon", "Hey there"])));
        assert_eq!(lead_doodle(15, 59).0, Doodle::Midday);
        assert_eq!(lead_doodle(16, 0), (Doodle::Evening, rot(&["Good evening", "Long day"])));
        assert_eq!(lead_doodle(18, 59).0, Doodle::Evening);
        assert_eq!(lead_doodle(19, 0), (Doodle::Night, rot(&["Good evening", "Evening"])));
        assert_eq!(lead_doodle(21, 59).0, Doodle::Night);
        assert_eq!(lead_doodle(22, 0), (Doodle::Night, rot(&["Night owl", "Late one", "Hey there"])));
        assert_eq!(lead_doodle(23, 59).0, Doodle::Night);
    }

    #[test]
    fn no_good_afternoon_before_noon_and_no_night_owl_before_ten() {
        for d in 0..60 {
            let date = Date::new(2026, 12, 1).plus(d);
            for m in (0..1440).step_by(5) {
                let mut i = at(2026, 1, 1, 0, 0);
                i.today = date;
                i.minutes = m;
                let g = greet(&i);
                if g.lead == "Good afternoon" {
                    assert!(m >= 720, "{date} {m}");
                }
                if g.lead == "Night owl" || g.lead == "Late one" {
                    assert!(m >= 1320, "{date} {m}");
                }
                if g.lead == "Good morning" {
                    assert!((480..690).contains(&m));
                }
            }
        }
    }

    #[test]
    fn lines_rotate_by_date_and_hold_all_day() {
        let a = wed(13, 0).lead;
        assert_eq!(wed(15, 59).lead, a, "same rule, same day, same line");
        let mut leads = std::collections::HashSet::new();
        for d in 0..4 {
            let mut i = at(2026, 12, 2, 13, 0);
            i.today = i.today.plus(d * 7); // Wednesdays
            leads.insert(greet(&i).lead);
        }
        assert_eq!(leads.len(), 2, "both afternoon lines appear over a few weeks: {leads:?}");
    }

    #[test]
    fn weekend_lines_follow_the_real_day() {
        let sat = greet(&at(2026, 11, 28, 9, 20));
        assert_eq!((sat.lead.as_str(), sat.doodle), ("Saturday morning", Doodle::Weekend));
        let sun = greet(&at(2026, 11, 29, 9, 20));
        assert!(sun.lead.ends_with("Sunday") && sun.doodle == Doodle::Weekend, "{sun:?}");
        assert_eq!(greet(&at(2026, 11, 29, 4, 59)).doodle, Doodle::Night, "before 5 am is still night");
        assert_eq!(greet(&at(2026, 11, 29, 12, 0)).doodle, Doodle::Midday, "weekend mornings end at noon");
        let mon = greet(&at(2026, 11, 30, 6, 0));
        assert_eq!((mon.lead.as_str(), mon.doodle), ("Fresh week", Doodle::Sunrise));
        assert_eq!(greet(&at(2026, 11, 30, 9, 0)).doodle, Doodle::Morning);
        assert_ne!(greet(&at(2026, 11, 30, 11, 30)).lead, "Fresh week");
        let fri = greet(&at(2026, 12, 4, 20, 0));
        assert_eq!((fri.lead.as_str(), fri.doodle), ("Nearly weekend", Doodle::Night));
        assert_ne!(greet(&at(2026, 12, 4, 15, 59)).lead, "Nearly weekend");
    }

    #[test]
    fn name_is_optional_and_never_replaced() {
        let mut i = at(2026, 12, 2, 13, 0);
        i.name = None;
        assert!(!greet(&i).line().contains(','));
        i.name = Some("  ".into());
        assert_eq!(greet(&i).name, None);
        i.name = Some("Alexandria".into());
        assert!(greet(&i).line().ends_with(", Alexandria"));
    }

    fn with_days(mut i: Inputs, days: &[(Date, u64)]) -> Inputs {
        i.usage.days = days.to_vec();
        i
    }

    fn streak(end: Date, n: i64, words: u64) -> Vec<(Date, u64)> {
        (0..n).map(|k| (end.plus(-k), words)).collect()
    }

    #[test]
    fn priority_birthday_then_new_year_then_milestone_then_time() {
        let jan1 = Date::new(2027, 1, 1);
        // A 7 day streak reached on New Year's Day, and the birthday is also New Year's Day.
        let mut i = with_days(at(2027, 1, 1, 9, 0), &streak(jan1, 7, 10));
        i.birthday = Some(MonthDay { month: 1, day: 1 });
        assert_eq!(greet(&i).doodle, Doodle::Birthday);
        assert_eq!(greet(&i).line(), "Happy birthday, Sam");
        assert_eq!(greet(&i).milestone, None, "the milestone waits");
        i.birthday = None;
        assert_eq!(greet(&i).lead, "Happy New Year");
        // 2 January: the missed streak milestone shows.
        i.today = jan1.plus(1);
        let g = greet(&i);
        assert_eq!((g.lead.as_str(), g.doodle, g.tally), ("7 days running", Doodle::Streak, 7));
        // A milestone beats the weekday and the time of day.
        let mut s = with_days(at(2026, 11, 28, 9, 0), &streak(Date::new(2026, 11, 28), 7, 10));
        assert_eq!(greet(&s).lead, "7 days running");
        s.usage.days.clear();
        assert_eq!(greet(&s).lead, "Saturday morning");
    }

    #[test]
    fn missed_milestone_shows_within_three_days_then_never() {
        let reached = Date::new(2026, 12, 2);
        let base = with_days(at(2026, 12, 2, 9, 0), &streak(reached, 7, 10));
        for (offset, shows) in [(0, true), (1, true), (2, true), (3, true), (4, false)] {
            let mut i = base.clone();
            i.today = reached.plus(offset);
            assert_eq!(greet(&i).doodle == Doodle::Streak, shows, "day +{offset}");
        }
        // Shown on its day: still shows the rest of that day, not the next.
        let mut i = base.clone();
        let g = greet(&i);
        let mut l = Ledger::default();
        l.mark_shown(&g);
        i.usage.shown = l.shown.clone();
        assert_eq!(greet(&i).doodle, Doodle::Streak);
        i.today = reached.plus(1);
        assert_ne!(greet(&i).doodle, Doodle::Streak);
    }

    #[test]
    fn first_dictation_and_word_milestones() {
        let d = Date::new(2026, 11, 20);
        let g = greet(&with_days(at(2026, 11, 20, 19, 15), &[(d, 40)]));
        assert_eq!((g.lead.as_str(), g.doodle), ("First words", Doodle::First));
        let days = [(d.plus(-2), 400), (d.plus(-1), 500), (d, 300)];
        let mut i = with_days(at(2026, 11, 20, 16, 37), &days);
        let g = greet(&i);
        assert_eq!((g.lead.as_str(), g.doodle), ("1,000 words spoken", Doodle::Words));
        i.usage.days = vec![(d.plus(-30), 9_990), (d, 20)];
        assert_eq!(greet(&i).lead, "10,000 words spoken");
        // Not on or before the day the ledger started (it can't vouch for those).
        i.usage.since = Some(d);
        assert_ne!(greet(&i).doodle, Doodle::Words);
    }

    #[test]
    fn streak_milestones_and_the_tally() {
        assert!([7, 14, 21, 30, 60, 90, 360].iter().all(|&n| is_streak_milestone(n)));
        assert!([1, 6, 8, 28, 35, 45].iter().all(|&n| !is_streak_milestone(n)));
        let end = Date::new(2026, 12, 24);
        let g = greet(&with_days(at(2026, 12, 24, 7, 50), &streak(end, 30, 10)));
        assert_eq!((g.lead.as_str(), g.tally), ("30 days running", 30));
        let g = greet(&with_days(at(2026, 12, 24, 7, 50), &streak(end, 90, 1)));
        assert_eq!((g.lead.as_str(), g.tally), ("90 days running", 12), "above 60 days the tally counts weeks");
        // A gap restarts the count.
        let mut d = streak(end, 6, 10);
        d.push((end.plus(-7), 10));
        assert_ne!(greet(&with_days(at(2026, 12, 24, 9, 0), &d)).doodle, Doodle::Streak);
    }

    #[test]
    fn one_year_with_voxa() {
        let mut i = at(2027, 3, 5, 10, 0);
        i.installed = Some(Date::new(2026, 3, 5));
        assert_eq!(greet(&i).line(), "One year with Voxa, Sam");
        assert_eq!(greet(&i).tally, 52);
        i.today = Date::new(2027, 3, 8);
        assert_eq!(greet(&i).lead, "One year with Voxa", "missed, within 3 days");
        i.today = Date::new(2027, 3, 9);
        assert_ne!(greet(&i).lead, "One year with Voxa");
        i.today = Date::new(2026, 3, 5);
        assert_ne!(greet(&i).lead, "One year with Voxa", "not on the install day itself");
    }

    #[test]
    fn welcome_back_and_first_open() {
        let mut i = with_days(at(2026, 12, 2, 13, 0), &[(Date::new(2026, 11, 25), 50)]);
        assert_eq!(greet(&i).lead, "Welcome back");
        assert_eq!(greet(&i).doodle, Doodle::Midday, "the time of day doodle");
        i.usage.days = vec![(Date::new(2026, 11, 26), 50)];
        assert_ne!(greet(&i).lead, "Welcome back", "6 days is not a week");
        i.first_open = true;
        assert_eq!(greet(&i).lead, "Hello there");
    }

    #[test]
    fn birthdays_are_day_and_month_only() {
        let leap = MonthDay { month: 2, day: 29 };
        assert!(leap.valid());
        assert!(leap.is(Date::new(2028, 2, 29)));
        assert!(leap.is(Date::new(2027, 2, 28)));
        assert!(!leap.is(Date::new(2028, 2, 28)));
        assert!(!MonthDay { month: 4, day: 31 }.valid());
        let mut i = at(2027, 3, 14, 8, 40);
        i.birthday = Some(MonthDay { month: 3, day: 14 });
        assert_eq!(greet(&i).line(), "Happy birthday, Sam");
    }

    #[test]
    fn moon_phase_matches_known_dates() {
        // Full and new moons (UTC): the age should be near half a month or near zero.
        let full = [Date::new(2024, 1, 25), Date::new(2025, 3, 14), Date::new(2026, 11, 24)];
        for d in full {
            let lit = moon_lit(moon_age(d));
            assert!(lit > 0.97, "{d}: {lit}");
        }
        let new = [Date::new(2024, 4, 8), Date::new(2025, 1, 29), Date::new(2026, 12, 9)];
        for d in new {
            let lit = moon_lit(moon_age(d));
            assert!(lit < 0.03, "{d}: {lit}");
        }
        // First quarter on 2025-04-05: about half lit and waxing.
        let a = moon_age(Date::new(2025, 4, 5));
        assert!((a / SYNODIC - 0.25).abs() < 0.05, "{a}");
    }

    #[test]
    fn the_date_seed_is_deterministic_and_matches_the_mockup() {
        // FNV-1a 32 of "a" is 0xE40C292C.
        assert_eq!(seed_of("a"), 0xE40C292C);
        assert_eq!(seed_of("2026-12-01sunrise"), seed_of("2026-12-01sunrise"));
        assert_ne!(seed_of("2026-12-01sunrise"), seed_of("2026-12-02sunrise"));
        let g1 = wed(13, 0);
        let g2 = wed(14, 0);
        assert_eq!(g1.seed, g2.seed, "stable all day");
        assert_eq!(g1.seed, seed_of("2026-12-02midday"));
        // mulberry32 from seed 1, first value (computed with the mockup's JavaScript).
        let mut r = Rng::new(1);
        assert!((r.next() - 0.6270739405881613).abs() < 1e-12);
    }

    #[test]
    fn ledger_round_trips_and_remembers_pruned_days() {
        let today = Date::new(2026, 12, 2);
        let mut l = Ledger::default();
        l.merge(today, &[(today.plus(-1), 120), (today, 30)]);
        assert_eq!(l.since, Some(today));
        // History pruned yesterday; today grew.
        l.merge(today, &[(today, 45)]);
        assert_eq!(l.days.iter().find(|d| d.0 == today.plus(-1)).map(|d| d.1), Some(120));
        assert_eq!(l.days.iter().find(|d| d.0 == today).map(|d| d.1), Some(45));
        assert!(l.should_play(Doodle::Midday, today));
        assert!(!l.should_play(Doodle::Midday, today), "once per doodle per day");
        assert!(l.should_play(Doodle::Evening, today));
        assert!(l.should_play(Doodle::Evening, today.plus(1)));
        let back = Ledger::parse(&l.to_text());
        assert_eq!(back.since, l.since);
        assert_eq!(back.played, l.played);
        assert_eq!(back.days.len(), 2);
        // A brand new user: since is yesterday, so today's first dictation counts.
        let mut fresh = Ledger::default();
        fresh.merge(today, &[]);
        assert_eq!(fresh.since, Some(today.plus(-1)));
    }
}
