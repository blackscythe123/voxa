//! Small decisions of the desktop shell, kept apart from GPUI and the OS so each one is tested.

use voxa_core::copy::StripText;
use voxa_core::{Effect, Flow, JobId, JobState, Mic, Problem, State, TranscribeError};

use crate::audio::{MicError, Trouble};
use crate::store::{Status, NOT_REACHED, NO_SPEECH, RECORDING, WAITING_NETWORK, WAITING_SIGN_IN, WRITING};

/// What to do with a take the core asks to upload.
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// The mic gave nothing, or the file couldn't be written: a job-scoped answer.
    Trouble(Mic),
    /// Quiet all through: never sent, answered as no speech.
    Silence,
    Upload,
}

pub fn transcribe_plan(trouble: Option<Trouble>, peak: Option<f32>, silence: f32) -> Plan {
    let quiet = peak.is_none_or(|p| p < silence);
    match trouble {
        Some(Trouble::Disk) if quiet => Plan::Trouble(Mic::CantSave),
        Some(Trouble::MicStopped) if quiet => Plan::Trouble(Mic::Stopped),
        _ if peak.is_some_and(|p| p < silence) => Plan::Silence,
        _ => Plan::Upload,
    }
}

/// Hunt 2: a dictation that finished off the strip only gets a toast when the answer changed it
/// (a late answer the core ignored, for a cancelled retry, changes nothing and says nothing).
pub fn answer_changed_job(effects: &[Effect], job: JobId) -> bool {
    effects.contains(&Effect::SaveJob { job })
}

/// The History row for a job's state: (text, status), or None to remove the entry.
/// `text_now` is the entry's text in History, kept while it is written out again. `held`: its
/// upload waits after a failed try (so it isn't "being written out" yet).
pub fn history_row(state: &JobState, text_now: &str, held: bool) -> Option<(String, Status)> {
    let waiting = |w: &str| Some((String::new(), Status::Waiting(w.into())));
    match state {
        JobState::Recording => waiting(RECORDING),
        JobState::Transcribing if held => Some((text_now.to_string(), Status::Waiting(WAITING_NETWORK.into()))),
        JobState::Transcribing => Some((text_now.to_string(), Status::Waiting(WRITING.into()))),
        JobState::WaitingForNetwork => waiting(WAITING_NETWORK),
        JobState::Ready(t) | JobState::Delivered(t) if t.trim().is_empty() => None,
        JobState::Ready(t) => Some((t.clone(), Status::Ready)),
        JobState::Delivered(t) => Some((t.clone(), Status::Pasted)),
        JobState::Failed(e) if e.needs_sign_in() => waiting(WAITING_SIGN_IN),
        JobState::Failed(TranscribeError::Server(204)) => Some((String::new(), Status::Failed(NO_SPEECH.into()))),
        JobState::Failed(_) => Some((String::new(), Status::Failed(NOT_REACHED.into()))),
        JobState::Discarded => None,
    }
}

/// History's "Send now" or "Write it out again" for a dictation of this run.
#[derive(Debug, PartialEq)]
pub enum Route {
    /// Still recording, being written out or on the strip: its result is on the way.
    Busy,
    /// Waiting or failed: the core sends it (on the strip when free, else in the background).
    Retry,
    /// Already written out: write it out again in the background, never pasted.
    Rewrite,
}

pub fn resend_route(flow: &Flow, job: JobId) -> Route {
    if flow.in_progress(job) {
        return Route::Busy;
    }
    match flow.job(job).map(|j| &j.state) {
        Some(JobState::Ready(_) | JobState::Delivered(_)) => Route::Rewrite,
        Some(JobState::WaitingForNetwork | JobState::Failed(_)) => Route::Retry,
        _ => Route::Busy,
    }
}

/// Why the mic wouldn't open, beyond what the core's readiness can say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MicNote {
    InUse,
}

/// Realworld 18: the readiness for a mic that wouldn't open. Only a refused access is "denied"
/// (which offers the privacy settings).
pub fn mic_failure(e: &MicError) -> (Mic, Option<MicNote>) {
    match e {
        MicError::NoDevice => (Mic::Missing, None),
        MicError::Disk(_) => (Mic::CantSave, None),
        MicError::Blocked(_) => (Mic::Denied, None),
        MicError::InUse(_) => (Mic::Stopped, Some(MicNote::InUse)),
        MicError::Failed(_) => (Mic::Stopped, None),
    }
}

/// Hunt 10: tried again at once only when the connection never got through, and only for the
/// take on the strip. A timeout may mean ChatGPT got the audio and is still writing it out:
/// trying again would transcribe the same take twice. That waits for the user's Retry.
pub fn quick_retry(result: &Result<String, TranscribeError>, strip: bool) -> bool {
    strip && matches!(result, Err(TranscribeError::Offline))
}

/// The strip's words when the desktop knows more than the core: the paste was refused by an
/// admin window, or another app holds the microphone.
pub fn strip_override(state: &State, paste_refused: Option<JobId>, mic_note: Option<MicNote>) -> Option<StripText> {
    match state {
        State::NeedsYou(Problem::PasteFailed { job }) if paste_refused == Some(*job) => Some(StripText {
            title: "Couldn't paste into this window".into(),
            detail: Some("It's copied, press Ctrl+V".into()),
            action: None,
            dismissable: true,
        }),
        State::Blocked(voxa_core::Blocker::MicStopped) if mic_note == Some(MicNote::InUse) => Some(StripText {
            title: "Another app is using the microphone".into(),
            detail: Some("Close it, then press your shortcut again".into()),
            action: None,
            dismissable: true,
        }),
        _ => None,
    }
}

/// What a paste did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PasteResult {
    Pasted,
    /// An admin window would drop the keys: the text was left on the clipboard instead.
    Refused,
    Failed,
}

/// Integrity of a process's token, as far as Voxa can tell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Integrity {
    Level(u32),
    /// The process exists but its token can't be read: elevated or protected.
    Hidden,
    Unknown,
}

/// Windows' high integrity level (an elevated "Run as administrator" process).
pub const HIGH: u32 = 0x3000;

/// Realworld 13: Windows silently drops keys sent to a window of a higher integrity level
/// (UIPI). Then Voxa must not claim it pasted.
pub fn paste_refused(own: u32, target: Integrity) -> bool {
    match target {
        Integrity::Level(t) => t > own,
        Integrity::Hidden => own < HIGH,
        Integrity::Unknown => false,
    }
}

/// Hunt 21: the session a check read from disk, adopted only if no upload adopted a newer
/// refreshed one while the check ran.
pub struct SessionSlot<S> {
    pub session: Option<S>,
    /// Goes up each time an upload's refreshed session is adopted.
    adopted: u64,
}

impl<S> Default for SessionSlot<S> {
    fn default() -> Self {
        Self { session: None, adopted: 0 }
    }
}

impl<S> SessionSlot<S> {
    pub fn version(&self) -> u64 {
        self.adopted
    }
    pub fn adopt_refreshed(&mut self, s: S) {
        self.session = Some(s);
        self.adopted += 1;
    }
    /// A check that started at `version` finished with `checked`.
    pub fn adopt_checked(&mut self, version: u64, checked: Option<S>) {
        if self.adopted == version || self.session.is_none() {
            self.session = checked;
        }
    }
    /// A sign-in or sign-out: whatever was adopted before no longer counts.
    pub fn reset(&mut self, s: Option<S>) {
        self.session = s;
        self.adopted += 1;
    }
}

/// Hunt 22: one strip rebuild at a time, so a window is never left behind.
#[derive(Default)]
pub struct Rebuild {
    running: bool,
}

impl Rebuild {
    /// Whether a rebuild may start now (none is in flight).
    pub fn begin(&mut self) -> bool {
        !std::mem::replace(&mut self.running, true)
    }
    pub fn end(&mut self) {
        self.running = false;
    }
}

/// Hunt 20: what "Send now" does when there is no session in memory.
#[derive(Debug, PartialEq)]
pub enum NoSession {
    /// The first check hasn't answered yet: wait for it, never open the sign-in window.
    WaitForCheck,
    /// Signed out for sure: open the sign-in window (once).
    SignIn,
}

pub fn without_session(known: voxa_core::Session) -> NoSession {
    match known {
        voxa_core::Session::Unknown => NoSession::WaitForCheck,
        _ => NoSession::SignIn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxa_core::flow::Delivery;
    use voxa_core::{Event, Network, Readiness, Session, Target};

    fn ready() -> Readiness {
        Readiness { session: Session::Valid, network: Network::Up, mic: Mic::Granted }
    }
    fn take(f: &mut Flow) -> JobId {
        f.handle(Event::Hotkey { target: Target { app: "A".into(), window: 1 } });
        match f.state() {
            State::Recording { job, .. } => *job,
            s => panic!("{s:?}"),
        }
    }

    /// Hunt 3/12: mic trouble is answered as trouble, silence as silence, never as a Cancel.
    #[test]
    fn every_transcribe_has_an_answer_and_trouble_is_job_scoped() {
        assert_eq!(transcribe_plan(Some(Trouble::MicStopped), Some(0.0), 0.003), Plan::Trouble(Mic::Stopped));
        assert_eq!(transcribe_plan(Some(Trouble::Disk), None, 0.003), Plan::Trouble(Mic::CantSave));
        assert_eq!(transcribe_plan(Some(Trouble::MicStopped), Some(0.2), 0.003), Plan::Upload, "a take with real sound is sent");
        assert_eq!(transcribe_plan(None, Some(0.001), 0.003), Plan::Silence);
        assert_eq!(transcribe_plan(None, Some(0.2), 0.003), Plan::Upload);
    }

    /// Hunt 2: the late answer of a cancelled retry changes nothing, so no "Couldn't send" toast.
    #[test]
    fn a_late_answer_that_changes_nothing_sends_no_toast() {
        let mut f = Flow::new(ready());
        let job = take(&mut f);
        f.handle(Event::Stop);
        f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
        f.handle(Event::Action);
        f.handle(Event::Cancel);
        let fx = f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
        assert!(!answer_changed_job(&fx, job));
        // A late success keeps the text: worth its "ready" toast.
        f.handle(Event::Retry { job });
        f.handle(Event::Cancel);
        let fx = f.handle(Event::Transcribed { job, result: Ok("hi".into()) });
        assert!(answer_changed_job(&fx, job));
    }

    /// Hunt 17: a take with no speech says so in History, not "Couldn't reach ChatGPT".
    #[test]
    fn no_speech_is_never_filed_as_unreachable() {
        let (_, s) = history_row(&JobState::Failed(TranscribeError::Server(204)), "", false).unwrap();
        assert_eq!(s, Status::Failed(NO_SPEECH.into()));
        let (_, s) = history_row(&JobState::Failed(TranscribeError::Server(500)), "", false).unwrap();
        assert_eq!(s, Status::Failed(NOT_REACHED.into()));
    }

    /// Realworld 5: a take is in History from its first moment, so a forced end never loses it.
    #[test]
    fn a_take_is_in_history_while_recording_and_writing() {
        assert_eq!(history_row(&JobState::Recording, "", false).unwrap().1, Status::Waiting(RECORDING.into()));
        assert_eq!(history_row(&JobState::Transcribing, "", false).unwrap().1, Status::Waiting(WRITING.into()));
        assert_eq!(history_row(&JobState::Transcribing, "", true).unwrap().1, Status::Waiting(WAITING_NETWORK.into()), "held after a failed try");
        // Written out again: its text stays in History meanwhile.
        assert_eq!(history_row(&JobState::Transcribing, "old words", false).unwrap().0, "old words");
        assert!(history_row(&JobState::Discarded, "", false).is_none());
    }

    /// Hunt 13: History's buttons always do something for this run's dictations.
    #[test]
    fn send_now_and_write_it_out_again_are_routed_for_this_runs_dictations() {
        let mut f = Flow::new(ready());
        let pasted = take(&mut f);
        f.handle(Event::Stop);
        f.handle(Event::Transcribed { job: pasted, result: Ok("hi".into()) });
        f.handle(Event::Delivered { job: pasted, result: Delivery::Pasted });
        f.handle(Event::Dismiss);
        assert_eq!(resend_route(&f, pasted), Route::Rewrite);
        let failed = take(&mut f);
        f.handle(Event::Stop);
        f.handle(Event::Transcribed { job: failed, result: Err(TranscribeError::Server(500)) });
        f.handle(Event::Dismiss);
        assert_eq!(resend_route(&f, failed), Route::Retry);
        let now = take(&mut f);
        assert_eq!(resend_route(&f, now), Route::Busy);
        assert_eq!(resend_route(&f, failed), Route::Retry, "while recording too (sent in the background)");
    }

    #[test]
    fn mic_in_use_is_not_reported_as_privacy() {
        assert_eq!(mic_failure(&MicError::InUse("x".into())), (Mic::Stopped, Some(MicNote::InUse)));
        assert_eq!(mic_failure(&MicError::Failed("x".into())).0, Mic::Stopped);
        assert_eq!(mic_failure(&MicError::Blocked("x".into())).0, Mic::Denied);
        let t = strip_override(&State::Blocked(voxa_core::Blocker::MicStopped), None, Some(MicNote::InUse)).unwrap();
        assert_eq!(t.title, "Another app is using the microphone");
    }

    /// Hunt 10: a timeout is never retried on its own (ChatGPT may be writing it out already).
    #[test]
    fn a_timeout_is_never_uploaded_again_automatically() {
        assert!(!quick_retry(&Err(TranscribeError::Timeout), true));
        assert!(quick_retry(&Err(TranscribeError::Offline), true));
        assert!(!quick_retry(&Err(TranscribeError::Offline), false), "background takes wait their turn instead");
    }

    /// Realworld 13: a paste into an admin window is reported as refused, with its own words.
    #[test]
    fn a_paste_into_an_admin_window_is_never_reported_as_pasted() {
        let medium = 0x2000;
        assert!(paste_refused(medium, Integrity::Level(HIGH)));
        assert!(paste_refused(medium, Integrity::Hidden));
        assert!(!paste_refused(medium, Integrity::Level(medium)));
        assert!(!paste_refused(medium, Integrity::Level(0x1000)), "a low (sandboxed) app accepts it");
        assert!(!paste_refused(HIGH, Integrity::Hidden), "an elevated Voxa can paste there");
        assert!(!paste_refused(medium, Integrity::Unknown));
        let t = strip_override(&State::NeedsYou(Problem::PasteFailed { job: JobId(3) }), Some(JobId(3)), None).unwrap();
        assert_eq!((t.title.as_str(), t.detail.as_deref()), ("Couldn't paste into this window", Some("It's copied, press Ctrl+V")));
        assert!(strip_override(&State::NeedsYou(Problem::PasteFailed { job: JobId(4) }), Some(JobId(3)), None).is_none());
    }

    /// Hunt 21: a check that read the old session never replaces a newer refreshed one.
    #[test]
    fn a_session_check_never_restores_an_older_cookie() {
        let mut s: SessionSlot<&str> = SessionSlot::default();
        s.adopt_checked(s.version(), Some("v1"));
        let started = s.version();
        s.adopt_refreshed("v2"); // an upload got a refreshed sign-in meanwhile
        s.adopt_checked(started, Some("v1"));
        assert_eq!(s.session, Some("v2"));
        // A check that started after it is adopted.
        let started = s.version();
        s.adopt_checked(started, Some("v3"));
        assert_eq!(s.session, Some("v3"));
    }

    /// Hunt 22: a second rebuild while one runs is refused, so the first's window isn't leaked.
    #[test]
    fn only_one_strip_rebuild_runs_at_a_time() {
        let mut r = Rebuild::default();
        assert!(r.begin());
        assert!(!r.begin());
        r.end();
        assert!(r.begin());
    }

    /// Hunt 20: Send now in the first second waits for the check instead of opening sign-in.
    #[test]
    fn send_now_before_the_first_check_never_opens_sign_in() {
        assert_eq!(without_session(Session::Unknown), NoSession::WaitForCheck);
        assert_eq!(without_session(Session::Expired), NoSession::SignIn);
    }
}
