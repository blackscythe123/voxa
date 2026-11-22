//! What must be true before Voxa records. Checked when the hotkey or mic is pressed, not after
//! the user has spoken (flow audit, systemic flaw 1).

/// ChatGPT session state. `Unknown` means not checked yet; it does not block recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Valid,
    Expired,
    Unknown,
}

/// Network reachability. Being offline never blocks recording: Voxa keeps the audio and
/// sends it when the connection is back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mic {
    Granted,
    Denied,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Readiness {
    pub session: Session,
    pub network: Network,
    pub mic: Mic,
}

impl Default for Readiness {
    fn default() -> Self {
        Self { session: Session::Unknown, network: Network::Up, mic: Mic::Granted }
    }
}

/// Why Voxa will not start recording. Order matters: the first that applies is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    MicDenied,
    MicMissing,
    SignedOut,
}

impl Readiness {
    pub fn blocker(&self) -> Option<Blocker> {
        match (self.mic, self.session) {
            (Mic::Denied, _) => Some(Blocker::MicDenied),
            (Mic::Missing, _) => Some(Blocker::MicMissing),
            (_, Session::Expired) => Some(Blocker::SignedOut),
            _ => None,
        }
    }
}
