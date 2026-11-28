//! What the engine remembers between runs (saved as JSON in the app's data folder): the switch,
//! recent reminders, the reminder waiting for an outcome, and the bandit's counts.

use serde::{Deserialize, Serialize};

use crate::bandit::Bandit;
use crate::features::DAY_MS;
use crate::messages::Style;

/// What happened to a reminder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Tapped, or followed by a dictation, within 24 hours.
    Opened,
    /// No reaction within 24 hours.
    Ignored,
    /// Swiped away.
    Dismissed,
}

impl Outcome {
    pub fn success(self) -> bool {
        self == Outcome::Opened
    }
}

/// How long a reminder may wait for a tap before it counts as ignored.
pub const OUTCOME_WINDOW_MS: u64 = DAY_MS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub sent_ms: u64,
    pub style: Style,
    pub slot: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NudgeState {
    /// The Settings switch "Reminders". On by default.
    pub enabled: bool,
    /// When reminders were sent, last 14 days (and always the latest).
    pub sent_ms: Vec<u64>,
    pub pending: Option<Pending>,
    pub last_outcome: Option<Outcome>,
    /// Reminders in a row that were ignored or dismissed, since the user last dictated or opened one.
    pub ignored_streak: u32,
    /// The "We'll stay quiet" reminder went out: no more until the user dictates again.
    pub quiet_paused: bool,
    /// Dictation start times seen in the last 28 days, so the features survive History being
    /// pruned or cleared (at most `MAX_REMEMBERED`).
    pub dictations_ms: Vec<u64>,
    pub bandit: Bandit,
}

/// The most dictation times kept.
pub const MAX_REMEMBERED: usize = 400;

impl Default for NudgeState {
    fn default() -> Self {
        Self {
            enabled: true,
            sent_ms: Vec::new(),
            pending: None,
            last_outcome: None,
            ignored_streak: 0,
            quiet_paused: false,
            dictations_ms: Vec::new(),
            bandit: Bandit::default(),
        }
    }
}

impl NudgeState {
    /// Merges dictation times from History into the remembered ones (last 28 days, newest kept).
    pub fn remember(&mut self, dictations_ms: &[u64], now_ms: u64) {
        let since = now_ms.saturating_sub(crate::features::LOOKBACK_DAYS * DAY_MS);
        self.dictations_ms.extend(dictations_ms.iter().copied().filter(|&t| t >= since && t <= now_ms));
        self.dictations_ms.retain(|&t| t >= since);
        self.dictations_ms.sort_unstable();
        self.dictations_ms.dedup();
        let extra = self.dictations_ms.len().saturating_sub(MAX_REMEMBERED);
        self.dictations_ms.drain(..extra);
    }

    pub fn last_sent(&self) -> Option<u64> {
        self.sent_ms.iter().copied().max()
    }

    /// Whether the user dictated after the latest reminder.
    pub fn dictated_since_last_nudge(&self, last_dictation_ms: Option<u64>) -> bool {
        match (last_dictation_ms, self.last_sent()) {
            (Some(d), Some(s)) => d > s,
            _ => false,
        }
    }

    /// Brings the state up to date before a decision: a dictation after the latest reminder ends a
    /// pause and an ignored streak, and a reminder with no tap after 24 hours is settled (opened if
    /// the user dictated within the window, ignored otherwise).
    pub fn observe(&mut self, now_ms: u64, last_dictation_ms: Option<u64>) {
        if let Some(p) = self.pending.clone() {
            let came_back = last_dictation_ms.is_some_and(|d| d > p.sent_ms && d <= p.sent_ms + OUTCOME_WINDOW_MS);
            if came_back {
                self.record_outcome(Outcome::Opened);
            } else if now_ms >= p.sent_ms + OUTCOME_WINDOW_MS {
                self.record_outcome(Outcome::Ignored);
            }
        }
        if self.dictated_since_last_nudge(last_dictation_ms) {
            self.ignored_streak = 0;
            self.quiet_paused = false;
        }
    }

    pub fn record_send(&mut self, now_ms: u64, style: Style, slot: Option<usize>) {
        // A reminder still waiting is overtaken: it got no reaction.
        if self.pending.is_some() {
            self.record_outcome(Outcome::Ignored);
        }
        self.sent_ms.retain(|&t| t + 14 * DAY_MS >= now_ms);
        self.sent_ms.push(now_ms);
        self.pending = Some(Pending { sent_ms: now_ms, style, slot });
        if style == Style::Quiet {
            self.quiet_paused = true;
        }
    }

    /// Settles the waiting reminder. Ignored when nothing is waiting (a late or repeated tap).
    pub fn record_outcome(&mut self, outcome: Outcome) {
        let Some(p) = self.pending.take() else { return };
        self.bandit.update(p.style.index(), p.slot, outcome.success());
        self.last_outcome = Some(outcome);
        if outcome.success() {
            self.ignored_streak = 0;
        } else {
            self.ignored_streak += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::HOUR_MS;

    #[test]
    fn untapped_reminder_settles_after_a_day() {
        let mut s = NudgeState::default();
        s.record_send(0, Style::Plain, Some(1));
        s.observe(23 * HOUR_MS, None);
        assert!(s.pending.is_some());
        s.observe(24 * HOUR_MS, None);
        assert!(s.pending.is_none());
        assert_eq!(s.last_outcome, Some(Outcome::Ignored));
        assert_eq!(s.ignored_streak, 1);
        assert_eq!(s.bandit.style[0], [0.0, 1.0]);
    }

    #[test]
    fn dictating_after_a_reminder_counts_as_opened_and_ends_the_streak() {
        let mut s = NudgeState { ignored_streak: 1, ..Default::default() };
        s.record_send(0, Style::Tip, Some(0));
        s.observe(5 * HOUR_MS, Some(4 * HOUR_MS));
        assert_eq!(s.last_outcome, Some(Outcome::Opened));
        assert_eq!(s.ignored_streak, 0);
        assert_eq!(s.bandit.style[2], [1.0, 0.0]);
        assert_eq!(s.bandit.slot[0], [1.0, 0.0]);
    }

    #[test]
    fn explicit_outcome_wins_and_late_taps_do_nothing() {
        let mut s = NudgeState::default();
        s.record_send(0, Style::Plain, None);
        s.record_outcome(Outcome::Dismissed);
        s.record_outcome(Outcome::Opened);
        assert_eq!(s.last_outcome, Some(Outcome::Dismissed));
        assert_eq!(s.ignored_streak, 1);
    }

    #[test]
    fn old_sends_are_forgotten_but_the_latest_is_kept() {
        let mut s = NudgeState::default();
        s.record_send(0, Style::Plain, None);
        s.record_send(20 * DAY_MS, Style::Plain, None);
        assert_eq!(s.sent_ms, vec![20 * DAY_MS]);
        assert_eq!(s.ignored_streak, 1, "the overtaken reminder counted as ignored");
    }

    #[test]
    fn quiet_style_pauses_until_the_next_dictation() {
        let mut s = NudgeState::default();
        s.record_send(10, Style::Quiet, None);
        assert!(s.quiet_paused);
        s.observe(5 * DAY_MS, Some(5));
        assert!(s.quiet_paused, "a dictation before the reminder does not count");
        s.observe(5 * DAY_MS, Some(4 * DAY_MS));
        assert!(!s.quiet_paused);
    }

    #[test]
    fn remembers_recent_dictations_only() {
        let mut s = NudgeState::default();
        let now = 100 * DAY_MS;
        s.remember(&[now - 40 * DAY_MS, now - 2 * DAY_MS, now - DAY_MS, now + DAY_MS], now);
        s.remember(&[now - DAY_MS], now);
        assert_eq!(s.dictations_ms, vec![now - 2 * DAY_MS, now - DAY_MS]);
        let many: Vec<u64> = (0..1000).map(|i| now - i * 60_000).collect();
        s.remember(&many, now);
        assert_eq!(s.dictations_ms.len(), MAX_REMEMBERED);
        assert_eq!(*s.dictations_ms.last().unwrap(), now, "the newest are kept");
    }

    #[test]
    fn survives_json_and_old_files() {
        let mut s = NudgeState::default();
        s.record_send(1, Style::Progress, Some(2));
        let back: NudgeState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let empty: NudgeState = serde_json::from_str("{}").unwrap();
        assert!(empty.enabled, "missing fields take the defaults: reminders on");
    }
}
