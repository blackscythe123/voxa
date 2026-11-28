//! Home's greeting on the desktop: the shared rules in voxa-core's `greeting`, fed from this
//! data folder (History and a small ledger file), plus the system's reduced motion setting.

use chrono::{Datelike, Local, NaiveDateTime, Timelike};
use voxa_core::doodle::{self, Drawing};
use voxa_core::greeting::{self, Date, Greeting, Ledger};

use crate::store::{Entry, Status};

const LEDGER_FILE: &str = "greeting.txt";

/// Only with a separate VOXA_DATA folder (tests).
fn test_env(name: &str) -> Option<String> {
    crate::test_hooks().then_some(())?;
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// The local time Home greets by. Test only: VOXA_TEST_NOW=2026-12-03T13:05 (with VOXA_DATA).
pub fn now() -> NaiveDateTime {
    test_env("VOXA_TEST_NOW").and_then(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M").ok()).unwrap_or_else(|| Local::now().naive_local())
}

/// Test only (with VOXA_DATA): hold the splash or the doodle at this many milliseconds.
pub fn test_freeze(name: &str) -> Option<f32> {
    test_env(name).and_then(|s| s.parse().ok())
}

fn date_of(d: chrono::NaiveDate) -> Date {
    Date::new(d.year(), d.month() as u8, d.day() as u8)
}

fn words_per_day(entries: &[Entry]) -> Vec<(Date, u64)> {
    let mut days: Vec<(Date, u64)> = Vec::new();
    for e in entries.iter().filter(|e| matches!(e.status, Status::Pasted | Status::Ready)) {
        let w = e.words() as u64;
        if w == 0 {
            continue;
        }
        let d = date_of(e.local_time().date_naive());
        match days.iter_mut().find(|x| x.0 == d) {
            Some(x) => x.1 += w,
            None => days.push((d, w)),
        }
    }
    days
}

/// Today's greeting and whether its doodle should play (once per doodle per day).
pub struct Header {
    pub greeting: Greeting,
    pub play: bool,
}

pub fn header(entries: &[Entry], name: Option<String>, birthday: Option<greeting::MonthDay>) -> Header {
    let dir = crate::data_dir();
    let now = now();
    let today = date_of(now.date());
    let path = dir.join(LEDGER_FILE);
    let mut ledger = Ledger::parse(&std::fs::read_to_string(&path).unwrap_or_default());
    ledger.merge(today, &words_per_day(entries));
    // When the data folder was made: the closest thing to an install date.
    let installed = std::fs::metadata(&dir).and_then(|m| m.created()).ok().map(|t| date_of(chrono::DateTime::<Local>::from(t).date_naive()));
    let g = greeting::greet(&greeting::Inputs {
        today,
        minutes: (now.hour() * 60 + now.minute()) as u16,
        name,
        birthday,
        installed,
        first_open: false,
        usage: ledger.usage(),
    });
    ledger.mark_shown(&g);
    let play = ledger.played.as_ref() != Some(&(g.doodle.key().to_string(), today));
    let _ = crate::store::write_atomic(&path, ledger.to_text().as_bytes());
    Header { greeting: g, play }
}

/// The doodle played today: later opens show it finished.
pub fn played(g: &Greeting) {
    let path = crate::data_dir().join(LEDGER_FILE);
    let mut ledger = Ledger::parse(&std::fs::read_to_string(&path).unwrap_or_default());
    if ledger.should_play(g.doodle, g.date) {
        let _ = crate::store::write_atomic(&path, ledger.to_text().as_bytes());
    }
}

/// The drawing for a band `width` wide: the subject on the right, the horizon edge to edge.
pub fn drawing(g: &Greeting, width: f32) -> Drawing {
    let dx = (width - doodle::BAND_W - 24.0).max(0.0);
    doodle::draw(g.doodle, g.date, g.tally, width.max(doodle::BAND_W), dx)
}

/// Windows "Animation effects" off, or macOS "Reduce motion" on. Test only: VOXA_TEST_REDUCED_MOTION=1.
pub fn reduced_motion() -> bool {
    if let Some(v) = test_env("VOXA_TEST_REDUCED_MOTION") {
        return v == "1";
    }
    os_reduced_motion()
}

#[cfg(windows)]
fn os_reduced_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};
    let mut on: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer we pass.
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut on as *mut i32 as *mut core::ffi::c_void, 0) };
    ok != 0 && on == 0
}

#[cfg(target_os = "macos")]
fn os_reduced_motion() -> bool {
    use objc::{class, msg_send, sel, sel_impl};
    // SAFETY: NSWorkspace.sharedWorkspace always exists in an app; the property is a BOOL.
    unsafe {
        let ws: *mut objc::runtime::Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let r: objc::runtime::BOOL = msg_send![ws, accessibilityDisplayShouldReduceMotion];
        r != objc::runtime::NO
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn os_reduced_motion() -> bool {
    false
}

#[cfg(test)]
mod tests {
    /// The greeting ledger is written atomically (a unique temp file, then replace_file), never a
    /// bare fs::write that a crash or a second process could leave half written.
    #[test]
    fn the_greeting_ledger_is_written_atomically() {
        let src = include_str!("greet.rs");
        let code = &src[..src.find("#[cfg(test)]").unwrap()];
        assert!(!code.contains(concat!("std::fs::", "write(")));
        assert_eq!(code.matches("crate::store::write_atomic(&path").count(), 2);
    }
}
