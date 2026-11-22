//! A dictation is a durable job from the first moment of recording, so nothing the user said is
//! ever lost (flow audit, systemic flaw 4).

use crate::error::TranscribeError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(pub u64);

/// The app and window the user was typing in when they started. Captured at start so a late
/// result never lands in the wrong chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub app: String,
    /// Opaque window identity from the shell (HWND on Windows, input-connection id on Android).
    pub window: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    Recording,
    /// Recorded while offline; sends itself when the network is back.
    WaitingForNetwork,
    Transcribing,
    /// Text is ready; delivery is in progress or needs the user.
    Ready(String),
    Delivered(String),
    Failed(TranscribeError),
    Discarded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: JobId,
    pub target: Target,
    pub state: JobState,
}
