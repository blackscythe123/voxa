//! Notifications for Kotlin: the words and anti-spam rules from `voxa_core::notices`, and the
//! background send of dictations left waiting for the network (docs/notifications.md).

use voxa_core::TranscribeError;
use voxa_core::notices::{self, Decision, Kind};
use voxa_store::{History, Status};

use crate::{VoxaCore, diag};

/// History's reason for a take recorded offline. The background send picks these up.
pub const WAITING_FOR_NETWORK: &str = "Sends when you're back online";
/// History's reason for a take that needs the user to sign in first.
pub const WAITING_FOR_SIGN_IN: &str = "Sign in to send";

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NoticeText {
    pub title: String,
    pub body: String,
    pub action: Option<String>,
}

impl From<notices::Notice> for NoticeText {
    fn from(n: notices::Notice) -> Self {
        Self { title: n.title, body: n.body, action: n.action }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoticeKind {
    SignIn,
    Dictations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoticeDecision {
    Post,
    Update,
    Skip,
}

#[uniffi::export]
pub fn notice_signed_out(waiting: u32) -> NoticeText {
    notices::signed_out(waiting).into()
}

#[uniffi::export]
pub fn notice_ready(count: u32, app: Option<String>) -> NoticeText {
    notices::ready(count, app.as_deref()).into()
}

#[uniffi::export]
pub fn notice_not_sent(count: u32) -> NoticeText {
    notices::not_sent(count, "Retry").into()
}

#[uniffi::export]
pub fn notice_public_title() -> String {
    notices::PUBLIC_TITLE.into()
}

/// The anti-spam rules (see `voxa_core::notices::decide`).
#[uniffi::export]
pub fn notice_decide(kind: NoticeKind, now_ms: u64, showing: bool, posted: Vec<u64>, last_dictations: u64) -> NoticeDecision {
    let kind = match kind {
        NoticeKind::SignIn => Kind::SignIn,
        NoticeKind::Dictations => Kind::Dictations,
    };
    match notices::decide(kind, now_ms, showing, &posted, last_dictations) {
        Decision::Post => NoticeDecision::Post,
        Decision::Update => NoticeDecision::Update,
        Decision::Skip => NoticeDecision::Skip,
    }
}

/// Post times worth keeping, with `now_ms` added.
#[uniffi::export]
pub fn notice_record(posted: Vec<u64>, now_ms: u64) -> Vec<u64> {
    notices::record(&posted, now_ms)
}

#[uniffi::export]
pub fn notice_quiet_hour(hour: u32) -> bool {
    notices::quiet_hour(hour)
}

/// How a background send of one waiting dictation ended.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SendOutcome {
    /// Written out; the text waits in History, ready to copy.
    Sent { app: String },
    NoSpeech,
    /// ChatGPT wants a sign-in: the entry now waits for it.
    NeedsSignIn,
    /// Still no connection: the entry keeps waiting for the network.
    Offline,
    /// ChatGPT couldn't be reached (blocked, timed out, server error). The entry keeps waiting,
    /// unless `give_up`, when it becomes "Couldn't reach ChatGPT".
    Unreachable { blocked: bool },
    /// Deleted, already sent, or its recording is gone.
    Gone,
    /// Already being sent (History's "Send now", or a send that outlived its job): try later.
    Busy,
}

#[uniffi::export]
impl VoxaCore {
    /// History entries waiting for the network, oldest first.
    pub fn waiting_for_network(&self) -> Vec<u64> {
        self.waiting(WAITING_FOR_NETWORK)
    }

    /// How many dictations wait for the user to sign in.
    pub fn waiting_for_sign_in(&self) -> u32 {
        self.waiting(WAITING_FOR_SIGN_IN).len() as u32
    }

    /// Send one dictation that waits for the network (the Android background job). Unlike
    /// `resend`, being offline or unreachable keeps it waiting, and an entry deleted meanwhile
    /// stays deleted. `canned`: debug builds only, the text to use instead of calling ChatGPT.
    pub fn send_waiting(&self, entry_id: u64, give_up: bool, canned: Option<String>) -> SendOutcome {
        let Some(_claim) = self.claim(entry_id) else {
            diag::log(|| format!("background send {entry_id}: already being sent"));
            return SendOutcome::Busy;
        };
        let eligible = |s: &Status| matches!(s, Status::Waiting(r) if r == WAITING_FOR_NETWORK) || (give_up && matches!(s, Status::Failed(_)));
        let Some(entry) = History::edit(&self.dir, |h| (false, h.get(entry_id).filter(|e| eligible(&e.status)).cloned())) else {
            return SendOutcome::Gone;
        };
        let Some(audio) = entry.audio.clone().filter(|a| a.exists()) else { return SendOutcome::Gone };
        let result = match canned {
            Some(t) => Ok(self.finish_text(t, entry.app.clone())),
            None => self.transcribe_recording(audio.to_string_lossy().to_string(), entry.app.clone()).map_err(TranscribeError::from),
        };
        diag::log(|| format!("background send {entry_id}: {}", match &result { Ok(t) if t.trim().is_empty() => "no speech".to_string(), Ok(_) => "ok".into(), Err(e) => format!("{e:?}") }));
        let (status, text, outcome) = match result {
            Ok(t) if t.trim().is_empty() => (Status::Failed("No speech in the recording".into()), String::new(), SendOutcome::NoSpeech),
            Ok(t) => (Status::Ready, t, SendOutcome::Sent { app: entry.app.clone() }),
            Err(e) if e.needs_sign_in() => (Status::Waiting(WAITING_FOR_SIGN_IN.into()), String::new(), SendOutcome::NeedsSignIn),
            Err(TranscribeError::Offline) if !give_up => return SendOutcome::Offline,
            Err(e) if give_up => (Status::Failed("Couldn't reach ChatGPT".into()), String::new(), SendOutcome::Unreachable { blocked: e == TranscribeError::Blocked }),
            Err(e) => return SendOutcome::Unreachable { blocked: e == TranscribeError::Blocked },
        };
        History::edit(&self.dir, move |h| {
            // Deleted or sent another way meanwhile: leave it.
            let Some(now) = h.get(entry_id).filter(|e| eligible(&e.status)).cloned() else { return (false, ()) };
            (h.upsert(voxa_store::Entry { status, text, ..now }), ())
        });
        outcome
    }
}

impl VoxaCore {
    fn waiting(&self, reason: &str) -> Vec<u64> {
        History::edit(&self.dir, |h| {
            let ids = h.entries.iter().filter(|e| matches!(&e.status, Status::Waiting(r) if r == reason)).map(|e| e.id).collect();
            (false, ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::now_ms;

    #[test]
    fn a_dictation_being_sent_is_never_sent_a_second_time() {
        let dir = std::env::temp_dir().join(format!("voxa-send-twice-{}-{}", std::process::id(), now_ms()));
        let core = VoxaCore::new(dir.to_string_lossy().to_string());
        let audio = dir.join("a.pcm");
        std::fs::write(&audio, vec![0u8; 3200]).unwrap();
        History::edit(&dir, |h| {
            let e = voxa_store::Entry { id: 42, app: "Notes".into(), text: String::new(), secs: 1.0, status: Status::Waiting(WAITING_FOR_NETWORK.into()), audio: Some(audio.clone()), started_in: None, via: None };
            (h.upsert(e), ())
        });
        let held = core.claim(42).expect("free");
        assert_eq!(core.send_waiting(42, false, Some("hello".into())), SendOutcome::Busy);
        drop(held);
        assert_eq!(core.send_waiting(42, false, Some("hello".into())), SendOutcome::Sent { app: "Notes".into() });
        // Sent: a late second run finds nothing to send.
        assert_eq!(core.send_waiting(42, false, Some("hello".into())), SendOutcome::Gone);
        let _ = std::fs::remove_dir_all(dir);
    }
}
