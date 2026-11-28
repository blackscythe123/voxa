//! Which recordings are uploaded, and when. The take on the strip goes at once: the user is
//! waiting for it. Everything else (takes that waited for the network or the sign-in, History's
//! Send now, earlier runs' waiting takes) goes one at a time, so takes never split a slow uplink
//! between them and time out together. What Voxa sends on its own (not the user asking) waits
//! longer after each failure, and stops after a few, so a flapping network never re-uploads
//! the same recordings every few seconds. Pure: the caller passes the time.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A dictation of this run (voxa-core job id).
    Job(u64),
    /// A History entry from an earlier run (entry id).
    Entry(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The take on the strip: starts now, next to anything else running.
    Strip,
    /// The user asked (Send now, Write it out again): first in line, no waiting.
    User,
    /// Voxa decided (back online, signed in, an earlier run's take).
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    /// No connection or no answer in time: the kind of failure worth trying again later.
    Unreachable,
    /// Anything else (sign-in, server error): not retried on a timer.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// Start it now (strip lane).
    StartNow,
    /// In the background line.
    Queued,
    /// Already queued or uploading: nothing to do (never two uploads of one recording).
    Already,
    /// Voxa tried on its own too often: stop and leave it for the user's Send now.
    GiveUp,
}

/// Waits before Voxa itself tries again after the 1st, 2nd and later failures.
pub const BACKOFF: [Duration; 3] = [Duration::from_secs(30), Duration::from_secs(120), Duration::from_secs(600)];
/// Failures after which Voxa stops sending a recording on its own.
pub const CAP: u32 = 4;

#[derive(Default)]
pub struct Uploads {
    line: VecDeque<Key>,
    running: Vec<Key>,
    /// The one background upload running now.
    background: Option<Key>,
    failures: HashMap<Key, u32>,
    not_before: HashMap<Key, Instant>,
}

impl Uploads {
    pub fn contains(&self, key: Key) -> bool {
        self.line.contains(&key) || self.running.contains(&key)
    }

    /// Queued, but held back by its wait after a failure.
    pub fn held(&self, key: Key, now: Instant) -> bool {
        self.line.contains(&key) && self.not_before.get(&key).is_some_and(|t| *t > now)
    }

    pub fn push(&mut self, key: Key, why: Why, _now: Instant) -> Pushed {
        if self.contains(key) {
            // The user asking moves a held one to the front, without its wait.
            if why == Why::User && self.line.contains(&key) {
                self.line.retain(|k| *k != key);
                self.line.push_front(key);
                self.not_before.remove(&key);
            }
            return Pushed::Already;
        }
        match why {
            Why::Strip => {
                self.running.push(key);
                Pushed::StartNow
            }
            Why::User => {
                self.failures.remove(&key);
                self.not_before.remove(&key);
                self.line.push_front(key);
                Pushed::Queued
            }
            Why::Auto => {
                if self.failures.get(&key).copied().unwrap_or(0) >= CAP {
                    return Pushed::GiveUp;
                }
                self.line.push_back(key);
                Pushed::Queued
            }
        }
    }

    /// The next background upload to start, if the lane is free and one is due.
    pub fn next(&mut self, now: Instant) -> Option<Key> {
        if self.background.is_some() {
            return None;
        }
        let at = self.line.iter().position(|k| self.not_before.get(k).is_none_or(|t| *t <= now))?;
        let key = self.line.remove(at)?;
        self.background = Some(key);
        self.running.push(key);
        Some(key)
    }

    /// An upload ended. Unreachable sets the wait before Voxa may try it again on its own.
    pub fn finished(&mut self, key: Key, outcome: Outcome, now: Instant) {
        self.running.retain(|k| *k != key);
        if self.background == Some(key) {
            self.background = None;
        }
        match outcome {
            Outcome::Sent => {
                self.failures.remove(&key);
                self.not_before.remove(&key);
            }
            Outcome::Unreachable => {
                let n = self.failures.entry(key).or_insert(0);
                *n += 1;
                let wait = BACKOFF[(*n as usize - 1).min(BACKOFF.len() - 1)];
                self.not_before.insert(key, now + wait);
            }
            Outcome::Other => {}
        }
    }

    /// Forget a recording that is gone (deleted, or answered without an upload).
    pub fn drop_key(&mut self, key: Key) {
        self.line.retain(|k| *k != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_takes_are_uploaded_one_at_a_time() {
        let t = Instant::now();
        let mut u = Uploads::default();
        for j in 1..=3 {
            assert_eq!(u.push(Key::Job(j), Why::Auto, t), Pushed::Queued);
        }
        assert_eq!(u.next(t), Some(Key::Job(1)));
        assert_eq!(u.next(t), None, "a second background upload waits for the first");
        u.finished(Key::Job(1), Outcome::Sent, t);
        assert_eq!(u.next(t), Some(Key::Job(2)));
    }

    #[test]
    fn the_take_on_the_strip_never_waits_behind_the_background_line() {
        let t = Instant::now();
        let mut u = Uploads::default();
        u.push(Key::Job(1), Why::Auto, t);
        assert_eq!(u.next(t), Some(Key::Job(1)));
        assert_eq!(u.push(Key::Job(2), Why::Strip, t), Pushed::StartNow);
    }

    #[test]
    fn one_recording_is_never_uploaded_twice_at_once() {
        let t = Instant::now();
        let mut u = Uploads::default();
        u.push(Key::Entry(7), Why::User, t);
        assert_eq!(u.push(Key::Entry(7), Why::User, t), Pushed::Already);
        assert_eq!(u.next(t), Some(Key::Entry(7)));
        assert_eq!(u.push(Key::Entry(7), Why::Auto, t), Pushed::Already);
        assert_eq!(u.push(Key::Entry(7), Why::Strip, t), Pushed::Already);
    }

    #[test]
    fn a_flapping_network_backs_off_and_then_stops_resending() {
        let t0 = Instant::now();
        let mut u = Uploads::default();
        let k = Key::Job(1);
        let mut now = t0;
        let mut uploads = 0;
        // A session check says "online" every 5 s for an hour; every upload drops.
        for _ in 0..720 {
            now += Duration::from_secs(5);
            if !u.contains(k) && u.push(k, Why::Auto, now) == Pushed::GiveUp {
                continue;
            }
            if u.next(now) == Some(k) {
                uploads += 1;
                u.finished(k, Outcome::Unreachable, now);
            }
        }
        assert_eq!(uploads, CAP as usize, "uploads in an hour of flapping");
        assert_eq!(u.push(k, Why::Auto, now), Pushed::GiveUp);
        // The user's Send now still works, at once.
        assert_eq!(u.push(k, Why::User, now), Pushed::Queued);
        assert_eq!(u.next(now), Some(k));
    }

    #[test]
    fn waits_grow_after_each_failure() {
        let t = Instant::now();
        let mut u = Uploads::default();
        let k = Key::Entry(1);
        u.push(k, Why::Auto, t);
        u.next(t);
        u.finished(k, Outcome::Unreachable, t);
        u.push(k, Why::Auto, t);
        assert!(u.held(k, t));
        assert_eq!(u.next(t + Duration::from_secs(29)), None);
        assert_eq!(u.next(t + Duration::from_secs(30)), Some(k));
        u.finished(k, Outcome::Unreachable, t + Duration::from_secs(30));
        u.push(k, Why::Auto, t);
        assert_eq!(u.next(t + Duration::from_secs(149)), None);
        assert_eq!(u.next(t + Duration::from_secs(150)), Some(k));
    }

    #[test]
    fn the_user_asking_skips_the_wait() {
        let t = Instant::now();
        let mut u = Uploads::default();
        let k = Key::Entry(1);
        u.push(Key::Entry(2), Why::Auto, t);
        u.push(k, Why::Auto, t);
        u.next(t);
        u.finished(Key::Entry(2), Outcome::Sent, t);
        u.next(t);
        u.finished(k, Outcome::Unreachable, t);
        u.push(k, Why::Auto, t);
        assert_eq!(u.push(k, Why::User, t), Pushed::Already);
        assert_eq!(u.next(t), Some(k), "Send now moves it to the front without its wait");
    }
}
