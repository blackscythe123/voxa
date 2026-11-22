//! The words the strip and the keyboard bar show, decided once here so desktop and Android
//! always say the same thing (flow audit: "error to copy-and-action mapping in the core").
//! Wording follows docs/ui-mocks/v7.1-native.html.

use crate::flow::{Flow, Problem, State};
use crate::job::JobId;
use crate::readiness::Blocker;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripText {
    pub title: String,
    pub detail: Option<String>,
    /// Label of the one primary action, if any.
    pub action: Option<String>,
    /// Whether the strip shows a close button.
    pub dismissable: bool,
}

fn text(title: impl Into<String>, detail: Option<String>, action: Option<String>, dismissable: bool) -> StripText {
    StripText { title: title.into(), detail, action, dismissable }
}

fn app_of(flow: &Flow, job: JobId) -> String {
    flow.job(job).map(|j| j.target.app.clone()).unwrap_or_default()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

/// What the strip says for the flow's current state. `None` when the strip is hidden.
pub fn strip_text(flow: &Flow) -> Option<StripText> {
    Some(match flow.state() {
        State::Idle => return None,
        State::Blocked(Blocker::SignedOut) => {
            text("Sign in to ChatGPT to dictate", s("Nothing was recorded"), s("Sign in"), true)
        }
        State::Blocked(Blocker::MicDenied) => {
            text("Voxa can't use the microphone", s("It's blocked in privacy settings"), s("Open settings"), true)
        }
        State::Blocked(Blocker::MicMissing) => {
            text("No microphone found", s("Connect one and try again"), s("Open settings"), true)
        }
        State::Recording { offline: true, .. } => text("Offline, keeping it", None, s("Stop"), false),
        State::Recording { .. } => text("Listening", None, s("Stop"), false),
        State::Transcribing { .. } => text("Writing it out", None, None, false),
        State::Delivering { .. } => text("Pasting", None, None, false),
        State::Delivered { job } => text(
            format!("Pasted in {}", app_of(flow, *job)),
            s("Your clipboard is back as it was"),
            None,
            false,
        ),
        State::NeedsYou(p) => match p {
            Problem::SignInToSend { job } => text(
                "Sign in to send this",
                Some(format!("Your recording is kept. It pastes in {} after", app_of(flow, *job))),
                s("Sign in"),
                true,
            ),
            Problem::WaitingForNetwork { .. } => {
                text("No connection. Sends when you're back", s("Your recording is kept"), s("Retry now"), true)
            }
            Problem::Unreachable { .. } => {
                text("Couldn't reach ChatGPT", s("Your recording is kept. Your sign-in is fine"), s("Retry"), true)
            }
            Problem::NoSpeech { .. } => text("Didn't hear any speech", s("Check the microphone"), None, true),
            Problem::TargetChanged { job, now_in } => text(
                format!("You moved to {}", now_in.app),
                Some(format!("Go back to {} and it pastes there", app_of(flow, *job))),
                Some(format!("Paste in {}", now_in.app)),
                true,
            ),
            Problem::PasteFailed { .. } => text("Couldn't paste here", s("The text is ready"), s("Paste again"), true),
        },
    })
}
