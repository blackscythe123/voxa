use voxa_core::text::remove_fillers;
use voxa_core::trim::{TrimParams, trim};

const RATE: u32 = 16_000;

fn tone(ms: u32, amp: f32) -> Vec<i16> {
    (0..RATE * ms / 1000).map(|i| ((i as f32 * 0.07).sin() * amp * 32767.0) as i16).collect()
}
fn hiss(ms: u32) -> Vec<i16> {
    // quiet room noise, deterministic
    (0..RATE * ms / 1000).map(|i| (((i * 7919) % 61) as i16) - 30).collect()
}

#[test]
fn long_silence_is_cut_and_speech_kept() {
    let mut a = hiss(2000);
    a.extend(tone(1500, 0.3)); // speech 1
    a.extend(hiss(30_000)); // 30 s of dead air
    a.extend(tone(1000, 0.2)); // speech 2
    a.extend(hiss(2000));
    let out = trim(&a, RATE, &TrimParams::default());
    assert!(out.trimmed);
    assert_eq!(out.spans.len(), 2, "two phrases: {:?}", out.spans);
    let secs = out.samples.len() as f32 / RATE as f32;
    // 2.5 s speech + 2 x 2 x 0.2 s padding + 0.1 s spacer (frame rounding allowed)
    assert!((3.0..3.6).contains(&secs), "kept {secs} s");
}

#[test]
fn short_pauses_are_natural_rhythm_and_kept() {
    let mut a = tone(800, 0.3);
    a.extend(hiss(250)); // < 300 ms pause
    a.extend(tone(800, 0.3));
    let out = trim(&a, RATE, &TrimParams::default());
    assert_eq!(out.spans.len(), 1, "a 250 ms pause must not split: {:?}", out.spans);
}

#[test]
fn too_little_speech_sends_the_original() {
    let mut a = hiss(5000);
    a.extend(tone(300, 0.3));
    a.extend(hiss(5000));
    let out = trim(&a, RATE, &TrimParams::default());
    assert!(!out.trimmed);
    assert_eq!(out.samples.len(), a.len());
}

#[test]
fn cuts_are_faded_no_clicks() {
    let mut a = hiss(1000);
    a.extend(tone(1500, 0.8));
    a.extend(hiss(5000));
    a.extend(tone(1500, 0.8));
    let out = trim(&a, RATE, &TrimParams::default());
    assert_eq!(out.samples[0], 0, "starts from silence");
    assert!(out.samples.last().unwrap().abs() < 200, "ends faded");
}

#[test]
fn fillers_removed_and_sentence_fixed() {
    assert_eq!(remove_fillers("Um, I think we should, uh, ship it."), "I think we should, ship it.");
    assert_eq!(remove_fillers("Hmm. Okay, let's go."), "Okay, let's go.");
    assert_eq!(remove_fillers("It works mm-mm-mm fine"), "It works fine");
    assert_eq!(remove_fillers("Ummmm yes"), "Yes");
    assert_eq!(remove_fillers("We met um."), "We met.");
}

#[test]
fn real_words_are_not_fillers() {
    let s = "Mum said umbrella, human, hummus and summer are fine.";
    assert_eq!(remove_fillers(s), s);
}

use voxa_core::text::{apply_style, changed_phrase, replace_phrases, spoken_formatting};

fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
    p.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn fixes_replace_whole_words_in_any_case() {
    let p = pairs(&[("clawed code", "Claude Code"), ("oxa", "Voxa")]);
    assert_eq!(replace_phrases("I asked Clawed Code about oxa.", &p), "I asked Claude Code about Voxa.");
    assert_eq!(replace_phrases("toxa and oxalic stay", &p), "toxa and oxalic stay", "only whole words");
    let longest_first = pairs(&[("claude", "Claude"), ("claude code", "Claude Code")]);
    assert_eq!(replace_phrases("open claude code", &longest_first), "open Claude Code");
}

#[test]
fn snippets_expand() {
    let p = pairs(&[("my email", "sam@example.com")]);
    assert_eq!(replace_phrases("Send it to my email.", &p), "Send it to sam@example.com.");
}

#[test]
fn spoken_new_lines_and_bullets() {
    assert_eq!(spoken_formatting("Hello, new line, how are you?"), "Hello\nHow are you?");
    assert_eq!(spoken_formatting("First point. New paragraph. Second point."), "First point.\n\nSecond point.");
    assert_eq!(spoken_formatting("Shopping: bullet point milk bullet point eggs"), "Shopping:\n- Milk\n- Eggs");
    assert_eq!(spoken_formatting("The newline character"), "The newline character", "not a command");
    // Owner's test, 2026-11-22: Whisper writes "new line" as "Newline", and "next para" is short.
    assert_eq!(
        spoken_formatting("Hi, my name is Simiyon Newline. I am from Coimbatore, Newline. But I'm studying in Chennai, SSN College, Newline. Yep. Next para.\n\nYeah, that's it."),
        "Hi, my name is Simiyon\nI am from Coimbatore\nBut I'm studying in Chennai, SSN College\nYep.\n\nYeah, that's it."
    );
    assert_eq!(spoken_formatting("Put a newline after each item"), "Put a newline after each item", "mid-sentence one-word form stays");
}

#[test]
fn per_app_style() {
    assert_eq!(apply_style("Sounds good.", true, false), "Sounds good");
    assert_eq!(apply_style("Wait for it...", true, false), "Wait for it...");
    assert_eq!(apply_style("Sounds Good.", false, true), "sounds good.");
}

#[test]
fn edits_become_fix_suggestions() {
    assert_eq!(changed_phrase("Ask clawed code to fix it.", "Ask Claude Code to fix it."), Some(("clawed code".into(), "Claude Code".into())));
    assert_eq!(changed_phrase("Hello there.", "Hello there."), None, "no change");
    assert_eq!(changed_phrase("One two three four five six", "Something entirely different now"), None, "a rewrite is not a fix");
    assert_eq!(changed_phrase("Send it.", "Send it now."), None, "an addition is not a fix");
}
