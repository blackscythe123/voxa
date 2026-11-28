//! Home's greeting through the Android-facing API: the ledger in the data folder, once-a-day play.

use voxa_ffi::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("voxa-ffi-greet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

#[test]
fn greeting_plays_once_per_doodle_per_day() {
    let core = VoxaCore::new(tmp("play"));
    let g = core.home_greeting("2026-12-02".into(), 13 * 60, 0, Some("Sam".into()), 0, 0, None, false);
    assert_eq!(g.doodle, "midday");
    assert!(g.lead == "Good afternoon" || g.lead == "Hey there", "{}", g.lead);
    assert_eq!(g.name.as_deref(), Some("Sam"));
    assert!(g.play);
    assert!(g.strokes.len() > 10 && g.done_ms <= 1400);
    core.doodle_played(g.doodle.clone(), g.date.clone());
    let again = core.home_greeting("2026-12-02".into(), 14 * 60, 0, Some("Sam".into()), 0, 0, None, false);
    assert!(!again.play, "the second visit shows it finished");
    assert_eq!(again.strokes, g.strokes, "the same drawing all day");
    let evening = core.home_greeting("2026-12-02".into(), 17 * 60, 0, None, 0, 0, None, false);
    assert_eq!(evening.doodle, "evening");
    assert!(evening.play, "a new doodle plays once");
    assert_eq!(evening.name, None);
}

#[test]
fn birthday_wins_and_is_optional() {
    let core = VoxaCore::new(tmp("bday"));
    let g = core.home_greeting("2027-03-14".into(), 8 * 60 + 40, 0, Some("Sam".into()), 3, 14, None, false);
    assert_eq!((g.lead.as_str(), g.doodle.as_str()), ("Happy birthday", "birthday"));
    let g = core.home_greeting("2027-03-14".into(), 8 * 60 + 40, 0, Some("Sam".into()), 0, 0, None, false);
    assert_ne!(g.doodle, "birthday");
}

#[test]
fn splash_and_flourish_are_reachable() {
    let spec = splash_spec();
    assert!(spec.wordmark.starts_with("M16 38") && spec.total_ms == 2420.0);
    assert_eq!(spec.tagline, "Talk instead of typing");
    let end = splash_end_frame();
    assert_eq!((end.ink, end.nib_alpha), (1.0, 0.0));
    assert!(splash_frame(1000.0).nib_alpha > 0.9);
    let core = VoxaCore::new(tmp("flourish"));
    let g = core.home_greeting("2026-12-02".into(), 21 * 60, 0, None, 0, 0, None, false);
    let star = g.strokes.iter().find(|s| s.tag == StrokeTag::Twinkle).expect("the night has a twinkling star").clone();
    assert!(doodle_flourish(g.doodle.clone(), star.clone(), g.scale, 1200.0).is_some());
    assert!(doodle_flourish(g.doodle, star, g.scale, 500.0).is_none());
}
