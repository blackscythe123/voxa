//! When to check, so checks stay cheap and never spam GitHub (owner rule, 2026-11-28):
//!
//! - At most one automatic check per 24 hours per device, never "on every start".
//! - Each install checks at its own time of day (a random slot stored once), so a release is not
//!   fetched by everyone at the same moment.
//! - A failed check backs off: 1 hour, then 4 hours, then the next day's slot. Never a tight retry.
//! - "Check for updates" by hand works at most once a minute.
//! - The feed's ETag and Last-Modified are kept, so an unchanged feed costs a 304 with no body
//!   (the cached copy is verified again every time, like a fresh one).
//!
//! Pure: the caller passes the time and stores [`CheckState`] (desktop update.json, Android prefs).

use serde::{Deserialize, Serialize};

pub const HOUR_MS: u64 = 60 * 60 * 1000;
pub const DAY_MS: u64 = 24 * HOUR_MS;
/// Retry delays after the first and second failed automatic check; after that, the next day.
pub const BACKOFF_MS: [u64; 2] = [HOUR_MS, 4 * HOUR_MS];
/// "Check for updates" pressed again within this long reuses the last answer.
pub const MANUAL_GAP_MS: u64 = 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CheckState {
    /// This install's time of day for the daily check, in ms after 00:00 UTC (set once).
    pub slot_ms: Option<u64>,
    /// The next automatic check is due at or after this time.
    pub next_due_ms: u64,
    pub last_ok_ms: u64,
    pub last_attempt_ms: u64,
    pub last_manual_ms: u64,
    /// Automatic checks that failed in a row.
    pub failures: u32,
    /// The highest feed serial verified so far (anti-replay).
    pub highest_serial: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// The last feed body (base64), served again on a 304 and verified again.
    pub feed_b64: Option<String>,
}

impl CheckState {
    /// Pick this install's slot once (from `random`), and schedule the first check at the next
    /// occurrence of it.
    pub fn init(&mut self, now_ms: u64, random: u64) {
        if self.slot_ms.is_none() {
            let slot = random % DAY_MS;
            self.slot_ms = Some(slot);
            if self.next_due_ms == 0 {
                self.next_due_ms = slot_at_or_after(now_ms, slot);
            }
        }
    }

    /// A random slot from the OS's secure random source (ring), or a time based one.
    pub fn init_random(&mut self, now_ms: u64) {
        let mut b = [0u8; 8];
        let r = match ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b) {
            Ok(()) => u64::from_le_bytes(b),
            Err(_) => now_ms.wrapping_mul(6364136223846793005).rotate_left(17),
        };
        self.init(now_ms, r);
    }

    /// An automatic check is due. A schedule far in the future (the clock went back) counts as due.
    pub fn due(&self, now_ms: u64) -> bool {
        self.slot_ms.is_some() && (now_ms >= self.next_due_ms || self.next_due_ms > now_ms.saturating_add(2 * DAY_MS))
    }

    /// A manual check may go to the network now (else the last answer is shown again).
    pub fn manual_allowed(&self, now_ms: u64) -> bool {
        self.last_manual_ms == 0 || now_ms < self.last_manual_ms || now_ms - self.last_manual_ms >= MANUAL_GAP_MS
    }

    /// Record a check's outcome. Any good answer (automatic or manual) moves the next automatic
    /// check to this install's slot at least 24 h later. Only automatic failures back off; a
    /// failed manual check leaves the automatic schedule as it was.
    pub fn record(&mut self, now_ms: u64, ok: bool, manual: bool) {
        let slot = self.slot_ms.unwrap_or(0);
        self.last_attempt_ms = now_ms;
        if manual {
            self.last_manual_ms = now_ms;
        }
        if ok {
            self.failures = 0;
            self.last_ok_ms = now_ms;
            self.next_due_ms = slot_at_or_after(now_ms.saturating_add(DAY_MS), slot);
        } else if !manual {
            self.failures = self.failures.saturating_add(1);
            self.next_due_ms = match BACKOFF_MS.get(self.failures as usize - 1) {
                Some(d) => now_ms.saturating_add(*d),
                None => slot_at_or_after(now_ms.saturating_add(DAY_MS), slot),
            };
        }
    }

    /// Remember a 200's validators and body.
    pub fn store_feed(&mut self, body: &[u8], etag: Option<String>, last_modified: Option<String>) {
        self.feed_b64 = Some(crate::b64_encode(body));
        self.etag = etag;
        self.last_modified = last_modified;
    }

    pub fn cached_feed(&self) -> Option<Vec<u8>> {
        self.feed_b64.as_deref().and_then(crate::b64_decode)
    }
}

/// The first time at or after `t` whose time of day (UTC) is `slot`.
pub fn slot_at_or_after(t: u64, slot: u64) -> u64 {
    let day = t - t % DAY_MS;
    let s = day + slot % DAY_MS;
    if s >= t { s } else { s + DAY_MS }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000; // some day, ms

    fn fresh(slot: u64) -> CheckState {
        let mut s = CheckState::default();
        s.init(T0, slot);
        s
    }

    #[test]
    fn check_due_respects_24_hours_and_the_per_install_offset() {
        let slot = 7 * HOUR_MS + 13 * 60 * 1000;
        let mut s = fresh(slot);
        let first = s.next_due_ms;
        assert_eq!(first % DAY_MS, slot, "the first check is at this install's slot");
        assert!(first >= T0 && first < T0 + DAY_MS);
        assert!(!s.due(first - 1));
        assert!(s.due(first));
        s.record(first, true, false);
        // Not again for 24 hours, however often Voxa starts.
        for t in (first..first + DAY_MS).step_by((HOUR_MS / 2) as usize) {
            assert!(!s.due(t), "due again at +{} h", (t - first) / HOUR_MS);
        }
        assert!(s.due(first + DAY_MS));
        assert_eq!(s.next_due_ms % DAY_MS, slot);
        // A device asleep at its slot checks once when it wakes, then the slot at least 24 h later.
        let late = first + DAY_MS + 5 * HOUR_MS;
        assert!(s.due(late));
        s.record(late, true, false);
        assert!(s.next_due_ms >= late + DAY_MS);
        assert_eq!(s.next_due_ms % DAY_MS, slot);
        // The slot is chosen once.
        s.init(late, 999);
        assert_eq!(s.slot_ms, Some(slot));
        // Two installs with different random numbers check at different times of day.
        assert_ne!(fresh(1).next_due_ms % DAY_MS, fresh(5 * HOUR_MS).next_due_ms % DAY_MS);
    }

    #[test]
    fn a_random_slot_is_within_a_day() {
        let mut s = CheckState::default();
        s.init_random(T0);
        assert!(s.slot_ms.unwrap() < DAY_MS);
        assert!(s.next_due_ms >= T0 && s.next_due_ms < T0 + DAY_MS);
    }

    #[test]
    fn the_manual_check_rate_limit_is_once_a_minute() {
        let mut s = fresh(0);
        assert!(s.manual_allowed(T0));
        s.record(T0, true, true);
        assert!(!s.manual_allowed(T0 + 1));
        assert!(!s.manual_allowed(T0 + MANUAL_GAP_MS - 1));
        assert!(s.manual_allowed(T0 + MANUAL_GAP_MS));
        // A good manual answer also counts as today's automatic check.
        assert!(!s.due(T0 + HOUR_MS));
    }

    #[test]
    fn the_backoff_sequence_is_one_hour_four_hours_then_the_next_day() {
        let slot = 3 * HOUR_MS;
        let mut s = fresh(slot);
        let t = s.next_due_ms;
        s.record(t, false, false);
        assert_eq!(s.next_due_ms, t + HOUR_MS);
        let t2 = s.next_due_ms;
        s.record(t2, false, false);
        assert_eq!(s.next_due_ms, t2 + 4 * HOUR_MS);
        let t3 = s.next_due_ms;
        s.record(t3, false, false);
        assert!(s.next_due_ms >= t3 + DAY_MS);
        assert_eq!(s.next_due_ms % DAY_MS, slot);
        let t4 = s.next_due_ms;
        s.record(t4, false, false);
        assert!(s.next_due_ms >= t4 + DAY_MS, "still once a day while it keeps failing");
        s.record(s.next_due_ms, true, false);
        assert_eq!(s.failures, 0);
        // A failed manual check never speeds up or slows down the automatic schedule.
        let before = s.next_due_ms;
        s.record(before - HOUR_MS, false, true);
        assert_eq!(s.next_due_ms, before);
        assert_eq!(s.failures, 0);
    }

    #[test]
    fn a_clock_set_back_does_not_stop_checks_forever() {
        let mut s = fresh(0);
        s.record(T0 + 30 * DAY_MS, true, false);
        assert!(s.due(T0), "next due a month ahead of the clock counts as due");
    }

    #[test]
    fn the_cached_feed_round_trips() {
        let mut s = fresh(0);
        s.store_feed(b"{\"key_id\":1}", Some("\"abc\"".into()), Some("Sat, 28 Nov 2026 06:49:48 GMT".into()));
        assert_eq!(s.cached_feed().unwrap(), b"{\"key_id\":1}");
        let back: CheckState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        let old: CheckState = serde_json::from_str("{\"highest_serial\":5}").unwrap();
        assert_eq!(old.highest_serial, 5);
        assert!(old.slot_ms.is_none());
    }
}
