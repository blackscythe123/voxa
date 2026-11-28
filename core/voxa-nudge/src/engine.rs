//! The engine as the phone uses it: hard rules first, then the network's prior, then the
//! on-device bandit; outcomes feed back into the bandit. The state is a small JSON file.

use std::path::{Path, PathBuf};

use crate::bandit::{send_probability, slot_of};
use crate::features::{Context, DAY_MS, context, encode, local};
use crate::messages::{K_STYLES, Style, available, message};
use crate::net::Net;
use crate::rng::Rng;
use crate::rules::{self, Block};
use crate::state::{NudgeState, Outcome};

/// How the engine decides when to remind, once the hard rules allow it.
///
/// [`Policy::UsualHour`] is the default: in its own simulator the evolved network did not beat
/// this simple rule (see the README), so the network stays an experiment behind this switch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Remind within [`USUAL_HOUR_WINDOW`] hours of the hour the user usually dictates
    /// ([`DEFAULT_HOUR`] with no history), in the tip wording.
    #[default]
    UsualHour,
    /// Experimental: the evolved network plus the on-device bandit.
    Network,
}

/// How close to the usual hour (either side) a reminder may go out, in hours.
pub const USUAL_HOUR_WINDOW: f32 = 1.5;
/// The hour used when the user has no dictation history yet.
pub const DEFAULT_HOUR: f32 = 19.0;

/// Hours between two clock times, the short way round (0..=12).
pub fn hour_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(24.0);
    d.min(24.0 - d)
}

/// The hour the default policy aims at: the usual hour ([`DEFAULT_HOUR`] with no history), kept
/// inside 09:30 to 20:30 so a night owl or an early bird is reminded in the daytime.
pub fn target_hour(usual_hour: Option<f32>) -> f32 {
    usual_hour.unwrap_or(DEFAULT_HOUR).clamp(9.5, 20.5)
}

/// The default policy's decision: the wording to send now, or None to wait. The tip wording,
/// or plain if the tip is not allowed. Never the quiet wording.
pub fn usual_hour(ctx: &Context, allowed: &[bool; K_STYLES]) -> Option<Style> {
    if hour_gap(ctx.hour, target_hour(ctx.usual_hour)) > USUAL_HOUR_WINDOW {
        return None;
    }
    [Style::Tip, Style::Plain].into_iter().find(|s| allowed[s.index()])
}

/// The trained model shipped with the app (made by `cargo run -p voxa-nudge --release --bin train`).
pub const CHAMPION_JSON: &str = include_str!("../model/champion.json");

/// The shipped model. Falls back to an empty network (which never sends) if the file is unusable.
pub fn bundled() -> Net {
    Net::from_json(CHAMPION_JSON).unwrap_or_default()
}

/// The decision for one check: Some((style, slot)) to send. `personalise` adds the bandit.
pub fn choose(
    net: &Net,
    ctx: &Context,
    state: &NudgeState,
    allowed: &[bool; K_STYLES],
    personalise: bool,
    rng: &mut Rng,
) -> Option<(Style, Option<usize>)> {
    let out = net.forward(&encode(ctx));
    let slot = slot_of(ctx.hour);
    let send = if personalise {
        let shift = slot.map(|s| state.bandit.slot_shift(s, rng)).unwrap_or(0.0);
        send_probability(out.p_send, shift) > 0.5
    } else {
        out.wants_to_send()
    };
    if !send {
        return None;
    }
    let best = (0..K_STYLES).filter(|&s| allowed[s]).max_by(|&a, &b| out.style[a].total_cmp(&out.style[b]).then(b.cmp(&a)))?;
    // The quiet wording ends reminders until the user dictates again, so the bandit never tries it
    // at random: it goes out only when the network picks it.
    let quiet = Style::Quiet.index();
    let style = if personalise && best != quiet {
        let mut explore = *allowed;
        explore[quiet] = false;
        state.bandit.pick_style(&out.style, &explore, rng)
    } else {
        best
    };
    Some((Style::from_index(style), slot))
}

/// What the app knows about the user's dictations.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    /// Start times of dictations (ms since the epoch), any order. History entry ids.
    pub dictations_ms: Vec<u64>,
    /// Seconds saved versus typing this month (for the progress wording).
    pub saved_secs_month: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Nudge {
    pub style: Style,
    pub title: String,
    pub body: String,
}

/// Why nothing was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    Rule(Block),
    /// The rules allowed it but the model chose to wait.
    ModelWaits,
    /// A reminder was due but `nudge.json` could not be saved, so the 3 day gap could not be
    /// kept: nothing is sent.
    NotSaved,
}

pub const STATE_FILE: &str = "nudge.json";

pub struct Engine {
    /// The default is [`Policy::UsualHour`]; [`Policy::Network`] uses `net` and the bandit.
    pub policy: Policy,
    pub net: Net,
    pub state: NudgeState,
    path: Option<PathBuf>,
    /// False when `nudge.json` exists but could not be read: the good file on disk is never
    /// overwritten with the stand in state.
    read_ok: bool,
}

impl Engine {
    /// An engine with the shipped model, not saved anywhere.
    pub fn in_memory(state: NudgeState) -> Self {
        Self { policy: Policy::default(), net: bundled(), state, path: None, read_ok: true }
    }

    /// Loads (or starts) the state in `dir`/nudge.json. No file: a fresh start (reminders on).
    /// A file that can't be understood keeps reminders off unless it still says "on"
    /// ([`NudgeState::from_json`]): a broken file must never turn reminders back on. A file that
    /// can't be read at all (another app holds it) also keeps reminders off, and this engine then
    /// never saves over it.
    pub fn open(dir: &Path) -> Self {
        let path = dir.join(STATE_FILE);
        let (state, read_ok) = match std::fs::read_to_string(&path) {
            Ok(s) => (NudgeState::from_json(&s), true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (NudgeState::default(), true),
            Err(_) => (NudgeState { enabled: false, ..Default::default() }, false),
        };
        Self { policy: Policy::default(), net: bundled(), state, path: Some(path), read_ok }
    }

    /// Writes the state to `nudge.json`: a temp file with a unique name, flushed to disk, then
    /// renamed over the old one. Refuses when the file could not be read at open.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if !self.read_ok {
            return Err(std::io::Error::other("nudge.json could not be read, so it is not overwritten"));
        }
        write_atomic(path, &serde_json::to_vec(&self.state)?)
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.state.enabled = on;
        let _ = self.save();
    }

    /// Asked by the app's background job. Sends at most what the rules allow; a sent reminder is
    /// remembered and waits for its outcome. A reminder whose sending can't be saved is not sent.
    pub fn decide(&mut self, now_ms: u64, utc_offset_minutes: i32, usage: &Usage, rng: &mut Rng) -> Result<Nudge, Skip> {
        self.state.remember(&usage.dictations_ms, now_ms);
        let dictations = self.state.dictations_ms.clone();
        // Remembered times are never after now (see NudgeState::remember).
        let last = dictations.iter().copied().max();
        self.state.observe(now_ms, last);
        let here = local(now_ms, utc_offset_minutes);
        let verdict = rules::check(now_ms, here.hour, last, &self.state);
        let result = match verdict {
            Err(b) => Err(Skip::Rule(b)),
            Ok(()) => {
                let ctx = context(now_ms, utc_offset_minutes, &dictations, &self.state);
                let allowed = available(usage.saved_secs_month);
                let picked = match self.policy {
                    Policy::UsualHour => usual_hour(&ctx, &allowed).map(|s| (s, slot_of(ctx.hour))),
                    Policy::Network => choose(&self.net, &ctx, &self.state, &allowed, true, rng),
                };
                match picked {
                    None => Err(Skip::ModelWaits),
                    Some((style, slot)) => {
                        let before = self.state.clone();
                        self.state.record_send(now_ms, style, slot);
                        if self.save().is_err() {
                            // Unsaved, the next check would not know it went out and could send
                            // again the next day.
                            self.state = before;
                            return Err(Skip::NotSaved);
                        }
                        let (title, body) = message(style, usage.saved_secs_month, here.day);
                        Ok(Nudge { style, title, body })
                    }
                }
            }
        };
        if result.is_err() {
            let _ = self.save();
        }
        result
    }

    /// When the app's one-shot job should next call [`Engine::decide`] (ms since the epoch): the
    /// next time the local clock reaches the user's usual hour ([`DEFAULT_HOUR`] with no history),
    /// kept inside the 09:00 to 21:00 daytime window, at least 10 minutes from now. One wake-up a
    /// day at most, instead of a check every 2 hours.
    pub fn next_check_ms(&mut self, now_ms: u64, utc_offset_minutes: i32, usage: &Usage) -> u64 {
        self.state.remember(&usage.dictations_ms, now_ms);
        let ctx = context(now_ms, utc_offset_minutes, &self.state.dictations_ms, &self.state);
        let target = target_hour(ctx.usual_hour);
        let day = DAY_MS as i64;
        let in_day = (now_ms as i64 + utc_offset_minutes as i64 * 60_000).rem_euclid(day);
        // Whole minutes, so the time is exact.
        let at = (target as f64 * 60.0).round() as i64 * 60_000;
        let mut ahead = (at - in_day).rem_euclid(day);
        if ahead < 10 * 60_000 {
            ahead += day;
        }
        now_ms + ahead as u64
    }

    /// The user tapped (Opened) or swiped away (Dismissed) the reminder.
    pub fn record_outcome(&mut self, outcome: Outcome) {
        self.state.record_outcome(outcome);
        let _ = self.save();
    }
}

/// Writes `bytes` to `path` safely: a temp file with a name no other writer uses, flushed to
/// disk, then renamed over `path`. The temp file is removed when anything fails.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}-{n}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::HOUR_MS;
    use crate::net::{N_GENES, gene_in_out};

    /// A network that always wants to send, preferring the tip.
    fn eager() -> Net {
        let mut n = Net::default();
        let bias = crate::features::N_IN - 1;
        n.on[gene_in_out(bias, 0)] = true;
        n.w[gene_in_out(bias, 0)] = 5.0;
        n.on[gene_in_out(bias, 1 + Style::Tip.index())] = true;
        n.w[gene_in_out(bias, 1 + Style::Tip.index())] = 4.0;
        n
    }

    const NOON: u64 = 20_423 * DAY_MS + 12 * HOUR_MS;

    #[test]
    fn the_shipped_model_loads_and_is_small() {
        let n = Net::from_json(CHAMPION_JSON).expect("champion.json loads");
        assert!(n.params() <= N_GENES);
        assert!(CHAMPION_JSON.len() < 5 * 1024, "{} bytes", CHAMPION_JSON.len());
    }

    #[test]
    fn rules_win_over_an_eager_model() {
        let mut e = Engine::in_memory(NudgeState::default());
        e.policy = Policy::Network;
        e.net = eager();
        let mut rng = Rng::new(1);
        let usage = Usage { dictations_ms: vec![NOON - 10 * DAY_MS], saved_secs_month: 0 };
        // Night: blocked.
        assert_eq!(e.decide(NOON + 10 * HOUR_MS, 0, &usage, &mut rng), Err(Skip::Rule(Block::NightTime)));
        // Noon: sent.
        let n = e.decide(NOON, 0, &usage, &mut rng).unwrap();
        assert!(!n.title.is_empty() && !n.body.is_empty());
        // Two hours later: too soon, however eager the model is.
        assert_eq!(e.decide(NOON + 2 * HOUR_MS, 0, &usage, &mut rng), Err(Skip::Rule(Block::TooSoon)));
        // Off switch.
        e.set_enabled(false);
        assert_eq!(e.decide(NOON + 5 * DAY_MS, 0, &usage, &mut rng), Err(Skip::Rule(Block::Off)));
    }

    #[test]
    fn local_time_decides_night() {
        let mut e = Engine::in_memory(NudgeState::default());
        e.policy = Policy::Network;
        e.net = eager();
        let usage = Usage { dictations_ms: vec![], saved_secs_month: 0 };
        // Noon UTC is 22:00 at UTC+10.
        assert_eq!(e.decide(NOON, 600, &usage, &mut Rng::new(2)), Err(Skip::Rule(Block::NightTime)));
    }

    #[test]
    fn progress_is_not_offered_without_progress() {
        let n = eager();
        let mut only_progress = n.clone();
        let bias = crate::features::N_IN - 1;
        only_progress.w[gene_in_out(bias, 1 + Style::Tip.index())] = 0.0;
        only_progress.on[gene_in_out(bias, 1 + Style::Progress.index())] = true;
        only_progress.w[gene_in_out(bias, 1 + Style::Progress.index())] = 9.0;
        let ctx = context(NOON, 0, &[], &NudgeState::default());
        let mut rng = Rng::new(3);
        for personalise in [false, true] {
            let (s, _) =
                choose(&only_progress, &ctx, &NudgeState::default(), &available(0), personalise, &mut rng).unwrap();
            assert_ne!(s, Style::Progress);
        }
    }

    #[test]
    fn outcomes_reach_the_bandit_and_the_file() {
        let dir = std::env::temp_dir().join(format!("voxa-nudge-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let usage = Usage { dictations_ms: vec![NOON - 10 * DAY_MS], saved_secs_month: 7800 };
        {
            let mut e = Engine::open(&dir);
            e.policy = Policy::Network;
            e.net = eager();
            let n = e.decide(NOON, 0, &usage, &mut Rng::new(4)).unwrap();
            e.record_outcome(Outcome::Opened);
            assert_eq!(e.state.bandit.style[n.style.index()], [1.0, 0.0]);
        }
        let e = Engine::open(&dir);
        assert_eq!(e.state.last_outcome, Some(Outcome::Opened));
        assert_eq!(e.state.sent_ms, vec![NOON]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dictations_are_remembered_when_history_forgets_them() {
        let mut e = Engine::in_memory(NudgeState::default());
        e.policy = Policy::Network;
        e.net = eager();
        let mut rng = Rng::new(6);
        let seen = Usage { dictations_ms: vec![NOON - HOUR_MS], saved_secs_month: 0 };
        assert_eq!(e.decide(NOON, 0, &seen, &mut rng), Err(Skip::Rule(Block::RecentlyActive)));
        // History was cleared, but the engine still knows the user dictated an hour ago.
        let cleared = Usage::default();
        assert_eq!(e.decide(NOON + HOUR_MS, 0, &cleared, &mut rng), Err(Skip::Rule(Block::RecentlyActive)));
    }

    #[test]
    fn two_ignored_then_silence_until_the_user_dictates() {
        let mut e = Engine::in_memory(NudgeState::default());
        e.policy = Policy::Network;
        e.net = eager();
        let mut rng = Rng::new(5);
        let mut usage = Usage { dictations_ms: vec![NOON - 10 * DAY_MS], saved_secs_month: 0 };
        let mut sent = 0;
        for day in 0..20 {
            if e.decide(NOON + day * DAY_MS, 0, &usage, &mut rng).is_ok() {
                sent += 1;
            }
        }
        assert_eq!(sent, 2, "two reminders, both ignored, then silence");
        usage.dictations_ms.push(NOON + 20 * DAY_MS);
        assert_eq!(e.decide(NOON + 21 * DAY_MS, 0, &usage, &mut rng), Err(Skip::Rule(Block::RecentlyActive)));
        assert!(e.decide(NOON + 23 * DAY_MS, 0, &usage, &mut rng).is_ok(), "back after dictating");
    }

    #[test]
    fn the_default_policy_is_the_usual_hour() {
        let mut e = Engine::in_memory(NudgeState::default());
        assert_eq!(e.policy, Policy::UsualHour);
        let mut rng = Rng::new(7);
        // Usually dictates at 10:00 (14 days of it, the last 5 days ago).
        let days: Vec<u64> = (5..19).map(|d| NOON - 2 * HOUR_MS - d * DAY_MS).collect();
        let usage = Usage { dictations_ms: days, saved_secs_month: 0 };
        // 14:00 is far from 10:00: wait.
        assert_eq!(e.decide(NOON + 2 * HOUR_MS, 0, &usage, &mut rng), Err(Skip::ModelWaits));
        // 10:30 the next day: the tip goes out.
        let n = e.decide(NOON + DAY_MS - 90 * 60_000, 0, &usage, &mut rng).unwrap();
        assert_eq!(n.style, Style::Tip);
    }

    #[test]
    fn usual_hour_defaults_to_the_evening_and_never_picks_quiet() {
        let mut ctx = context(NOON, 0, &[], &NudgeState::default());
        assert_eq!(usual_hour(&ctx, &[true; K_STYLES]), None, "noon is far from 19:00");
        ctx.hour = 19.5;
        assert_eq!(usual_hour(&ctx, &[true; K_STYLES]), Some(Style::Tip));
        assert_eq!(usual_hour(&ctx, &[true, false, false, true]), Some(Style::Plain));
        assert_eq!(usual_hour(&ctx, &[false, false, false, true]), None);
        assert_eq!(hour_gap(23.5, 0.5), 1.0);
    }

    #[test]
    fn a_damaged_file_keeps_reminders_off() {
        let dir = std::env::temp_dir().join(format!("voxa-nudge-damaged-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join(STATE_FILE);
        // No file: a fresh start, reminders on.
        let _ = std::fs::remove_file(&file);
        assert!(Engine::open(&dir).state.enabled);
        // Garbage: off.
        std::fs::write(&file, br#"{"enabled": tru"#).unwrap();
        assert!(!Engine::open(&dir).state.enabled);
        // Off, with a field the engine can't read: stays off.
        std::fs::write(&file, br#"{"enabled": false, "sent_ms": "oops"}"#).unwrap();
        assert!(!Engine::open(&dir).state.enabled);
        // A newer, unknown version: off unless it says on, and nothing else is trusted.
        std::fs::write(&file, br#"{"version": 99, "sent_ms": [1]}"#).unwrap();
        let e = Engine::open(&dir);
        assert!(!e.state.enabled);
        assert!(e.state.sent_ms.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_next_check_is_the_next_usual_hour_in_daytime() {
        let mut e = Engine::in_memory(NudgeState::default());
        // No history: 19:00 today, seen from noon.
        assert_eq!(e.next_check_ms(NOON, 0, &Usage::default()), NOON + 7 * HOUR_MS);
        // Seen from 19:05: tomorrow's 19:00.
        assert_eq!(e.next_check_ms(NOON + 7 * HOUR_MS + 5 * 60_000, 0, &Usage::default()), NOON + DAY_MS + 7 * HOUR_MS);
        // A night owl (usual hour 23:00) is checked at 20:30, inside the daytime window.
        let owl = Usage { dictations_ms: (1..10).map(|d| NOON + 11 * HOUR_MS - d * DAY_MS).collect(), saved_secs_month: 0 };
        let next = e.next_check_ms(NOON, 0, &owl);
        assert!((next as i64 - (NOON + 8 * HOUR_MS + 30 * 60_000) as i64).abs() < 60_000, "{}", next - NOON);
    }
}
