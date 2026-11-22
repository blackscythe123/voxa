//! STAND-INS until the real services exist. Each one is named here so it cannot ship by accident.
//!
//! - Transcriber: returns scripted outcomes instead of calling ChatGPT.
//!   `VOXA_FAKE=expired,ok` gives an expired session first, then success. Outcomes:
//!   ok, empty, expired, blocked, offline, timeout. Default: ok.
//! - Session: `VOXA_SESSION=expired` starts signed out. Default: signed in.
//! - Sign-in: pretends the user signed in 1.5 s after "Sign in" is clicked.
//! - Microphone: the ink line is driven by a fake level; no audio is captured.

use std::collections::VecDeque;
use std::sync::Mutex;

use voxa_core::TranscribeError;

pub const FAKE_TEXT: &str = "Hello from Voxa.";

static SCRIPT: Mutex<Option<VecDeque<String>>> = Mutex::new(None);

pub fn next_transcription() -> Result<String, TranscribeError> {
    let mut guard = SCRIPT.lock().unwrap();
    let script = guard.get_or_insert_with(|| {
        std::env::var("VOXA_FAKE").unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    });
    match script.pop_front().as_deref().unwrap_or("ok") {
        "empty" => Ok(String::new()),
        "expired" => Err(TranscribeError::SessionExpired),
        "blocked" => Err(TranscribeError::Blocked),
        "offline" => Err(TranscribeError::Offline),
        "timeout" => Err(TranscribeError::Timeout),
        _ => Ok(FAKE_TEXT.to_string()),
    }
}

pub fn starts_signed_out() -> bool {
    std::env::var("VOXA_SESSION").map(|v| v == "expired").unwrap_or(false)
}

/// Fake mic level, 0..1, shaped like speech: bursts and pauses.
pub fn level(t: f32) -> f32 {
    ((t * 0.21).sin() * 0.5 + 0.5).powf(2.0) * (0.6 + 0.4 * (t * 1.7).sin().abs())
}
