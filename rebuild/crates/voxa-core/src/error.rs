//! Typed failures from turning speech into text. Replaces the current apps' string matching
//! (`classifyError`, raw `SESSION_EXPIRED` shown to users).

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscribeError {
    /// No ChatGPT session stored at all.
    SignedOut,
    /// ChatGPT rejected the session (401/403 with a JSON body).
    SessionExpired,
    /// Cloudflare challenge or block (HTML body, or `cf-mitigated`). Never means "sign in again".
    Blocked,
    /// No connection (DNS, connect, reset).
    Offline,
    /// The request ran past the time limit.
    Timeout,
    /// ChatGPT answered with an unexpected status.
    Server(u16),
}

impl TranscribeError {
    /// Classify an HTTP response the way the core expects. Cloudflare is checked before the
    /// status code: a Cloudflare block arrives as 403 and must not look like an expired session
    /// (the Android sign-in loop of Nov 2026 came from exactly this).
    pub fn from_response(status: u16, body_is_html: bool, cf_mitigated: bool) -> Option<Self> {
        if body_is_html || cf_mitigated {
            return Some(Self::Blocked);
        }
        match status {
            200..=299 => None,
            401 | 403 => Some(Self::SessionExpired),
            s => Some(Self::Server(s)),
        }
    }

    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::SignedOut | Self::SessionExpired)
    }
}
