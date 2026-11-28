//! Rules around one take, copied from the desktop's `main.rs` (the constants near the top) so every
//! app decides the same way. Retrying an upload lives in `voxa_chatgpt::ChatGpt::transcribe`: only
//! a connection that never opened is tried again, so one send never uploads the audio twice.

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
}
