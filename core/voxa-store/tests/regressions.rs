//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use voxa_store::{Entry, History, Status};

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("voxa-store-reg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn entry(id: u64, text: &str) -> Entry {
    Entry { id, app: "A".into(), text: text.into(), secs: 1.0, status: Status::Pasted, audio: None, started_in: None, via: None }
}

/// A history.json that doesn't parse (a torn write: `write_atomic` never fsyncs before the
/// rename, so a power loss can leave an empty or partial file) loads as an empty History, and the
/// next edit of any kind saves that over it. Every earlier dictation is gone, with no backup.
#[test]
fn a_damaged_history_file_is_not_overwritten_by_the_next_edit() {
    let dir = tmp("corrupt");
    History::edit(&dir, |h| (h.upsert(entry(1, "one")) | h.upsert(entry(2, "two")), ()));
    let good = std::fs::read(dir.join("history.json")).unwrap();
    // Torn write: only the first half reached the disk.
    std::fs::write(dir.join("history.json"), &good[..good.len() / 2]).unwrap();
    History::edit(&dir, |h| (h.upsert(entry(3, "three")), ()));
    let backups = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name() != "history.json").count();
    let now = History::load(&dir);
    assert!(
        now.entries.len() >= 3 || backups > 0,
        "the old entries were silently replaced: now {} entries, no copy of the damaged file kept",
        now.entries.len()
    );
}

/// `write_atomic` uses one fixed temp name (`history.tmp`). The in-process HISTORY_LOCK protects
/// History::edit, but Settings/Words saves and any second process (or `History::save` called
/// directly, as `delete_all` does) share the temp file: concurrent writers interleave into it or
/// their renames fail, and the errors are dropped by `save`.
#[test]
fn concurrent_atomic_writes_never_fail_or_mix() {
    let dir = tmp("tmpname");
    let path = dir.join("settings.json");
    let errors = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for i in 0..8 {
            let path = path.clone();
            let errors = &errors;
            s.spawn(move || {
                for j in 0..200 {
                    let body = format!("{{\"n\":\"{}\"}}", "x".repeat(10 + (i * 37 + j) % 300));
                    if voxa_store::write_atomic(&path, body.as_bytes()).is_err() {
                        errors.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });
        }
    });
    let ok = serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).is_ok();
    let errors = errors.into_inner();
    assert!(ok && errors == 0, "file parses: {ok}; {errors} of 1600 writes failed (each a silently lost save)");
}

/// A settings file that can't be parsed gives no settings to decide from (so nobody prunes a
/// "Forever" History at the default 30 days), and a copy of it is kept.
#[test]
fn damaged_settings_are_never_taken_as_the_defaults() {
    let dir = tmp("settings-damaged");
    voxa_store::Settings { keep_days: 0, ..Default::default() }.save(&dir).unwrap();
    std::fs::write(dir.join("settings.json"), b"{\"keep_days\": 0, \"remove_fil").unwrap();
    assert_eq!(voxa_store::Settings::load_checked(&dir), None);
    let copies = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("settings.broken-")).count();
    assert!(copies > 0, "a copy of the damaged settings is kept");
    std::fs::remove_file(dir.join("settings.json")).unwrap();
    assert_eq!(voxa_store::Settings::load_checked(&dir), Some(voxa_store::Settings::default()), "no file yet is fine");
}

/// While History is damaged, nothing is pruned and no recording counts as unlisted, so the
/// entries lost in the torn part keep their audio.
#[test]
fn a_damaged_history_turns_off_pruning_and_orphan_deletion() {
    let dir = tmp("damaged-prune");
    let mut h = History::load(&dir);
    for i in 0..5 {
        let mut e = entry(1_000 + i, "old");
        e.audio = Some(dir.join(format!("{i}.pcm")));
        h.upsert(e);
    }
    h.save().unwrap();
    let bytes = std::fs::read(dir.join("history.json")).unwrap();
    std::fs::write(dir.join("history.json"), &bytes[..bytes.len() * 2 / 3]).unwrap();
    let mut h = History::load(&dir);
    assert_eq!(h.loaded, voxa_store::Loaded::Damaged);
    assert!(!h.entries.is_empty(), "the entries before the cut are salvaged");
    assert_eq!(h.prune(1, u64::MAX / 2), 0, "nothing pruned from a damaged History");
    assert!(!h.may_delete_unlisted_audio("4.pcm"));
    // Saved again (with the copy kept aside), it reads cleanly; the copy still names 4.pcm.
    h.save().unwrap();
    let h = History::load(&dir);
    assert_eq!(h.loaded, voxa_store::Loaded::Clean);
    assert!(!h.may_delete_unlisted_audio("4.pcm"), "recorded before the damage: its entry may be in the lost part");
    assert!(h.may_delete_unlisted_audio("9999999999999-1.pcm"), "recorded after the damage was found");
}
