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
    /// The mic stream stopped delivering sound (device reset, unplugged mid-take).
    Stopped,
    /// The recording couldn't be saved (disk full, data folder locked or unavailable).
    CantSave,
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
    MicStopped,
    CantSave,
    SignedOut,
}

impl Readiness {
    pub fn blocker(&self) -> Option<Blocker> {
        match (self.mic, self.session) {
            (Mic::Denied, _) => Some(Blocker::MicDenied),
            (Mic::Missing, _) => Some(Blocker::MicMissing),
            (Mic::Stopped, _) => Some(Blocker::MicStopped),
            (Mic::CantSave, _) => Some(Blocker::CantSave),
            (_, Session::Expired) => Some(Blocker::SignedOut),
            _ => None,
        }
    }
}
