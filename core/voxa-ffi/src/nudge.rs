//! The "come back" reminder engine (`voxa-nudge`) for the Android app.
//!
//! The app's periodic background job calls [`ReminderEngine::check`] every couple of hours; it
//! returns the reminder to post, or nothing. The hard rules (at most one every 3 days, never 21:00
//! to 09:00, never within 48 hours of a dictation, stop after 2 ignored, the off switch) are
//! enforced here, in Rust, whatever the model says. Notification taps and swipes come back through
//! [`ReminderEngine::record_outcome`]. The state is `nudge.json` in the app's data folder.
//!
//! Wiring is described in docs/reminders.md.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use voxa_nudge as vn;
use voxa_store::History;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReminderStyle {
    Plain,
    Progress,
    Tip,
    Quiet,
}

impl From<vn::Style> for ReminderStyle {
    fn from(s: vn::Style) -> Self {
        match s {
            vn::Style::Plain => Self::Plain,
            vn::Style::Progress => Self::Progress,
            vn::Style::Tip => Self::Tip,
            vn::Style::Quiet => Self::Quiet,
        }
    }
}

/// A reminder to post on the "Reminders" channel.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Reminder {
    pub style: ReminderStyle,
    pub title: String,
    pub body: String,
}

/// What the user did with the reminder. No reaction within 24 hours counts as ignored by itself
/// (and a dictation within 24 hours counts as opened), so the app only reports taps and swipes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReminderOutcome {
    Opened,
    Dismissed,
}

/// The reminder engine. Cheap to create; every call reads and writes `nudge.json`, so the
/// background job and the notification receiver can each make their own.
#[derive(uniffi::Object)]
pub struct ReminderEngine {
    dir: PathBuf,
}

/// One engine call at a time per process (the file is read, changed and written back).
static LOCK: Mutex<()> = Mutex::new(());

/// Days since 1970-01-01 to (year, month): Howard Hinnant's civil-from-days.
fn year_month(days: i64) -> (i64, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m)
}

/// What History knows: when the user dictated (written dictations only) and the seconds saved
/// versus typing this calendar month, local time.
fn usage_from_history(dir: &std::path::Path, now_ms: u64, utc_offset_minutes: i32) -> vn::Usage {
    let h = History::load(dir);
    let offset = utc_offset_minutes as i64 * 60_000;
    let month = |ms: u64| year_month((ms as i64 + offset).div_euclid(86_400_000));
    let this_month = month(now_ms);
    let written = h.entries.iter().filter(|e| !e.status.is_unsent());
    let mut usage = vn::Usage::default();
    for e in written {
        usage.dictations_ms.push(e.id);
        if month(e.id) == this_month {
            usage.saved_secs_month += voxa_store::typing_saved_secs(e.words(), e.secs);
        }
    }
    usage
}

#[uniffi::export]
impl ReminderEngine {
    /// `data_dir`: the app's private files folder (the same one given to `VoxaCore`).
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Arc<Self> {
        Arc::new(Self { dir: PathBuf::from(data_dir) })
    }

    /// Asked by the periodic job (every 2 hours suits the model). `utc_offset_minutes`: the
    /// phone's current offset from UTC. Returns the reminder to post now, or None. A returned
    /// reminder is already counted as sent.
    pub fn check(&self, now_ms: u64, utc_offset_minutes: i32) -> Option<Reminder> {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let usage = usage_from_history(&self.dir, now_ms, utc_offset_minutes);
        let mut engine = vn::Engine::open(&self.dir);
        let mut rng = vn::rng::Rng::new(now_ms ^ engine.state.bandit.trials().to_bits() as u64);
        engine.decide(now_ms, utc_offset_minutes, &usage, &mut rng).ok().map(|n| Reminder {
            style: n.style.into(),
            title: n.title,
            body: n.body,
        })
    }

    /// The user tapped or swiped away the last reminder.
    pub fn record_outcome(&self, outcome: ReminderOutcome) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        vn::Engine::open(&self.dir).record_outcome(match outcome {
            ReminderOutcome::Opened => vn::Outcome::Opened,
            ReminderOutcome::Dismissed => vn::Outcome::Dismissed,
        });
    }

    /// The Settings switch "Reminders" (on by default).
    pub fn enabled(&self) -> bool {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        vn::Engine::open(&self.dir).state.enabled
    }

    pub fn set_enabled(&self, on: bool) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        vn::Engine::open(&self.dir).set_enabled(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxa_store::{Entry, Status};

    const DAY: u64 = 86_400_000;

    #[test]
    fn calendar_months() {
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(year_month(20_423), (2025, 12)); // 2025-12-01
        assert_eq!(year_month(20_454), (2026, 1)); // 2026-01-01
        assert_eq!(year_month(20_453), (2025, 12)); // 2025-12-31
        assert_eq!(year_month(11_016), (2000, 2)); // 2000-02-29
    }

    fn entry(id: u64, status: Status) -> Entry {
        Entry {
            id,
            app: "Notes".into(),
            text: "one two three four five six seven eight nine ten eleven twelve".into(),
            secs: 4.0,
            status,
            audio: None,
            started_in: None,
            via: None,
        }
    }

    #[test]
    fn reminders_follow_history_and_the_switch() {
        let dir = std::env::temp_dir().join(format!("voxa-ffi-reminders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let noon = 20_440 * DAY + 12 * 3_600_000;
        History::edit(&dir, |h| {
            h.upsert(entry(noon - 3_600_000, Status::Pasted));
            h.upsert(entry(noon - 20 * DAY, Status::Failed("offline".into())));
            (true, ())
        });
        let usage = usage_from_history(&dir, noon, 0);
        assert_eq!(usage.dictations_ms, vec![noon - 3_600_000], "unsent takes are not dictations");
        assert!(usage.saved_secs_month > 0);

        let engine = ReminderEngine::new(dir.to_string_lossy().to_string());
        assert!(engine.enabled());
        // Dictated an hour ago: never a reminder, whatever the model thinks.
        assert_eq!(engine.check(noon, 0), None);
        engine.set_enabled(false);
        assert!(!engine.enabled());
        assert_eq!(engine.check(noon + 10 * DAY, 0), None);
        engine.set_enabled(true);
        // Night is always quiet.
        assert_eq!(engine.check(noon + 10 * DAY + 11 * 3_600_000, 0), None);
        // A tap with nothing waiting is harmless.
        engine.record_outcome(ReminderOutcome::Opened);
        assert!(dir.join("nudge.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
