//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use std::path::PathBuf;
use voxa_store::{Entry, History, Status};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("voxa-kb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entry(id: u64, text: &str) -> Entry {
    Entry { id, app: "ZZ test".into(), text: text.into(), secs: 3.0, status: Status::Pasted, audio: None, started_in: None, via: None }
}

fn backups(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains("broken")).count()
}

/// A history.json that can't be parsed (power cut mid-write, a zero-filled file, a newer app's
/// format) must not be silently replaced by the next take. Today History::load returns an empty
/// list on any error and History::edit saves that list plus the new entry over the file.
#[test]
fn damaged_history_is_not_overwritten_by_the_next_take() {
    let dir = tmp("damaged");
    let mut h = History::load(&dir);
    for i in 0..50 {
        h.upsert(entry(1_700_000_000_000 + i, "kept dictation"));
    }
    h.save().unwrap();
    // A torn write: the tail of the file is lost.
    let bytes = std::fs::read(dir.join("history.json")).unwrap();
    std::fs::write(dir.join("history.json"), &bytes[..bytes.len() / 2]).unwrap();

    // The next take writes its entry, as voxa-ffi's flow does.
    History::edit(&dir, |h| (h.upsert(entry(1_800_000_000_000, "new")), ()));

    let after = History::load(&dir);
    let old_left = after.entries.iter().filter(|e| e.text == "kept dictation").count();
    assert!(old_left > 0 || backups(&dir) > 0, "all 50 earlier dictations are gone and no copy of the damaged file was kept");
}

/// Zero-byte file (the classic result of rename-without-fsync after a crash).
#[test]
fn zero_byte_history_is_not_overwritten_by_the_next_take() {
    let dir = tmp("zero");
    let mut h = History::load(&dir);
    h.upsert(entry(1_700_000_000_000, "kept dictation"));
    h.save().unwrap();
    std::fs::write(dir.join("history.json"), b"").unwrap();
    History::edit(&dir, |h| (h.upsert(entry(1_800_000_000_000, "new")), ()));
    assert!(backups(&dir) > 0, "zero-byte history silently restarted from empty and was saved over, no backup kept");
}

/// A clock that jumps forward (dead CMOS battery, a bad network time, a user setting the date
/// ahead) must not delete pasted history. prune() trusts now_ms blindly.
#[test]
fn a_clock_jump_forward_does_not_delete_history() {
    let dir = tmp("clock");
    let real_now = 1_764_300_000_000u64;
    let mut h = History::load(&dir);
    for i in 0..10u64 {
        h.upsert(entry(real_now - i * 3_600_000, "today"));
    }
    let wrong_now = real_now + 40 * 86_400_000; // clock 40 days fast
    let dropped = h.prune(30, wrong_now);
    assert_eq!(dropped, 0, "a clock 40 days fast deleted {dropped} of today's dictations");
}

/// Cost of the whole-file read-modify-write every History::edit does (every take state change,
/// every keyboard show), at 10k entries. Prints numbers; fails above a generous 150 ms.
#[test]
fn ten_thousand_entries_edit_cost() {
    let dir = tmp("big");
    let mut h = History::load(&dir);
    let text = "This is a fairly ordinary dictation of about forty words that someone might say into a chat box or an email while working through their day, nothing special at all, just enough text to look like a real entry.";
    for i in 0..10_000u64 {
        h.upsert(entry(1_700_000_000_000 + i * 60_000, text));
    }
    h.save().unwrap();
    let size = std::fs::metadata(dir.join("history.json")).unwrap().len();
    let t = std::time::Instant::now();
    History::edit(&dir, |h| (h.upsert(entry(1_900_000_000_000, "new")), ()));
    let edit = t.elapsed();
    let t = std::time::Instant::now();
    let _ = History::edit(&dir, |h| (false, h.entries.len()));
    let read = t.elapsed();
    eprintln!("history.json {:.1} MB; one edit {edit:?}; one read-only edit {read:?}", size as f64 / 1e6);
    assert!(edit.as_millis() < 150, "one edit took {edit:?}");
}
