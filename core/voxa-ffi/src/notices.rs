//! Notifications for Kotlin: the words and anti-spam rules from `voxa_core::notices`, and the
//! background send of dictations left waiting for the network (docs/notifications.md).

use voxa_core::TranscribeError;
use voxa_core::greeting::{Date, MonthDay};
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
    /// A5: a verified update is ready to install (ID 4, channel "updates").
    Update,
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
        NoticeKind::Update => Kind::Update,
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

/// A5: "Voxa 1.1.0 is ready to install".
#[uniffi::export]
pub fn notice_update_ready(version: String) -> NoticeText {
    notices::update_ready(&version).into()
}

/// A5's lock screen title ("Voxa update ready"), not the global "Voxa needs you".
#[uniffi::export]
pub fn notice_update_public_title() -> String {
    notices::UPDATE_PUBLIC_TITLE.into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NoticeUpdateDecision {
    Post,
    /// Deferred to the next allowed moment (on screen, or the daily cap is used).
    Wait,
    /// Already posted for this version: never again.
    Never,
}

/// A5's rules (see `voxa_core::notices::decide_update`): once per version, never while Voxa is
/// visible, and it counts toward the daily cap, deferred rather than dropped.
#[uniffi::export]
pub fn notice_decide_update(version: String, posted_for: String, visible: bool, now_ms: u64, posted: Vec<u64>) -> NoticeUpdateDecision {
    match notices::decide_update(&version, &posted_for, visible, now_ms, &posted) {
        notices::UpdateDecision::Post => NoticeUpdateDecision::Post,
        notices::UpdateDecision::Wait => NoticeUpdateDecision::Wait,
        notices::UpdateDecision::Never => NoticeUpdateDecision::Never,
    }
}

#[uniffi::export]
pub fn notice_quiet_hour(hour: u32) -> bool {
    notices::quiet_hour(hour)
}

/// The birthday wish ("Happy birthday, Sam"). `name`: the account's name, first word used.
#[uniffi::export]
pub fn notice_birthday(name: Option<String>) -> NoticeText {
    notices::birthday(name.as_deref()).into()
}

fn month_day(month: u8, day: u8) -> Option<MonthDay> {
    Some(MonthDay { month, day }).filter(|b| b.valid())
}

/// The local date ("2027-03-14") whose midnight the next birthday wish is for, or None when
/// `today` or the birthday isn't valid. `wished_year`: the year last wished, 0 for never.
#[uniffi::export]
pub fn birthday_next_wish(today: String, month: u8, day: u8, wished_year: i32) -> Option<String> {
    Some(notices::next_birthday_wish(Date::parse(&today)?, month_day(month, day)?, wished_year).to_string())
}

/// Whether the wish is due now (`minutes` after local midnight on `today`), with Android's grace:
/// any time on the day itself, since Doze may hold the alarm back.
#[uniffi::export]
pub fn birthday_wish_due(today: String, minutes: u16, month: u8, day: u8, wished_year: i32) -> bool {
    match (Date::parse(&today), month_day(month, day)) {
        (Some(t), Some(b)) => notices::birthday_wish_due(t, minutes, b, wished_year, notices::BIRTHDAY_GRACE_ANDROID_MIN),
        _ => false,
    }
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

    /// History entries waiting for a sign-in, oldest first: sent by the background send once
    /// the session is good again (also takes from earlier runs, and ones the user dismissed).
    pub fn waiting_for_sign_in_ids(&self) -> Vec<u64> {
        self.waiting(WAITING_FOR_SIGN_IN)
    }

    /// Bytes of recordings for these entries: the background send asks for an unmetered network
    /// when this is large, so a long take isn't sent again and again over mobile data.
    pub fn audio_bytes_of(&self, entry_ids: Vec<u64>) -> u64 {
        History::edit(&self.dir, |h| {
            let n = entry_ids.iter().filter_map(|id| h.get(*id)?.audio.as_ref().and_then(|a| std::fs::metadata(a).ok()).map(|m| m.len())).sum();
            (false, n)
        })
    }

    /// Send one dictation that waits for the network or a sign-in (the Android background job).
    /// Being offline always keeps it waiting (also on the user's Retry: a flaky connection never
    /// turns a waiting take into a failed one); unreachable keeps it waiting unless `give_up`. An
    /// entry deleted meanwhile stays deleted. `canned`: debug builds only, the text to use
    /// instead of calling ChatGPT.
    pub fn send_waiting(&self, entry_id: u64, give_up: bool, canned: Option<String>) -> SendOutcome {
        let Some(_claim) = self.claim(entry_id) else {
            diag::log(|| format!("background send {entry_id}: already being sent"));
            return SendOutcome::Busy;
        };
        let eligible =
            |s: &Status| matches!(s, Status::Waiting(r) if r == WAITING_FOR_NETWORK || r == WAITING_FOR_SIGN_IN) || (give_up && matches!(s, Status::Failed(_)));
        let Some(entry) = History::edit(&self.dir, |h| (false, h.get(entry_id).filter(|e| eligible(&e.status)).cloned())) else {
            return SendOutcome::Gone;
        };
        let Some(audio) = entry.audio.clone().filter(|a| a.exists()) else { return SendOutcome::Gone };
        let result = match canned {
            Some(t) => Ok(self.finish_text(t, entry.app.clone())),
            None => self.transcribe_recording(audio.to_string_lossy().to_string(), entry.app.clone()).map_err(TranscribeError::from),
        };
        diag::log(|| format!("background send {entry_id}: {}", match &result { Ok(t) if t.trim().is_empty() => "no speech".to_string(), Ok(_) => "ok".into(), Err(e) => format!("{e:?}") }));
        let (outcome, write) = background_result(result, give_up, &entry.app);
        let Some((status, text)) = write else { return outcome };
        History::edit(&self.dir, move |h| {
            // Deleted or sent another way meanwhile: leave it.
            let Some(now) = h.get(entry_id).filter(|e| eligible(&e.status)).cloned() else { return (false, ()) };
            (h.upsert(voxa_store::Entry { status, text, ..now }), ())
        });
        outcome
    }
}

/// How one background send ended, and what to write into the entry (None: leave it as it is).
/// Being offline always keeps a take waiting, even on the user's Retry (`give_up`): a flaky
/// connection never turns waiting takes into failed ones.
pub(crate) fn background_result(result: Result<String, TranscribeError>, give_up: bool, app: &str) -> (SendOutcome, Option<(Status, String)>) {
    match result {
        Ok(t) if t.trim().is_empty() => (SendOutcome::NoSpeech, Some((Status::Failed(TranscribeError::Server(204).reason().into()), String::new()))),
        Ok(t) => (SendOutcome::Sent { app: app.to_string() }, Some((Status::Ready, t))),
        Err(e) if e.needs_sign_in() => (SendOutcome::NeedsSignIn, Some((Status::Waiting(WAITING_FOR_SIGN_IN.into()), String::new()))),
        Err(TranscribeError::Offline) => (SendOutcome::Offline, None),
        Err(e) if give_up => {
            let blocked = e == TranscribeError::Blocked;
            (SendOutcome::Unreachable { blocked }, Some((Status::Failed(e.reason().into()), String::new())))
        }
        Err(e) => (SendOutcome::Unreachable { blocked: e == TranscribeError::Blocked }, None),
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

    /// The user's Retry on a flaky connection: offline keeps every take waiting (to be sent
    /// automatically), and only a real failure becomes "not sent".
    #[test]
    fn a_retry_while_offline_never_marks_a_waiting_take_failed() {
        assert_eq!(background_result(Err(TranscribeError::Offline), true, "Notes"), (SendOutcome::Offline, None));
        let (o, w) = background_result(Err(TranscribeError::Server(503)), true, "Notes");
        assert_eq!(o, SendOutcome::Unreachable { blocked: false });
        assert_eq!(w, Some((Status::Failed("ChatGPT had a problem. Try again later".into()), String::new())));
        assert_eq!(background_result(Err(TranscribeError::Timeout), false, "Notes"), (SendOutcome::Unreachable { blocked: false }, None));
    }
}
