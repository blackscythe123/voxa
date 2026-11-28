//! Hard rules, checked in code before the model is asked anything. The model can never override
//! them: when a rule blocks, the network is not even run.

use crate::features::{DAY_MS, HOUR_MS};
use crate::state::NudgeState;

/// At most one reminder every 3 days.
pub const MIN_GAP_MS: u64 = 3 * DAY_MS;
/// No reminders from 21:00 to 09:00 local time.
pub const QUIET_FROM_HOUR: f32 = 21.0;
pub const QUIET_UNTIL_HOUR: f32 = 9.0;
/// No reminders when the user dictated in the last 48 hours.
pub const ACTIVE_WINDOW_MS: u64 = 48 * HOUR_MS;
/// Stop after this many ignored (or dismissed) reminders in a row, until the user dictates again.
pub const MAX_IGNORED_IN_A_ROW: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// The Settings switch is off.
    Off,
    /// Between 21:00 and 09:00 local time.
    NightTime,
    /// The user dictated in the last 48 hours.
    RecentlyActive,
    /// A reminder went out less than 3 days ago.
    TooSoon,
    /// The last two reminders were ignored and the user has not dictated since.
    IgnoredTwice,
    /// The "We'll stay quiet" reminder went out and the user has not dictated since.
    PromisedQuiet,
}

pub fn is_night(local_hour: f32) -> bool {
    local_hour >= QUIET_FROM_HOUR || local_hour < QUIET_UNTIL_HOUR
}

/// `Ok` when a reminder may be sent now.
pub fn check(now_ms: u64, local_hour: f32, last_dictation_ms: Option<u64>, state: &NudgeState) -> Result<(), Block> {
    if !state.enabled {
        return Err(Block::Off);
    }
    if is_night(local_hour) {
        return Err(Block::NightTime);
    }
    if last_dictation_ms.is_some_and(|d| now_ms < d.saturating_add(ACTIVE_WINDOW_MS)) {
        return Err(Block::RecentlyActive);
    }
    if state.last_sent().is_some_and(|s| now_ms < s.saturating_add(MIN_GAP_MS)) {
        return Err(Block::TooSoon);
    }
    let came_back = state.dictated_since_last_nudge(last_dictation_ms);
    if state.ignored_streak >= MAX_IGNORED_IN_A_ROW && !came_back {
        return Err(Block::IgnoredTwice);
    }
    if state.quiet_paused && !came_back {
        return Err(Block::PromisedQuiet);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::Style;
    use crate::state::Outcome;

    const NOON: f32 = 12.0;
    /// Day 100 at noon UTC, far from any edge.
    const NOW: u64 = 100 * DAY_MS + 12 * HOUR_MS;

    fn quiet_user() -> Option<u64> {
        Some(NOW - 10 * DAY_MS)
    }

    #[test]
    fn allowed_when_nothing_blocks() {
        assert_eq!(check(NOW, NOON, quiet_user(), &NudgeState::default()), Ok(()));
        assert_eq!(check(NOW, NOON, None, &NudgeState::default()), Ok(()), "never dictated");
    }

    #[test]
    fn off_switch() {
        let s = NudgeState { enabled: false, ..Default::default() };
        assert_eq!(check(NOW, NOON, quiet_user(), &s), Err(Block::Off));
    }

    #[test]
    fn never_between_21_and_9() {
        let s = NudgeState::default();
        for h in [21.0, 22.5, 23.99, 0.0, 3.0, 8.99] {
            assert_eq!(check(NOW, h, quiet_user(), &s), Err(Block::NightTime), "hour {h}");
        }
        for h in [9.0, 12.0, 20.99] {
            assert_eq!(check(NOW, h, quiet_user(), &s), Ok(()), "hour {h}");
        }
    }

    #[test]
    fn never_within_48_hours_of_a_dictation() {
        let s = NudgeState::default();
        assert_eq!(check(NOW, NOON, Some(NOW - 47 * HOUR_MS), &s), Err(Block::RecentlyActive));
        assert_eq!(check(NOW, NOON, Some(NOW - 48 * HOUR_MS), &s), Ok(()));
    }

    #[test]
    fn at_most_one_every_3_days() {
        let mut s = NudgeState::default();
        s.record_send(NOW - 3 * DAY_MS + 1, Style::Plain, None);
        s.record_outcome(Outcome::Opened);
        assert_eq!(check(NOW, NOON, quiet_user(), &s), Err(Block::TooSoon));
        let mut s = NudgeState::default();
        s.record_send(NOW - 3 * DAY_MS, Style::Plain, None);
        s.record_outcome(Outcome::Opened);
        assert_eq!(check(NOW, NOON, quiet_user(), &s), Ok(()));
    }

    #[test]
    fn stops_after_two_ignored_until_the_user_dictates() {
        let mut s = NudgeState::default();
        for t in [NOW - 9 * DAY_MS, NOW - 5 * DAY_MS] {
            s.record_send(t, Style::Plain, None);
            s.record_outcome(Outcome::Ignored);
        }
        assert_eq!(check(NOW, NOON, Some(NOW - 20 * DAY_MS), &s), Err(Block::IgnoredTwice));
        // Dismissals count too.
        let mut d = NudgeState::default();
        for t in [NOW - 9 * DAY_MS, NOW - 5 * DAY_MS] {
            d.record_send(t, Style::Tip, None);
            d.record_outcome(Outcome::Dismissed);
        }
        assert_eq!(check(NOW, NOON, Some(NOW - 20 * DAY_MS), &d), Err(Block::IgnoredTwice));
        // One ignored is not enough.
        let mut one = NudgeState::default();
        one.record_send(NOW - 5 * DAY_MS, Style::Plain, None);
        one.record_outcome(Outcome::Ignored);
        assert_eq!(check(NOW, NOON, Some(NOW - 20 * DAY_MS), &one), Ok(()));
        // A dictation after the last reminder lifts it, even before observe() resets the streak.
        assert_eq!(check(NOW, NOON, Some(NOW - 4 * DAY_MS), &s), Ok(()));
    }

    #[test]
    fn quiet_promise_is_kept() {
        let mut s = NudgeState::default();
        s.record_send(NOW - 6 * DAY_MS, Style::Quiet, None);
        s.record_outcome(Outcome::Opened);
        assert_eq!(check(NOW, NOON, Some(NOW - 20 * DAY_MS), &s), Err(Block::PromisedQuiet));
        assert_eq!(check(NOW, NOON, Some(NOW - 3 * DAY_MS), &s), Ok(()));
    }
}
