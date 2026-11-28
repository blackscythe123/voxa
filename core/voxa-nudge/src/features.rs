//! What the model sees: a handful of normalised numbers built from the clock, the dictation
//! history and the reminder history. The simulator and the phone build them with the same code.

use crate::state::{NudgeState, Outcome};

pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 24 * HOUR_MS;

/// Inputs to the network, the last one a constant 1 (the bias).
pub const N_IN: usize = 15;

pub const INPUT_NAMES: [&str; N_IN] = [
    "hour_sin",
    "hour_cos",
    "weekday_sin",
    "weekday_cos",
    "days_since_dictation",
    "usual_hour_sin",
    "usual_hour_cos",
    "near_usual_hour",
    "active_day_fraction",
    "nudges_last_14_days",
    "last_nudge_opened",
    "last_nudge_ignored",
    "last_nudge_dismissed",
    "hours_since_last_nudge",
    "bias",
];

/// How far back "usual hour" and "fraction of days with a dictation" look.
pub const LOOKBACK_DAYS: u64 = 28;

/// Local wall-clock time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    /// Days since 1970-01-01 in local time.
    pub day: i64,
    /// 0.0 up to 24.0.
    pub hour: f32,
    /// Monday = 0 ... Sunday = 6.
    pub weekday: u32,
}

pub fn local(now_ms: u64, utc_offset_minutes: i32) -> Local {
    let ms = now_ms as i64 + utc_offset_minutes as i64 * 60_000;
    let day = ms.div_euclid(DAY_MS as i64);
    let in_day = ms.rem_euclid(DAY_MS as i64);
    // 1970-01-01 was a Thursday (Monday = 0 makes it 3).
    Local { day, hour: in_day as f32 / HOUR_MS as f32, weekday: (day + 3).rem_euclid(7) as u32 }
}

/// The situation in plain units, before normalising.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub hour: f32,
    pub weekday: u32,
    /// Days since the last dictation; `LOOKBACK_DAYS` when there is none.
    pub days_since_dictation: f32,
    /// Circular mean hour of recent dictations, when there are any.
    pub usual_hour: Option<f32>,
    /// Share of the last `LOOKBACK_DAYS` days with at least one dictation.
    pub active_fraction: f32,
    pub nudges_14d: u32,
    pub last_outcome: Option<Outcome>,
    pub hours_since_nudge: Option<f32>,
}

fn circ(hour: f32, period: f32) -> (f32, f32) {
    let a = std::f32::consts::TAU * hour / period;
    (a.sin(), a.cos())
}

/// The network's input vector. Every entry is in [-1, 1].
///
/// `near_usual_hour` is cos(now - usual hour): 1 at the user's usual hour, -1 twelve hours away,
/// 0 with no history. A tanh network this small cannot multiply the two clock encodings itself.
pub fn encode(c: &Context) -> [f32; N_IN] {
    let (hs, hc) = circ(c.hour, 24.0);
    let (ws, wc) = circ(c.weekday as f32, 7.0);
    let (us, uc) = c.usual_hour.map(|h| circ(h, 24.0)).unwrap_or((0.0, 0.0));
    let near = c.usual_hour.map(|u| circ(c.hour - u, 24.0).1).unwrap_or(0.0);
    let one = |o: Outcome| if c.last_outcome == Some(o) { 1.0 } else { 0.0 };
    [
        hs,
        hc,
        ws,
        wc,
        c.days_since_dictation.clamp(0.0, LOOKBACK_DAYS as f32) / LOOKBACK_DAYS as f32,
        us,
        uc,
        near,
        c.active_fraction.clamp(0.0, 1.0),
        c.nudges_14d.min(7) as f32 / 7.0,
        one(Outcome::Opened),
        one(Outcome::Ignored),
        one(Outcome::Dismissed),
        c.hours_since_nudge.map(|h| h.clamp(0.0, 336.0) / 336.0).unwrap_or(1.0),
        1.0,
    ]
}

/// Builds the context from the dictation start times (ms since the epoch, any order) and the
/// reminder state.
pub fn context(now_ms: u64, utc_offset_minutes: i32, dictations_ms: &[u64], state: &NudgeState) -> Context {
    let now = local(now_ms, utc_offset_minutes);
    let since = now_ms.saturating_sub(LOOKBACK_DAYS * DAY_MS);
    let mut last: Option<u64> = None;
    let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0u32);
    let mut days: Vec<i64> = Vec::new();
    for &t in dictations_ms.iter().filter(|&&t| t <= now_ms) {
        last = Some(last.map_or(t, |l| l.max(t)));
        if t >= since {
            let l = local(t, utc_offset_minutes);
            let (s, c) = circ(l.hour, 24.0);
            sx += s;
            sy += c;
            n += 1;
            if !days.contains(&l.day) {
                days.push(l.day);
            }
        }
    }
    let usual_hour = (n > 0 && (sx * sx + sy * sy).sqrt() > 1e-3 * n as f32)
        .then(|| (sx.atan2(sy) / std::f32::consts::TAU * 24.0).rem_euclid(24.0));
    let since_14 = now_ms.saturating_sub(14 * DAY_MS);
    Context {
        hour: now.hour,
        weekday: now.weekday,
        days_since_dictation: last.map_or(LOOKBACK_DAYS as f32, |l| (now_ms - l) as f32 / DAY_MS as f32),
        usual_hour,
        active_fraction: days.len() as f32 / LOOKBACK_DAYS as f32,
        nudges_14d: state.sent_ms.iter().filter(|&&t| t >= since_14 && t <= now_ms).count() as u32,
        last_outcome: state.last_outcome,
        hours_since_nudge: state.sent_ms.iter().max().map(|&t| now_ms.saturating_sub(t) as f32 / HOUR_MS as f32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_and_weekday() {
        // 2025-12-01 is a Monday; 10:30 UTC is 16:00 at +05:30.
        let monday_utc = 20_423 * DAY_MS;
        let l = local(monday_utc + 10 * HOUR_MS + 30 * 60_000, 330);
        assert_eq!(l.weekday, 0);
        assert!((l.hour - 16.0).abs() < 1e-4);
        // A negative offset can move the clock back to Sunday.
        assert_eq!(local(monday_utc + HOUR_MS, -120).weekday, 6);
    }

    #[test]
    fn context_from_history() {
        let day0 = 20_423 * DAY_MS;
        let now = day0 + 10 * DAY_MS + 15 * HOUR_MS;
        // Dictations at 23:00 and 01:00 on different days: the usual hour wraps around midnight.
        let d = [day0 + 5 * DAY_MS + 23 * HOUR_MS, day0 + 7 * DAY_MS + HOUR_MS];
        let c = context(now, 0, &d, &NudgeState::default());
        let u = c.usual_hour.unwrap();
        assert!(u < 0.1 || u > 23.9, "usual hour {u}");
        assert!((c.days_since_dictation - (3.0 + 14.0 / 24.0)).abs() < 1e-3);
        assert!((c.active_fraction - 2.0 / 28.0).abs() < 1e-6);
        assert_eq!(c.hours_since_nudge, None);
        let x = encode(&c);
        assert!(x.iter().all(|v| (-1.0..=1.0).contains(v)));
        assert_eq!(x[N_IN - 1], 1.0);
    }

    #[test]
    fn no_history_is_well_defined() {
        let c = context(1_000 * DAY_MS, 0, &[], &NudgeState::default());
        assert_eq!(c.usual_hour, None);
        assert_eq!(c.days_since_dictation, LOOKBACK_DAYS as f32);
        assert_eq!(encode(&c)[4], 1.0);
        assert_eq!(encode(&c)[7], 0.0);
    }
}
