//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use voxa_nudge::engine::{Engine, Usage};
use voxa_nudge::features::{DAY_MS, HOUR_MS};
use voxa_nudge::rng::Rng;
use voxa_nudge::rules::{self, Block};
use voxa_nudge::state::{NudgeState, Outcome};
use voxa_nudge::{Skip, Style};

/// 2025-12-01 00:00 UTC.
const DAY0: u64 = 20_423 * DAY_MS;

/// Runs the app's loop for `days`: ask next_check_ms, then decide at that time. Returns how many
/// reminders went out.
fn run_app_loop(e: &mut Engine, start: u64, usage: &Usage, days: usize) -> usize {
    let mut rng = Rng::new(11);
    let mut now = start;
    let mut sent = 0;
    for _ in 0..days {
        let at = e.next_check_ms(now, 0, usage);
        match e.decide(at, 0, usage, &mut rng) {
            Ok(_) => sent += 1,
            Err(why) => eprintln!("check at {:.2}h local: {why:?}", (at % DAY_MS) as f64 / HOUR_MS as f64),
        }
        now = at;
    }
    sent
}

#[test]
fn a_night_owl_gets_a_reminder_inside_allowed_hours() {
    // Dictated at 23:00 every day for two weeks, the last one 5 days ago. Long inactive: a
    // reminder is due. The app checks at next_check_ms (clamped to 20:30); decide used to send
    // only within 1.5 h of 23:00, so every check said ModelWaits, forever.
    let now = DAY0 + 30 * DAY_MS + 12 * HOUR_MS;
    let owl = Usage {
        dictations_ms: (5..19).map(|d| now + 11 * HOUR_MS - d * DAY_MS).collect(),
        saved_secs_month: 0,
    };
    let mut e = Engine::in_memory(NudgeState::default());
    let sent = run_app_loop(&mut e, now, &owl, 14);
    assert!(sent >= 1, "a night owl inactive for 5..19 days got {sent} reminders in 14 daily checks");
}

#[test]
fn an_early_bird_gets_a_reminder_inside_allowed_hours() {
    // Usual hour 07:00: checks are clamped to 09:30, 2.5 h away, so it was never inside the window.
    let now = DAY0 + 30 * DAY_MS + 12 * HOUR_MS;
    let bird = Usage {
        dictations_ms: (5..19).map(|d| now - 5 * HOUR_MS - d * DAY_MS).collect(),
        saved_secs_month: 0,
    };
    let mut e = Engine::in_memory(NudgeState::default());
    let sent = run_app_loop(&mut e, now, &bird, 14);
    assert!(sent >= 1, "an early bird inactive for 5..19 days got {sent} reminders in 14 daily checks");
}

#[test]
fn a_reminder_stamped_far_in_the_future_does_not_block_reminders_until_that_date() {
    // The phone clock was a year ahead when a reminder went out, then corrected.
    let real_now = DAY0 + 100 * DAY_MS + 12 * HOUR_MS;
    let mut s = NudgeState::default();
    s.record_send(real_now + 365 * DAY_MS, Style::Tip, Some(3));
    // Ten days of real time later, with the user long inactive:
    let now = real_now + 10 * DAY_MS;
    s.observe(now, Some(real_now - 30 * DAY_MS));
    assert_ne!(
        rules::check(now, 12.0, Some(real_now - 30 * DAY_MS), &s),
        Err(Block::TooSoon),
        "a future-stamped send blocks reminders for a year"
    );
}

#[test]
fn a_reminder_stamped_in_the_future_still_settles() {
    let real_now = DAY0 + 100 * DAY_MS + 12 * HOUR_MS;
    let mut s = NudgeState::default();
    s.record_send(real_now + 365 * DAY_MS, Style::Tip, Some(3));
    s.observe(real_now + 10 * DAY_MS, Some(real_now - 30 * DAY_MS));
    assert!(s.pending.is_none(), "a reminder 'sent' in the future never settles");
}

#[test]
fn a_dictation_stamped_slightly_in_the_future_counts_as_recent() {
    // The clock was 2 h ahead when the user dictated, then NTP put it back: the dictation looks
    // 2 h in the future. It is in fact a dictation from minutes ago, so the 48 h rule must hold.
    let now = DAY0 + 50 * DAY_MS + 19 * HOUR_MS; // 19:00, the default usual hour
    let usage = Usage { dictations_ms: vec![now + 2 * HOUR_MS], saved_secs_month: 0 };
    let mut e = Engine::in_memory(NudgeState::default());
    let r = e.decide(now, 0, &usage, &mut Rng::new(1));
    assert_eq!(r.map(|n| n.style), Err(Skip::Rule(Block::RecentlyActive)), "reminded right after dictating");
}

#[test]
fn a_file_saying_on_with_one_bad_field_keeps_the_spacing_and_quiet_promise() {
    // A version-1 file that says "on", with every rule field readable but one bandit field
    // damaged. The fallback used to keep "enabled" and throw away sent_ms, quiet_paused and
    // ignored_streak, so the 3 day gap, the quiet promise and the ignored-twice stop all went.
    let sent = DAY0 + 100 * DAY_MS + 12 * HOUR_MS;
    let json = format!(
        r#"{{"version":1,"enabled":true,"sent_ms":[{sent}],"quiet_paused":true,"ignored_streak":2,"bandit":{{"style":"oops"}}}}"#
    );
    let s = NudgeState::from_json(&json);
    assert!(s.enabled);
    let now = sent + DAY_MS; // one day after the last reminder
    assert_ne!(rules::check(now, 12.0, Some(sent - 20 * DAY_MS), &s), Ok(()), "a reminder one day after the last one");
}

#[test]
fn a_newer_version_file_saying_on_keeps_the_quiet_promise() {
    let sent = DAY0 + 100 * DAY_MS + 12 * HOUR_MS;
    let json = format!(r#"{{"version":2,"enabled":true,"sent_ms":[{sent}],"quiet_paused":true}}"#);
    let s = NudgeState::from_json(&json);
    let now = sent + 5 * DAY_MS;
    assert_eq!(
        rules::check(now, 12.0, Some(sent - 20 * DAY_MS), &s),
        Err(Block::PromisedQuiet),
        "the 'We'll stay quiet' promise is broken after a downgrade"
    );
}

#[test]
fn coming_back_within_24h_counts_as_opened_even_with_a_later_dictation() {
    // Reminder at 19:00. The user dictates at 20:00 (came back) and again the next day at 19:05.
    // The next check runs at 19:10 the next day: only the LAST dictation used to be looked at, it
    // is outside the 24 h window, so the reminder was booked as Ignored.
    let sent = DAY0 + 50 * DAY_MS + 19 * HOUR_MS;
    let mut st = NudgeState::default();
    st.record_send(sent, Style::Tip, Some(3));
    let mut e = Engine::in_memory(st);
    let usage = Usage {
        dictations_ms: vec![sent + HOUR_MS, sent + DAY_MS + 5 * 60_000],
        saved_secs_month: 0,
    };
    let _ = e.decide(sent + DAY_MS + 10 * 60_000, 0, &usage, &mut Rng::new(2));
    assert_eq!(e.state.last_outcome, Some(Outcome::Opened), "bandit style counts {:?}", e.state.bandit.style);
}

#[test]
fn the_ignored_streak_never_wraps_back_to_zero() {
    // A file with ignored_streak at u32::MAX: one more ignore used to wrap it to 0 in release
    // builds (and panic in debug builds), so the ignored-twice stop lifted.
    let mut s = NudgeState::from_json(r#"{"ignored_streak": 4294967295}"#);
    let t = DAY0 + 10 * DAY_MS + 12 * HOUR_MS;
    s.record_send(t, Style::Plain, None);
    s.record_outcome(Outcome::Ignored);
    let now = t + 5 * DAY_MS;
    assert_eq!(rules::check(now, 12.0, Some(t - 20 * DAY_MS), &s), Err(Block::IgnoredTwice));
}

/// A transient read failure (here: another handle holding the file with no sharing) makes
/// Engine::open start with reminders OFF and an empty history; the next decide() used to save
/// that over the good file, switching reminders the user had on off for good.
#[cfg(windows)]
#[test]
fn a_transient_read_error_never_overwrites_the_saved_state() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = std::env::temp_dir().join(format!("voxa-nudge-reg-ioerr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("nudge.json");
    std::fs::write(&file, br#"{"version":1,"enabled":true}"#).unwrap();
    let mut e = {
        let _held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
        Engine::open(&dir)
    };
    assert!(std::fs::read_to_string(&file).unwrap().contains("true"), "the file itself is fine once released");
    let now = DAY0 + 50 * DAY_MS + 19 * HOUR_MS;
    let _ = e.decide(now, 0, &Usage::default(), &mut Rng::new(3));
    let enabled_after = Engine::open(&dir).state.enabled;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(enabled_after, "one failed read rewrote nudge.json with reminders off");
}

/// A fresh, empty folder for one test.
fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("voxa-nudge-reg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_reminder_that_cannot_be_saved_is_not_sent() {
    // The folder disappears after the state was read (here: a plain file takes its place), so
    // nudge.json can't be written. Sending anyway would forget the reminder went out, and the
    // next day's check would send again: one a day instead of one every 3 days.
    let dir = test_dir("unsaved");
    let mut e = Engine::open(&dir);
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&dir, b"not a folder").unwrap();
    assert!(e.save().is_err(), "a failed save is reported");
    let now = DAY0 + 50 * DAY_MS + 19 * HOUR_MS; // 19:00, the default usual hour
    let usage = Usage { dictations_ms: vec![now - 10 * DAY_MS], saved_secs_month: 0 };
    let r = e.decide(now, 0, &usage, &mut Rng::new(4));
    let _ = std::fs::remove_file(&dir);
    assert_eq!(r.map(|n| n.style), Err(Skip::NotSaved));
    assert!(e.state.sent_ms.is_empty() && e.state.pending.is_none(), "an unsent reminder is not remembered as sent");
}

#[test]
fn a_leftover_temp_file_does_not_stop_saving() {
    // Something stuck at the old fixed temp name (here: a folder) made every save fail silently.
    let dir = test_dir("tmpname");
    std::fs::create_dir_all(dir.join("nudge.json.tmp")).unwrap();
    let mut e = Engine::open(&dir);
    e.set_enabled(false);
    let after = Engine::open(&dir).state.enabled;
    let leftovers = std::fs::read_dir(&dir).unwrap().flatten().filter(|f| f.path().is_file()).count();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!after, "turning reminders off was not saved");
    assert_eq!(leftovers, 1, "only nudge.json is left, no temp files");
}
