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
    /// Classify an HTTP response the way the core expects.
    ///
    /// - `cf-mitigated`, or an HTML page with 403 or 503 (Cloudflare's challenge and block pages),
    ///   is Blocked, and a Cloudflare block must not look like an expired session (the Android
    ///   sign-in loop of Nov 2026 came from exactly this).
    /// - Any other HTML page answering in ChatGPT's place (a hotel or cafe Wi-Fi sign-in page, a
    ///   redirect, 511) means the connection doesn't reach ChatGPT yet: Offline, so the take waits
    ///   for the network instead of blaming Cloudflare.
    /// - Other statuses keep their number (413, 429, 5xx), so the words can be honest.
    pub fn from_response(status: u16, body_is_html: bool, cf_mitigated: bool) -> Option<Self> {
        if cf_mitigated {
            return Some(Self::Blocked);
        }
        match status {
            403 | 503 if body_is_html => Some(Self::Blocked),
            200..=399 | 511 if body_is_html => Some(Self::Offline),
            200..=299 => None,
            300..=399 | 511 => Some(Self::Offline),
            401 | 403 => Some(Self::SessionExpired),
            s => Some(Self::Server(s)),
        }
    }

    /// What History says about a take that failed this way: plain and honest about the cause.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::SignedOut | Self::SessionExpired => "Sign in to send",
            Self::Offline => "Sends when you're back online",
            Self::Blocked => "ChatGPT blocked the request",
            Self::Timeout => "ChatGPT took too long to answer",
            Self::Server(204) => "No speech in the recording",
            Self::Server(413) => "Too long for ChatGPT",
            Self::Server(429) => "ChatGPT is busy. Try again in a minute",
            Self::Server(500..=599) => "ChatGPT had a problem. Try again later",
            Self::Server(_) => "Couldn't reach ChatGPT",
        }
    }

    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::SignedOut | Self::SessionExpired)
    }
}

#[cfg(test)]
mod tests {
    use super::TranscribeError as E;

    #[test]
    fn a_wifi_sign_in_page_is_not_reported_as_cloudflare() {
        assert_eq!(E::from_response(200, true, false), Some(E::Offline), "captive portal page");
        assert_eq!(E::from_response(302, false, false), Some(E::Offline));
        assert_eq!(E::from_response(511, true, false), Some(E::Offline));
        assert_eq!(E::from_response(502, true, false), Some(E::Server(502)), "a gateway error page keeps its status");
        assert_eq!(E::from_response(413, true, false), Some(E::Server(413)), "nginx's too large page");
        assert_eq!(E::from_response(403, true, false), Some(E::Blocked), "Cloudflare's challenge");
        assert_eq!(E::from_response(503, true, false), Some(E::Blocked));
        assert_eq!(E::from_response(200, false, true), Some(E::Blocked));
    }

    #[test]
    fn failures_have_honest_reasons() {
        assert_eq!(E::Server(413).reason(), "Too long for ChatGPT");
        assert_eq!(E::Server(429).reason(), "ChatGPT is busy. Try again in a minute");
        assert_eq!(E::Server(503).reason(), "ChatGPT had a problem. Try again later");
        assert_eq!(E::Server(0).reason(), "Couldn't reach ChatGPT");
        for e in [E::Server(413), E::Server(429), E::Server(500), E::Timeout, E::Blocked, E::Offline] {
            assert!(!e.reason().contains(['-', '\u{2013}', '\u{2014}']), "no dashes in copy");
        }
    }
}
