//! A simulator of Voxa users, for training and comparing reminder policies.
//!
//! This is a model of users, not data. Every number below is an assumption chosen to be
//! plausible (people have a usual time of day, habits fade without use, a well timed reminder can
//! bring someone back, too many or badly timed reminders annoy and eventually make people turn
//! notifications off). The README lists the assumptions.
//!
//! Each user lives 14 warm-up days (no reminders, to build a history) and then 28 days in which
//! the policy may remind them. The policy is asked every 2 hours from 07:00 to 23:00, through the
//! same feature code, state and (optionally) the same hard rules as the phone.
//!
//! Randomness that belongs to the user (whether they dictate today, at what hour, whether they open
//! a reminder) comes from a hash of (user, day, check), so two policies see the same users living
//! the same days ("common random numbers"); differences come from the policies, not luck.

use crate::bandit::slot_of;
use crate::engine;
use crate::features::{Context, DAY_MS, HOUR_MS, context};
use crate::messages::{K_STYLES, Style};
use crate::net::Net;
use crate::rng::{Rng, hash_unit, mix};
use crate::rules;
use crate::state::{NudgeState, Outcome};

/// Simulated day 0 is Monday 2025-12-01, at UTC.
pub const DAY0: u64 = 20_423;
pub const WARMUP_DAYS: u64 = 14;
pub const EVAL_DAYS: u64 = 28;
/// When the policy is asked (local hours). 07:00, 21:00 and 23:00 are always blocked by the rules;
/// they are there so the policies without rules can be measured.
pub const CHECK_HOURS: [f32; 9] = [7.0, 9.0, 11.0, 13.0, 15.0, 17.0, 19.0, 21.0, 23.0];

// Response model.
const BASE_OPEN: f64 = 0.7;
const TIMING_WIDTH_HOURS: f64 = 2.5;
const OPEN_BUMP: f64 = 0.35;
const HABIT_BUMP: f64 = 0.08;
const ANNOY_DECAY: f64 = 0.93;
const OPT_OUT_AT: f64 = 1.2;

// Score weights.
pub const W_DISMISS: f64 = 0.3;
pub const W_NIGHT: f64 = 1.0;
pub const W_WHILE_ACTIVE: f64 = 0.5;
pub const W_OPT_OUT: f64 = 3.0;
pub const W_SEND: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Morning,
    Midday,
    Evening,
    WeekdayWorker,
    WeekendUser,
    NightOwl,
}

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: u64,
    pub kind: Kind,
    /// When they like to dictate (local hour).
    pub pref_hour: f64,
    /// When they are most receptive to a reminder. The same as `pref_hour` in the training world.
    pub response_hour: f64,
    /// Monday first: how likely a day of the week is a dictation day.
    pub day_weight: [f64; 7],
    /// Daily chance of dictating at the start, and the most a habit can reach.
    pub e0: f64,
    pub emax: f64,
    /// Engagement kept after a day without dictating.
    pub decay: f64,
    pub lapsing: bool,
    pub hates_notifications: bool,
    /// How receptive to reminders (0..1) and how much each one annoys.
    pub tolerance: f64,
    pub annoy_gain: f64,
    /// How much each style appeals, by [`Style::ALL`].
    pub affinity: [f64; K_STYLES],
}

pub fn make_user(id: u64) -> User {
    let mut r = Rng::new(id);
    let roll = r.unit();
    let (kind, pref, sd) = match roll {
        x if x < 0.22 => (Kind::Morning, 9.5, 1.0),
        x if x < 0.38 => (Kind::Midday, 13.5, 1.5),
        x if x < 0.60 => (Kind::Evening, 19.5, 1.0),
        x if x < 0.75 => (Kind::WeekdayWorker, 11.0, 2.0),
        x if x < 0.85 => (Kind::WeekendUser, 15.0, 2.0),
        _ => (Kind::NightOwl, 22.0, 1.0),
    };
    let pref_hour = (pref + sd * r.normal()).clamp(6.0, 23.5);
    let mut day_weight = [0.0; 7];
    for (d, w) in day_weight.iter_mut().enumerate() {
        let weekend = d >= 5;
        *w = match kind {
            Kind::WeekdayWorker if weekend => 0.15,
            Kind::WeekendUser if !weekend => 0.25,
            _ => r.range(0.8, 1.0),
        };
    }
    let lapsing = r.chance(0.35);
    let decay = if lapsing { r.range(0.88, 0.93) } else { r.range(0.96, 0.99) };
    let hates = r.chance(0.15);
    let (tolerance, annoy_gain) = if hates { (0.25, 0.35) } else { (r.range(0.6, 1.0), r.range(0.05, 0.12)) };
    let e0 = r.range(0.25, 0.85);
    let mut affinity = [0.0; K_STYLES];
    for a in &mut affinity {
        *a = r.range(0.4, 1.0);
    }
    // Assumed links between who someone is and what wording works for them: heavier users like
    // hearing their progress, fading users respond to a tip, people who dislike notifications
    // respond best to being told we will leave them alone.
    affinity[Style::Progress.index()] += 0.6 * (e0 - 0.25);
    if lapsing {
        affinity[Style::Tip.index()] += 0.5;
    }
    affinity[Style::Quiet.index()] = if hates { 1.2 } else { 0.3 };
    let fav = r.below(K_STYLES);
    affinity[fav] *= 1.5;
    for a in &mut affinity {
        *a = a.clamp(0.0, 1.5);
    }
    User {
        id,
        kind,
        pref_hour,
        response_hour: pref_hour,
        day_weight,
        e0,
        emax: (e0 + 0.2).min(0.95),
        decay,
        lapsing,
        hates_notifications: hates,
        tolerance,
        annoy_gain,
        affinity,
    }
}

/// `n` users from a pool seed. Different seeds give disjoint users (training, validation, test).
pub fn population(n: usize, seed: u64) -> Vec<User> {
    (0..n as u64).map(|i| make_user(mix(seed.wrapping_mul(1_000_003).wrapping_add(i)))).collect()
}

/// A "reality differs from the simulator" world for testing on-device personalisation: users read
/// reminders five hours away from when they dictate (so dictation history misleads about timing),
/// tips work much worse and the plain wording much better than the training world assumed.
pub fn population_shifted(n: usize, seed: u64) -> Vec<User> {
    population(n, seed)
        .into_iter()
        .map(|mut u| {
            u.response_hour = if u.pref_hour < 15.0 { u.pref_hour + 5.0 } else { u.pref_hour - 5.0 };
            u.affinity[Style::Tip.index()] *= 0.35;
            u.affinity[Style::Plain.index()] = (u.affinity[Style::Plain.index()] * 1.6).min(1.5);
            u
        })
        .collect()
}

/// A reminder policy. Sees what the phone would see.
pub trait Policy {
    fn decide(&mut self, ctx: &Context, state: &NudgeState, rng: &mut Rng) -> Option<Style>;
}

pub struct Never;
impl Policy for Never {
    fn decide(&mut self, _: &Context, _: &NudgeState, _: &mut Rng) -> Option<Style> {
        None
    }
}

/// The classic: a plain reminder at a fixed hour, at most every 3 days.
pub struct FixedTime {
    pub hour: f32,
}
impl Policy for FixedTime {
    fn decide(&mut self, ctx: &Context, _: &NudgeState, _: &mut Rng) -> Option<Style> {
        ((ctx.hour - self.hour).abs() < 0.01 && ctx.hours_since_nudge.is_none_or(|h| h >= 72.0)).then_some(Style::Plain)
    }
}

/// A hand-written heuristic: a reminder within an hour and a half of the user's usual dictation
/// hour (19:00 when there is no history), at most every 3 days, always in one wording.
pub struct UsualHour {
    pub style: Style,
}
impl Policy for UsualHour {
    fn decide(&mut self, ctx: &Context, _: &NudgeState, _: &mut Rng) -> Option<Style> {
        let target = ctx.usual_hour.unwrap_or(19.0) as f64;
        (hour_gap(ctx.hour as f64, target) <= 1.5 && ctx.hours_since_nudge.is_none_or(|h| h >= 72.0)).then_some(self.style)
    }
}

/// A random style with chance `p` at every check.
pub struct Random {
    pub p: f64,
}
impl Policy for Random {
    fn decide(&mut self, _: &Context, _: &NudgeState, rng: &mut Rng) -> Option<Style> {
        rng.chance(self.p).then(|| Style::from_index(rng.below(K_STYLES)))
    }
}

/// A plain reminder at the first moment the rules allow.
pub struct FirstChance;
impl Policy for FirstChance {
    fn decide(&mut self, _: &Context, _: &NudgeState, _: &mut Rng) -> Option<Style> {
        Some(Style::Plain)
    }
}

/// The evolved network, alone or with the on-device bandit.
pub struct NetPolicy<'a> {
    pub net: &'a Net,
    pub personalise: bool,
}
impl Policy for NetPolicy<'_> {
    fn decide(&mut self, ctx: &Context, state: &NudgeState, rng: &mut Rng) -> Option<Style> {
        engine::choose(self.net, ctx, state, &[true; K_STYLES], self.personalise, rng).map(|(s, _)| s)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SimConfig {
    /// Whether the hard rules apply (they always do on the phone).
    pub rules: bool,
    /// Days in which reminders may go out (after the 14 warm-up days).
    pub eval_days: u64,
}

impl SimConfig {
    pub const RULES: SimConfig = SimConfig { rules: true, eval_days: EVAL_DAYS };
    pub const NO_RULES: SimConfig = SimConfig { rules: false, eval_days: EVAL_DAYS };
}

/// Totals over users.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tally {
    pub users: f64,
    /// Days with at least one dictation during the reminder days.
    pub active_days: f64,
    pub sends: f64,
    pub opens: f64,
    pub dismissals: f64,
    pub ignores: f64,
    pub night_sends: f64,
    /// Sent within 48 hours of a dictation.
    pub sends_while_active: f64,
    pub opt_outs: f64,
    /// Users with no dictation in the last 7 days.
    pub lapsed: f64,
}

impl Tally {
    pub fn add(&mut self, o: &Tally) {
        self.users += o.users;
        self.active_days += o.active_days;
        self.sends += o.sends;
        self.opens += o.opens;
        self.dismissals += o.dismissals;
        self.ignores += o.ignores;
        self.night_sends += o.night_sends;
        self.sends_while_active += o.sends_while_active;
        self.opt_outs += o.opt_outs;
        self.lapsed += o.lapsed;
    }

    /// Fitness per user: retained active days minus the annoyance penalties.
    pub fn score(&self) -> f64 {
        (self.active_days
            - W_DISMISS * self.dismissals
            - W_NIGHT * self.night_sends
            - W_WHILE_ACTIVE * self.sends_while_active
            - W_OPT_OUT * self.opt_outs
            - W_SEND * self.sends)
            / self.users.max(1.0)
    }

    pub fn per_user(&self, v: f64) -> f64 {
        v / self.users.max(1.0)
    }

    pub fn open_rate(&self) -> f64 {
        if self.sends > 0.0 { self.opens / self.sends } else { 0.0 }
    }
}

fn at(day: u64, hour: f64) -> u64 {
    (DAY0 + day) * DAY_MS + (hour * HOUR_MS as f64) as u64
}

/// A standard normal from two hashed uniforms (Box-Muller).
fn hashed_normal(id: u64, day: u64, k: u64) -> f64 {
    let u1 = hash_unit(&[id, day, k]).max(1e-12);
    let u2 = hash_unit(&[id, day, k + 1]);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn hour_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(24.0);
    d.min(24.0 - d)
}

/// One user's warm-up and reminder days under a policy.
pub fn simulate_user(u: &User, policy: &mut dyn Policy, cfg: SimConfig) -> Tally {
    let mut t = Tally { users: 1.0, ..Default::default() };
    let mut st = NudgeState::default();
    let mut dict: Vec<u64> = Vec::new();
    let mut last_dict: Option<u64> = None;
    let mut rng = Rng::new(u.id ^ 0x0B0B_5EED);
    let (mut e, mut annoy, mut opted_out) = (u.e0, 0.0f64, false);
    for d in 0..WARMUP_DAYS + cfg.eval_days {
        let eval = d >= WARMUP_DAYS;
        let dw = u.day_weight[(d % 7) as usize];
        let organic = hash_unit(&[u.id, d, 1]) < e * dw;
        let org_hour = (u.pref_hour + 1.5 * hashed_normal(u.id, d, 2)).clamp(6.0, 23.9);
        let mut org_done = false;
        let mut active_today = false;
        let push = |time: u64, dict: &mut Vec<u64>, last: &mut Option<u64>| {
            dict.push(time);
            *last = Some(last.map_or(time, |l| l.max(time)));
        };
        for (ci, &h) in CHECK_HOURS.iter().enumerate() {
            if organic && !org_done && org_hour < h as f64 {
                push(at(d, org_hour), &mut dict, &mut last_dict);
                org_done = true;
                active_today = true;
            }
            if !eval {
                continue;
            }
            let now = at(d, h as f64);
            st.observe(now, last_dict);
            let allowed = if cfg.rules { rules::check(now, h, last_dict, &st).is_ok() } else { st.enabled };
            if !allowed {
                continue;
            }
            // Only the last 28 days matter to the features; keep the scan short.
            dict.retain(|&x| x + 29 * DAY_MS >= now);
            let ctx = context(now, 0, &dict, &st);
            let Some(style) = policy.decide(&ctx, &st, &mut rng) else { continue };

            t.sends += 1.0;
            if rules::is_night(h) {
                t.night_sends += 1.0;
            }
            let recent_48h = last_dict.is_some_and(|l| now < l + rules::ACTIVE_WINDOW_MS);
            if recent_48h {
                t.sends_while_active += 1.0;
            }
            st.record_send(now, style, slot_of(h));

            let timing = (-hour_gap(h as f64, u.response_hour).powi(2) / (2.0 * TIMING_WIDTH_HOURS.powi(2))).exp();
            let recent_24h = last_dict.is_some_and(|l| now < l + DAY_MS);
            let mut p_open = BASE_OPEN
                * u.tolerance
                * u.affinity[style.index()]
                * timing
                * (0.4 + 0.6 * dw)
                * (0.35 + 0.65 * (e / u.emax).min(1.0))
                * (1.0 - 0.6 * annoy.min(1.0))
                * if recent_24h { 0.5 } else { 1.0 };
            if style == Style::Quiet {
                p_open *= 0.7;
            }
            let p_dismiss = 0.1 + 0.5 * (1.0 - u.tolerance) + 0.3 * annoy.min(1.0);
            let outcome = if opted_out {
                Outcome::Ignored
            } else if hash_unit(&[u.id, d, 10 + ci as u64]) < p_open.min(0.95) {
                Outcome::Opened
            } else if hash_unit(&[u.id, d, 30 + ci as u64]) < p_dismiss {
                Outcome::Dismissed
            } else {
                Outcome::Ignored
            };
            if !opted_out {
                let mut inc = u.annoy_gain * (1.0 + 2.0 * (1.0 - timing));
                if rules::is_night(h) {
                    inc += 0.35;
                }
                if recent_48h {
                    inc += 0.2;
                }
                if outcome == Outcome::Opened {
                    inc *= 0.3;
                }
                if style == Style::Quiet {
                    inc *= 0.4;
                }
                annoy += inc;
            }
            match outcome {
                Outcome::Opened => {
                    t.opens += 1.0;
                    push(now + HOUR_MS / 10, &mut dict, &mut last_dict);
                    active_today = true;
                    e += OPEN_BUMP * (u.emax - e).max(0.0);
                }
                Outcome::Dismissed => t.dismissals += 1.0,
                Outcome::Ignored => t.ignores += 1.0,
            }
            st.record_outcome(outcome);
            if annoy > OPT_OUT_AT && !opted_out {
                opted_out = true;
                t.opt_outs += 1.0;
                e *= 0.8;
            }
        }
        if organic && !org_done {
            push(at(d, org_hour), &mut dict, &mut last_dict);
            active_today = true;
        }
        if active_today {
            e += HABIT_BUMP * (u.emax - e).max(0.0);
            if eval {
                t.active_days += 1.0;
            }
        } else {
            e *= u.decay;
        }
        // Lingering annoyance wears the habit down.
        e = (e * (1.0 - 0.03 * (annoy - 0.3).clamp(0.0, 2.0))).clamp(0.0, 1.0);
        annoy *= ANNOY_DECAY;
    }
    let end = at(WARMUP_DAYS + cfg.eval_days, 0.0);
    if last_dict.is_none_or(|l| l + 7 * DAY_MS < end) {
        t.lapsed = 1.0;
    }
    t
}

/// Runs every user under a fresh policy and adds up the results.
pub fn evaluate<'a>(users: &[User], make: &dyn Fn() -> Box<dyn Policy + 'a>, cfg: SimConfig) -> Tally {
    let mut total = Tally::default();
    for u in users {
        let mut p = make();
        total.add(&simulate_user(u, p.as_mut(), cfg));
    }
    total
}

/// The training objective: the network (no bandit) behind the hard rules.
pub fn net_score(net: &Net, users: &[User]) -> f64 {
    evaluate(users, &|| Box::new(NetPolicy { net, personalise: false }), SimConfig::RULES).score()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn users_are_varied_and_repeatable() {
        let a = population(2000, 1);
        assert_eq!(a, population(2000, 1));
        assert_ne!(a[0], population(1, 2)[0]);
        let share = |k: Kind| a.iter().filter(|u| u.kind == k).count() as f64 / a.len() as f64;
        for k in [Kind::Morning, Kind::Midday, Kind::Evening, Kind::WeekdayWorker, Kind::WeekendUser, Kind::NightOwl] {
            assert!(share(k) > 0.05, "{k:?} {}", share(k));
        }
        let haters = a.iter().filter(|u| u.hates_notifications).count();
        assert!((200..400).contains(&haters), "{haters}");
    }

    #[test]
    fn the_rules_hold_in_the_simulator() {
        let users = population(300, 3);
        let t = evaluate(&users, &|| Box::new(FirstChance), SimConfig::RULES);
        assert!(t.sends > 0.0);
        assert_eq!(t.night_sends, 0.0);
        assert_eq!(t.sends_while_active, 0.0);
        // At most one every 3 days over 28 days: 10 per user.
        assert!(t.sends <= 10.0 * t.users);
        let wild = evaluate(&users, &|| Box::new(Random { p: 0.1 }), SimConfig::NO_RULES);
        assert!(wild.night_sends > 0.0, "without rules random sends at night");
    }

    #[test]
    fn reminders_can_help_and_spam_hurts() {
        let users = population(1500, 4);
        let never = evaluate(&users, &|| Box::new(Never), SimConfig::RULES);
        let first = evaluate(&users, &|| Box::new(FirstChance), SimConfig::RULES);
        let spam = evaluate(&users, &|| Box::new(Random { p: 0.5 }), SimConfig::NO_RULES);
        assert_eq!(never.sends, 0.0);
        let fixed = evaluate(&users, &|| Box::new(FixedTime { hour: 19.0 }), SimConfig::RULES);
        let usual = evaluate(&users, &|| Box::new(UsualHour { style: Style::Plain }), SimConfig::RULES);
        assert!(usual.active_days > never.active_days, "a well timed reminder brings some users back");
        assert!(usual.score() > fixed.score(), "timing matters");
        assert!(spam.score() < never.score(), "spam scores below doing nothing");
        assert!(spam.opt_outs > first.opt_outs);
    }

    #[test]
    fn same_policy_same_result() {
        let users = population(100, 5);
        let a = evaluate(&users, &|| Box::new(Random { p: 0.2 }), SimConfig::RULES);
        let b = evaluate(&users, &|| Box::new(Random { p: 0.2 }), SimConfig::RULES);
        assert_eq!(a, b);
    }
}
