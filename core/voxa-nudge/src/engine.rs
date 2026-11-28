//! The engine as the phone uses it: hard rules first, then the network's prior, then the
//! on-device bandit; outcomes feed back into the bandit. The state is a small JSON file.

use std::path::{Path, PathBuf};

use crate::bandit::{send_probability, slot_of};
use crate::features::{Context, context, encode, local};
use crate::messages::{K_STYLES, Style, available, message};
use crate::net::Net;
use crate::rng::Rng;
use crate::rules::{self, Block};
use crate::state::{NudgeState, Outcome};

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
}

pub const STATE_FILE: &str = "nudge.json";

pub struct Engine {
    pub net: Net,
    pub state: NudgeState,
    path: Option<PathBuf>,
}

impl Engine {
    /// An engine with the shipped model, not saved anywhere.
    pub fn in_memory(state: NudgeState) -> Self {
        Self { net: bundled(), state, path: None }
    }

    /// Loads (or starts) the state in `dir`/nudge.json. A damaged file starts over (reminders on).
    pub fn open(dir: &Path) -> Self {
        let path = dir.join(STATE_FILE);
        let state = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { net: bundled(), state, path: Some(path) }
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Ok(json) = serde_json::to_vec(&self.state) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.state.enabled = on;
        self.save();
    }

    /// Asked every couple of hours by the app's background job. Sends at most what the rules
    /// allow; a sent reminder is remembered and waits for its outcome.
    pub fn decide(&mut self, now_ms: u64, utc_offset_minutes: i32, usage: &Usage, rng: &mut Rng) -> Result<Nudge, Skip> {
        self.state.remember(&usage.dictations_ms, now_ms);
        let dictations = self.state.dictations_ms.clone();
        let last = dictations.iter().copied().filter(|&t| t <= now_ms).max();
        self.state.observe(now_ms, last);
        let here = local(now_ms, utc_offset_minutes);
        let verdict = rules::check(now_ms, here.hour, last, &self.state);
        let result = match verdict {
            Err(b) => Err(Skip::Rule(b)),
            Ok(()) => {
                let ctx = context(now_ms, utc_offset_minutes, &dictations, &self.state);
                match choose(&self.net, &ctx, &self.state, &available(usage.saved_secs_month), true, rng) {
                    None => Err(Skip::ModelWaits),
                    Some((style, slot)) => {
                        self.state.record_send(now_ms, style, slot);
                        let (title, body) = message(style, usage.saved_secs_month, here.day);
                        Ok(Nudge { style, title, body })
                    }
                }
            }
        };
        self.save();
        result
    }

    /// The user tapped (Opened) or swiped away (Dismissed) the reminder.
    pub fn record_outcome(&mut self, outcome: Outcome) {
        self.state.record_outcome(outcome);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{DAY_MS, HOUR_MS};
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
}
