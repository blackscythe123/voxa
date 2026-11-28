//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.
//! No network: only the pure functions.

use voxa_chatgpt::{has_session_token, merge_set_cookies_at, parse_http_date, upload_timeout};

const TOKEN: &str = "__Secure-next-auth.session-token";

/// RFC 6265 5.2.2: a Max-Age that isn't a valid number is ignored, and a number too large is
/// capped. Here any parse failure becomes Max-Age=0, i.e. "delete": a huge Max-Age (or one with
/// a stray character) on a re-issued session token signs the user out.
#[test]
fn a_huge_or_odd_max_age_does_not_delete_the_session_token() {
    let jar = format!("{TOKEN}=OLD; a=1");
    let huge = format!("{TOKEN}=NEW; Path=/; Max-Age=99999999999999999999; Secure");
    let merged = merge_set_cookies_at(&jar, &[&huge], 1_800_000_000);
    assert!(has_session_token(&merged), "session token deleted by an oversized Max-Age: {}", merged.replace("NEW", "<v>"));
    let odd = format!("{TOKEN}=NEW; Max-Age=\"7776000\"");
    let merged = merge_set_cookies_at(&jar, &[&odd], 1_800_000_000);
    assert!(has_session_token(&merged), "session token deleted by a quoted Max-Age");
}

/// Expires dates come from the network. Absurd values overflow i64 arithmetic: a panic in debug
/// builds (inside a UniFFI call), a wrapped nonsense time in release. They should be rejected.
#[test]
fn absurd_expires_dates_are_rejected_not_overflowed() {
    let r = std::panic::catch_unwind(|| parse_http_date("Sat, 28 Nov 99999999999999999 08:59:41 GMT"));
    assert!(matches!(r, Ok(None)), "absurd year: {:?}", r.as_ref().map_err(|_| "panicked"));
    let r = std::panic::catch_unwind(|| parse_http_date("Sat, 28 Nov 2026 9223372036854775807:00:00 GMT"));
    assert!(matches!(r, Ok(None)), "absurd hour: {:?}", r.as_ref().map_err(|_| "panicked"));
}

/// upload_timeout's own premise is a ~133 kbit/s uplink (60 s per MB), but the 15-minute cap
/// breaks it above ~14.5 MB (about 7.5 minutes of speech that trimming can't shorten): such a
/// take can never finish on that uplink, and `take::with_retries` then uploads it twice more
/// (Timeout is retried), so it spends ~45 minutes and three full uploads before failing.
#[test]
fn upload_timeout_covers_a_long_take_on_the_slow_uplink_it_assumes() {
    let bytes = 19_200_000usize; // a 10-minute 16 kHz mono WAV
    let needed_secs = bytes as u64 * 8 / 133_000;
    assert!(upload_timeout(bytes).as_secs() >= needed_secs, "allowed {} s, needs {needed_secs} s", upload_timeout(bytes).as_secs());
}

/// A 2xx reply with no transcript in a known field is a server problem, never "no speech": the
/// recording is kept and can be sent again. An empty string is ChatGPT hearing nothing.
#[test]
fn a_reply_without_a_transcript_is_not_no_speech() {
    use voxa_chatgpt::transcript_from;
    assert_eq!(transcript_from(&serde_json::json!({"text": "hi"})), Some("hi".into()));
    assert_eq!(transcript_from(&serde_json::json!({"text": ""})), Some(String::new()));
    assert_eq!(transcript_from(&serde_json::json!({})), None);
    assert_eq!(transcript_from(&serde_json::json!({"text": null})), None);
    assert_eq!(transcript_from(&serde_json::json!({"result": {"text": "nested"}})), None);
    assert_eq!(transcript_from(&serde_json::json!({"message": "Rate limited"})), None, "an error message is never pasted as a transcript");
}

/// One press of Send never uploads the same audio twice: only a failure to connect (nothing was
/// sent) is tried again, at most twice.
#[test]
fn audio_is_only_sent_again_when_nothing_went_out() {
    use voxa_chatgpt::retry_wait;
    assert_eq!(retry_wait(false, 0), None, "a timeout or reset after sending is never retried");
    assert!(retry_wait(true, 0).is_some());
    assert!(retry_wait(true, 1).is_some());
    assert_eq!(retry_wait(true, 2), None);
}

/// A JSON 401 or 403 from the transcribe request, right after the access token was accepted, is
/// not a sign-out: it must never block recording or start a sign-in loop.
#[test]
fn a_refusal_right_after_a_valid_token_is_not_a_sign_out() {
    use voxa_chatgpt::transcribe_reply_error;
    use voxa_core::TranscribeError;
    assert_eq!(transcribe_reply_error(403, false, false), Some(TranscribeError::Server(403)));
    assert_eq!(transcribe_reply_error(401, false, false), Some(TranscribeError::Server(401)));
    assert_eq!(transcribe_reply_error(403, true, false), Some(TranscribeError::Blocked));
    assert_eq!(transcribe_reply_error(200, false, false), None);
}
