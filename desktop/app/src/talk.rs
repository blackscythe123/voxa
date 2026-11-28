//! How long the user talked in a take: paused time is left out, also when the take is stopped
//! while paused (the core has already left `Recording { paused }` by the time the shell stops
//! the mic, so the shell keeps its own clock).

use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct TalkClock {
    talked: Duration,
    /// When the mic last went live, while it is live.
    live_since: Option<Instant>,
}

impl TalkClock {
    /// A new take: the mic is live from `now`.
    pub fn start(&mut self, now: Instant) {
        self.talked = Duration::ZERO;
        self.live_since = Some(now);
    }

    pub fn pause(&mut self, now: Instant) {
        if let Some(t) = self.live_since.take() {
            self.talked += now.saturating_duration_since(t);
        }
    }

    /// The mic is live again from `now` (call once it really reopened).
    pub fn resume(&mut self, now: Instant) {
        if self.live_since.is_none() {
            self.live_since = Some(now);
        }
    }

    /// Talking time so far.
    pub fn total(&self, now: Instant) -> Duration {
        self.talked + self.live_since.map(|t| now.saturating_duration_since(t)).unwrap_or_default()
    }

    /// The take ended: its talking time, and the clock stops.
    pub fn stop(&mut self, now: Instant) -> Duration {
        self.pause(now);
        self.talked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_while_paused_leaves_the_pause_out() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        let mut c = TalkClock::default();
        c.start(s(0));
        c.pause(s(10));
        // Five minutes of pause, then Stop without resuming.
        assert_eq!(c.total(s(310)), Duration::from_secs(10));
        assert_eq!(c.stop(s(310)), Duration::from_secs(10));
    }

    #[test]
    fn resume_adds_only_the_talking_after_it() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        let mut c = TalkClock::default();
        c.start(s(0));
        c.pause(s(4));
        c.resume(s(60));
        assert_eq!(c.total(s(63)), Duration::from_secs(7));
        assert_eq!(c.stop(s(66)), Duration::from_secs(10));
        c.start(s(100));
        assert_eq!(c.stop(s(101)), Duration::from_secs(1), "a new take starts from zero");
    }
}
