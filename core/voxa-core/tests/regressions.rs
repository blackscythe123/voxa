//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use std::collections::{BTreeMap, BTreeSet};

use voxa_core::flow::Delivery;
use voxa_core::*;

fn t(app: &str, w: u64) -> Target {
    Target { app: app.into(), window: w }
}
fn ready() -> Readiness {
    Readiness { session: Session::Valid, network: Network::Up, mic: Mic::Granted }
}
fn start(f: &mut Flow) -> JobId {
    f.handle(Event::Hotkey { target: t("WhatsApp", 1) });
    match f.state() {
        State::Recording { job, .. } => *job,
        s => panic!("expected Recording, got {s:?}"),
    }
}

/// on_delivered never checks that the job is the one being delivered: a late or repeated
/// Delivered answer for an old job replaces the state of a take being recorded now. The new take
/// is then stuck in Recording (Stop and Cancel no longer see it) while the mic keeps running.
#[test]
fn a_late_delivered_answer_does_not_hijack_a_new_recording() {
    let mut f = Flow::new(ready());
    let a = start(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Ok("first".into()) });
    f.handle(Event::Delivered { job: a, result: Delivery::Pasted });
    f.handle(Event::Dismiss);
    let b = start(&mut f);
    // The keyboard reports the old paste a second time (or late, after a field change).
    f.handle(Event::Delivered { job: a, result: Delivery::TargetChanged(t("Slack", 2)) });
    assert!(matches!(f.state(), State::Recording { job, .. } if *job == b), "new take hijacked: {:?}", f.state());
    let fx = f.handle(Event::Stop);
    assert!(fx.contains(&Effect::StopCapture { job: b }), "Stop must still stop the live take; effects {fx:?}");
}

/// Cancelling a "Retry now" of an offline take puts it back to WaitingForNetwork, but the network
/// is already Up, so nothing ever sends it until the network goes down and up again (and on
/// Android the background job skips it because the keyboard "owns" WaitingForNetwork jobs).
#[test]
fn a_cancelled_retry_of_an_offline_take_is_not_left_waiting_while_online() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let j = start(&mut f);
    f.handle(Event::Stop);
    assert_eq!(f.job(j).unwrap().state, JobState::WaitingForNetwork);
    f.handle(Event::Readiness(ready())); // back online: resent
    f.handle(Event::Cancel); // user cancels the resend
    f.handle(Event::Transcribed { job: j, result: Err(TranscribeError::Timeout) }); // old upload ends
    // Network stays up; the shell repeats the same readiness.
    let fx = f.handle(Event::Readiness(ready()));
    let sent = fx.contains(&Effect::Transcribe { job: j });
    assert!(
        sent || f.job(j).unwrap().state != JobState::WaitingForNetwork,
        "job {j:?} says WaitingForNetwork while the network is Up, and nothing will ever send it"
    );
}

/// "The audio is kept and is sent after sign-in": only the one on screen is. A SignInToSend take
/// the user dismissed stays Failed(SessionExpired) after signing in, while offline takes are all
/// resent when the network returns.
#[test]
fn a_dismissed_sign_in_to_send_take_is_sent_after_signing_in() {
    let mut f = Flow::new(ready());
    let j = start(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: j, result: Err(TranscribeError::SessionExpired) });
    f.handle(Event::Dismiss);
    let fx = f.handle(Event::Readiness(ready()));
    assert!(fx.contains(&Effect::Transcribe { job: j }), "signed back in, but the kept take is never sent: {fx:?}");
}

// ── property / fuzz test ─────────────────────────────────────────────────────

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A model shell that only ever answers what the core asked for (a well-behaved keyboard):
/// each Transcribe gets exactly one Transcribed, each Paste/PasteHere exactly one Delivered.
/// `stale_deliveries`: also replay an old Delivered sometimes (a duplicate callback).
fn fuzz(seed: u64, steps: usize, stale_deliveries: bool) -> Result<(), String> {
    let mut rng = XorShift(seed | 1);
    let mut f = Flow::new(ready());
    let mut in_flight: BTreeSet<JobId> = BTreeSet::new();
    let mut pastes: BTreeSet<JobId> = BTreeSet::new();
    let mut delivered_before: Vec<JobId> = Vec::new();
    let mut cancelled_fresh: BTreeSet<JobId> = BTreeSet::new();
    let mut readiness = ready();
    let mut trace: Vec<String> = Vec::new();

    let mut apply = |f: &mut Flow, ev: Event, in_flight: &mut BTreeSet<JobId>, pastes: &mut BTreeSet<JobId>, trace: &mut Vec<String>, cancelled: &BTreeSet<JobId>| -> Result<(), String> {
        trace.push(format!("{ev:?}"));
        let fx = f.handle(ev);
        for e in &fx {
            match e {
                Effect::Transcribe { job } => {
                    if !in_flight.insert(*job) {
                        return Err(format!("second Transcribe for {job:?} while one is in flight"));
                    }
                }
                Effect::Paste { job, .. } | Effect::PasteHere { job, .. } => {
                    if cancelled.contains(job) {
                        return Err(format!("Paste after Cancel for {job:?}"));
                    }
                    pastes.insert(*job);
                }
                _ => {}
            }
        }
        // State/job consistency.
        match f.state().clone() {
            State::Recording { job, .. } if f.job(job).map(|j| &j.state) != Some(&JobState::Recording) => {
                return Err(format!("state Recording({job:?}) but job is {:?}", f.job(job).map(|j| &j.state)));
            }
            State::Transcribing { job } if f.job(job).map(|j| &j.state) != Some(&JobState::Transcribing) => {
                return Err(format!("state Transcribing({job:?}) but job is {:?}", f.job(job).map(|j| &j.state)));
            }
            _ => {}
        }
        // A job in Recording must be the one on screen (otherwise no Stop can ever reach it).
        for j in f.jobs() {
            if j.state == JobState::Recording && !matches!(f.state(), State::Recording { job, .. } if *job == j.id) {
                return Err(format!("job {:?} is Recording but the screen shows {:?}: orphaned live mic", j.id, f.state()));
            }
        }
        Ok(())
    };

    for _ in 0..steps {
        let ev = match rng.below(16) {
            0 | 1 => Event::Hotkey { target: t("A", rng.below(3)) },
            2 => Event::Stop,
            3 => {
                if let State::Recording { job, .. } | State::Transcribing { job } = f.state().clone() {
                    if f.job(job).is_some_and(|j| j.state == JobState::Recording) || !f.jobs().any(|_| false) {
                        // fresh take cancelled while recording, or a transcribing one
                        if f.job(job).is_some_and(|j| matches!(j.state, JobState::Recording)) {
                            cancelled_fresh.insert(job);
                        }
                    }
                }
                Event::Cancel
            }
            4 => Event::Pause,
            5 => Event::Resume,
            6 => Event::Action,
            7 => Event::Dismiss,
            8 | 9 => {
                let Some(&job) = in_flight.iter().nth(rng.below(in_flight.len().max(1) as u64) as usize) else { continue };
                in_flight.remove(&job);
                let result = match rng.below(6) {
                    0 => Ok("words".into()),
                    1 => Ok(String::new()),
                    2 => Err(TranscribeError::Offline),
                    3 => Err(TranscribeError::Timeout),
                    4 => Err(TranscribeError::SessionExpired),
                    _ => Ok("more words".into()),
                };
                Event::Transcribed { job, result }
            }
            10 => {
                let Some(&job) = pastes.iter().next() else { continue };
                pastes.remove(&job);
                delivered_before.push(job);
                let result = match rng.below(3) {
                    0 => Delivery::Pasted,
                    1 => Delivery::TargetChanged(t("B", 9)),
                    _ => Delivery::Failed,
                };
                Event::Delivered { job, result }
            }
            11 => {
                readiness.network = if rng.below(2) == 0 { Network::Up } else { Network::Down };
                Event::Readiness(readiness)
            }
            12 => {
                readiness.session = match rng.below(3) {
                    0 => Session::Valid,
                    1 => Session::Expired,
                    _ => Session::Unknown,
                };
                Event::Readiness(readiness)
            }
            13 => {
                let n = f.jobs().count() as u64;
                if n == 0 {
                    continue;
                }
                Event::Retry { job: JobId(1 + rng.below(n)) }
            }
            14 => Event::Focus { target: t("A", rng.below(3)) },
            _ => {
                if !stale_deliveries || delivered_before.is_empty() {
                    continue;
                }
                let job = delivered_before[rng.below(delivered_before.len() as u64) as usize];
                Event::Delivered { job, result: Delivery::Failed }
            }
        };
        apply(&mut f, ev, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed}: {e}\nlast events: {:?}", &trace[trace.len().saturating_sub(8)..]))?;
    }

    // Drain like a patient shell: answer everything, the network and session come back, the user
    // stops whatever is recording, then retries what is left until nothing changes.
    readiness = ready();
    apply(&mut f, Event::Readiness(Readiness { network: Network::Down, ..readiness }), &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).ok();
    for _ in 0..50 {
        apply(&mut f, Event::Stop, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
        while let Some(&job) = in_flight.iter().next() {
            in_flight.remove(&job);
            apply(&mut f, Event::Transcribed { job, result: Ok("done".into()) }, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
        }
        while let Some(&job) = pastes.iter().next() {
            pastes.remove(&job);
            apply(&mut f, Event::Delivered { job, result: Delivery::Pasted }, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
        }
        apply(&mut f, Event::Dismiss, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).ok();
        apply(&mut f, Event::Readiness(readiness), &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).ok();
        let ids: Vec<JobId> = f.jobs().filter(|j| matches!(j.state, JobState::WaitingForNetwork | JobState::Failed(_))).map(|j| j.id).collect();
        for id in ids {
            apply(&mut f, Event::Retry { job: id }, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
            while let Some(&job) = in_flight.iter().next() {
                in_flight.remove(&job);
                apply(&mut f, Event::Transcribed { job, result: Ok("done".into()) }, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
            }
            while let Some(&job) = pastes.iter().next() {
                pastes.remove(&job);
                apply(&mut f, Event::Delivered { job, result: Delivery::Pasted }, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).map_err(|e| format!("seed {seed} drain: {e}"))?;
            }
            apply(&mut f, Event::Dismiss, &mut in_flight, &mut pastes, &mut trace, &cancelled_fresh).ok();
        }
    }
    let stuck: BTreeMap<JobId, JobState> = f
        .jobs()
        .filter(|j| matches!(j.state, JobState::Recording | JobState::Transcribing | JobState::WaitingForNetwork))
        .map(|j| (j.id, j.state.clone()))
        .collect();
    if !stuck.is_empty() {
        return Err(format!("seed {seed}: jobs never reached a terminal state: {stuck:?}, screen {:?}, uploading {:?}", f.state(), stuck.keys().filter(|j| f.is_uploading(**j)).collect::<Vec<_>>()));
    }
    Ok(())
}

/// Well-behaved shell (each request answered exactly once). Invariants: never two Transcribe in
/// flight for one job, no Paste after a fresh take was cancelled, state agrees with its job, no
/// orphaned Recording job, and every job reaches a terminal state once the shell drains.
#[test]
fn fuzz_well_behaved_shell_keeps_every_invariant() {
    let mut failures = Vec::new();
    for seed in 1..=3000u64 {
        if let Err(e) = fuzz(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 120, false) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} of 3000 seeds broke an invariant; first:\n{}", failures.len(), failures[0]);
}

/// Same, with an occasional repeated Delivered callback (a duplicate answer from the shell).
#[test]
fn fuzz_duplicate_delivered_callbacks_never_break_the_flow() {
    let mut failures = Vec::new();
    for seed in 1..=3000u64 {
        if let Err(e) = fuzz(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 120, true) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{} of 3000 seeds broke an invariant; first:\n{}", failures.len(), failures[0]);
}

// ── notices: clock going backwards ───────────────────────────────────────────

/// A post time in the future (the phone clock was ahead, then corrected) counts as "recent"
/// until the clock catches up, and `record` keeps it forever meanwhile: two such stamps silence
/// every notification (including "You're signed out") for as long as the clock was wrong.
#[test]
fn future_post_times_do_not_silence_notifications() {
    let now = 1_800_000_000_000u64;
    let year = 365 * 24 * 3_600_000u64;
    let posted = [now + year, now + year];
    assert_eq!(notices::decide(notices::Kind::SignIn, now, false, &posted, 0), notices::Decision::Post);
}

#[test]
fn a_future_last_dictations_time_does_not_block_for_months() {
    let now = 1_800_000_000_000u64;
    let month = 30 * 24 * 3_600_000u64;
    assert_eq!(notices::decide(notices::Kind::Dictations, now, false, &[], now + month), notices::Decision::Post);
}

// ── text: UTF-8 boundary panic from transcript text ──────────────────────────

/// find_phrase lowercases the text and, when the byte length is unchanged, uses offsets from the
/// lowercased copy to slice the original. "ẞ" shrinks by one byte and "İ" grows by one, so the
/// total is equal but the offsets in between are shifted: slicing lands inside "é" and panics.
/// Reached from every transcript through the word list (`Words::apply`) and spoken formatting.
#[test]
fn replace_phrases_never_panics_on_unicode_case_changes() {
    let r = std::panic::catch_unwind(|| text::replace_phrases("ẞéa İ", &[("a".to_string(), "b".to_string())]));
    assert!(r.is_ok(), "replace_phrases panicked on a German/Turkish transcript");
}

#[test]
fn spoken_formatting_never_panics_on_unicode_case_changes() {
    let r = std::panic::catch_unwind(|| text::spoken_formatting("ẞénew line İ"));
    assert!(r.is_ok(), "spoken_formatting panicked on a German/Turkish transcript");
}
