//! Rules around one take, copied from the desktop's `main.rs` (the constants near the top and the
//! `Effect::Transcribe` handler) so every app decides the same way.

use std::time::Duration;

use voxa_core::TranscribeError;

/// Below this peak RMS a recording is treated as silence and never sent. Measured on a PC mic:
/// soft speech peaked at 0.0156, a silent take at 0.0079, so the cutoff sits well under soft
/// speech. (Not re-measured on phone mics yet.)
pub const SILENCE: f32 = 0.003;
/// Whisper invents these for near-silent audio. Only trusted when the recording was also quiet.
pub const QUIET: f32 = 0.02;
pub const SILENCE_PHRASES: &[&str] = &["thank you.", "thank you", "thanks for watching!", "thanks for watching.", "you", "bye."];

/// Whether `text` is Whisper's invention for a near-silent take with this peak RMS.
pub fn invented_for_silence(text: &str, peak: f32) -> bool {
    peak < QUIET && SILENCE_PHRASES.contains(&text.trim().to_lowercase().as_str())
}

/// Whether a take is too quiet to send at all.
pub fn is_silent(peak: f32) -> bool {
    peak < SILENCE
}

/// Waits before the second and third tries when the upload failed as offline or timed out.
pub const RETRY_WAITS_MS: [u64; 2] = [1500, 4000];

/// Run an upload up to three times: a dropped connection is common on flaky Wi-Fi and mobile data,
/// so try twice more before calling it offline.
pub fn with_retries(mut upload: impl FnMut() -> Result<String, TranscribeError>) -> Result<String, TranscribeError> {
    let mut result = upload();
    for wait in RETRY_WAITS_MS {
        if !matches!(result, Err(TranscribeError::Offline | TranscribeError::Timeout)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(wait));
        result = upload();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invented_phrases_only_count_when_quiet() {
        assert!(invented_for_silence("Thank you.", 0.01));
        assert!(!invented_for_silence("Thank you.", 0.05));
        assert!(!invented_for_silence("Thank you for the notes.", 0.01));
        assert!(is_silent(0.001) && !is_silent(0.0156));
    }

    #[test]
    fn retries_only_network_failures() {
        let mut n = 0;
        assert_eq!(with_retries(|| { n += 1; Err(TranscribeError::SessionExpired) }), Err(TranscribeError::SessionExpired));
        assert_eq!(n, 1, "a rejected session is not retried");
    }
}
