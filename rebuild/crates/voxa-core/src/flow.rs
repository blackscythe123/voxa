//! The dictation state machine. Pure: events in, effects out.

use std::collections::BTreeMap;

use crate::error::TranscribeError;
use crate::job::{Job, JobId, JobState, Target};
use crate::readiness::{Blocker, Network, Readiness, Session};

/// What the strip (desktop) or keyboard bar (Android) shows. One state at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// The hotkey was pressed but recording did not start. Nothing was recorded.
    Blocked(Blocker),
    Recording { job: JobId, offline: bool },
    Transcribing { job: JobId },
    Delivering { job: JobId },
    Delivered { job: JobId },
    /// Something needs the user. Always one action (or none) plus a dismiss.
    NeedsYou(Problem),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The session ended after recording; the audio is kept and is sent after sign-in.
    SignInToSend { job: JobId },
    /// Recorded while offline; it sends itself when the network is back.
    WaitingForNetwork { job: JobId },
    /// ChatGPT was unreachable, blocked or too slow. The sign-in is fine.
    Unreachable { job: JobId },
    /// The mic was live but nothing was said.
    NoSpeech { job: JobId },
    /// The user is now in a different app than the one they started in.
    TargetChanged { job: JobId, now_in: Target },
    /// The paste could not be delivered (blocked field, no permission).
    PasteFailed { job: JobId },
}

impl Problem {
    pub fn job(&self) -> JobId {
        match self {
            Problem::SignInToSend { job }
            | Problem::WaitingForNetwork { job }
            | Problem::Unreachable { job }
            | Problem::NoSpeech { job }
            | Problem::TargetChanged { job, .. }
            | Problem::PasteFailed { job } => *job,
        }
    }
}

/// Outcome of a paste attempt, reported by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    Pasted,
    /// The target is no longer in front; the shell did not paste.
    TargetChanged(Target),
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The hotkey (desktop) or mic key (Android) was pressed while the user was in `target`.
    Hotkey { target: Target },
    Stop,
    Cancel,
    /// The strip's one primary action.
    Action,
    /// The strip's close button, or Esc on a needs-you state.
    Dismiss,
    Transcribed { job: JobId, result: Result<String, TranscribeError> },
    Delivered { job: JobId, result: Delivery },
    Readiness(Readiness),
}

/// Work for the shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    ShowStrip,
    /// Hide after the given delay (0 = now).
    HideStrip { after_ms: u32 },
    StartCapture { job: JobId },
    StopCapture { job: JobId },
    DiscardCapture { job: JobId },
    Transcribe { job: JobId },
    /// Paste into the original target; the shell must check it is still in front.
    Paste { job: JobId, target: Target, text: String },
    /// Paste into whatever is in front now (the user chose "Paste in X").
    PasteHere { job: JobId, text: String },
    OpenSignIn,
    OpenMicSettings,
    /// Persist the job (history, retry, offline queue).
    SaveJob { job: JobId },
}

pub struct Flow {
    state: State,
    readiness: Readiness,
    jobs: BTreeMap<JobId, Job>,
    next_id: u64,
}

impl Default for Flow {
    fn default() -> Self {
        Self::new(Readiness::default())
    }
}

impl Flow {
    pub fn new(readiness: Readiness) -> Self {
        Self { state: State::Idle, readiness, jobs: BTreeMap::new(), next_id: 1 }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn readiness(&self) -> Readiness {
        self.readiness
    }

    pub fn job(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn jobs(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    pub fn handle(&mut self, event: Event) -> Vec<Effect> {
        let mut fx = Vec::new();
        match event {
            Event::Hotkey { target } => self.on_hotkey(target, &mut fx),
            Event::Stop => self.on_stop(&mut fx),
            Event::Cancel => self.on_cancel(&mut fx),
            Event::Action => self.on_action(&mut fx),
            Event::Dismiss => self.on_dismiss(&mut fx),
            Event::Transcribed { job, result } => self.on_transcribed(job, result, &mut fx),
            Event::Delivered { job, result } => self.on_delivered(job, result, &mut fx),
            Event::Readiness(r) => self.on_readiness(r, &mut fx),
        }
        fx
    }

    fn set_job(&mut self, id: JobId, state: JobState, fx: &mut Vec<Effect>) {
        if let Some(j) = self.jobs.get_mut(&id) {
            j.state = state;
            fx.push(Effect::SaveJob { job: id });
        }
    }

    fn on_hotkey(&mut self, target: Target, fx: &mut Vec<Effect>) {
        match self.state.clone() {
            State::Recording { .. } => self.on_stop(fx),
            // Busy finishing the previous dictation: ignore rather than start a second one.
            State::Transcribing { .. } | State::Delivering { .. } => {}
            _ => {
                if let Some(b) = self.readiness.blocker() {
                    self.state = State::Blocked(b);
                    fx.push(Effect::ShowStrip);
                    return;
                }
                let id = JobId(self.next_id);
                self.next_id += 1;
                self.jobs.insert(id, Job { id, target, state: JobState::Recording });
                let offline = self.readiness.network == Network::Down;
                self.state = State::Recording { job: id, offline };
                fx.extend([Effect::SaveJob { job: id }, Effect::StartCapture { job: id }, Effect::ShowStrip]);
            }
        }
    }

    fn on_stop(&mut self, fx: &mut Vec<Effect>) {
        let State::Recording { job, .. } = self.state else { return };
        fx.push(Effect::StopCapture { job });
        // Decide by the network now, not when recording started.
        if self.readiness.network == Network::Down {
            self.set_job(job, JobState::WaitingForNetwork, fx);
            self.state = State::NeedsYou(Problem::WaitingForNetwork { job });
        } else {
            self.set_job(job, JobState::Transcribing, fx);
            self.state = State::Transcribing { job };
            fx.push(Effect::Transcribe { job });
        }
    }

    fn on_cancel(&mut self, fx: &mut Vec<Effect>) {
        match self.state {
            State::Recording { job, .. } => {
                fx.push(Effect::DiscardCapture { job });
                self.set_job(job, JobState::Discarded, fx);
            }
            State::Transcribing { job } => self.set_job(job, JobState::Discarded, fx),
            _ => return,
        }
        self.state = State::Idle;
        fx.push(Effect::HideStrip { after_ms: 0 });
    }

    fn on_action(&mut self, fx: &mut Vec<Effect>) {
        match self.state.clone() {
            State::Blocked(Blocker::SignedOut) => fx.push(Effect::OpenSignIn),
            State::Blocked(Blocker::MicDenied | Blocker::MicMissing) => fx.push(Effect::OpenMicSettings),
            State::NeedsYou(p) => match p {
                Problem::SignInToSend { .. } => fx.push(Effect::OpenSignIn),
                Problem::Unreachable { job } => self.retry(job, fx),
                Problem::TargetChanged { job, .. } | Problem::PasteFailed { job } => {
                    if let Some(text) = self.ready_text(job) {
                        self.state = State::Delivering { job };
                        fx.push(Effect::PasteHere { job, text });
                    }
                }
                // The offline state's action is Discard.
                Problem::WaitingForNetwork { job } => {
                    self.set_job(job, JobState::Discarded, fx);
                    self.state = State::Idle;
                    fx.push(Effect::HideStrip { after_ms: 0 });
                }
                Problem::NoSpeech { .. } => {}
            },
            _ => {}
        }
    }

    fn on_dismiss(&mut self, fx: &mut Vec<Effect>) {
        match &self.state {
            // Dismissing keeps the job: it stays in History (and in the offline queue).
            State::Blocked(_) | State::NeedsYou(_) | State::Delivered { .. } => {
                self.state = State::Idle;
                fx.push(Effect::HideStrip { after_ms: 0 });
            }
            _ => {}
        }
    }

    fn retry(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        self.set_job(job, JobState::Transcribing, fx);
        self.state = State::Transcribing { job };
        fx.extend([Effect::ShowStrip, Effect::Transcribe { job }]);
    }

    fn ready_text(&self, job: JobId) -> Option<String> {
        match &self.jobs.get(&job)?.state {
            JobState::Ready(t) | JobState::Delivered(t) => Some(t.clone()),
            _ => None,
        }
    }

    fn on_transcribed(&mut self, job: JobId, result: Result<String, TranscribeError>, fx: &mut Vec<Effect>) {
        // A cancelled job stays cancelled; a superseded one only updates its record.
        if matches!(self.jobs.get(&job).map(|j| &j.state), Some(JobState::Discarded) | None) {
            return;
        }
        let current = matches!(self.state, State::Transcribing { job: j } if j == job);
        match result {
            Ok(text) if text.trim().is_empty() => {
                self.set_job(job, JobState::Failed(TranscribeError::Server(204)), fx);
                if current {
                    self.state = State::NeedsYou(Problem::NoSpeech { job });
                    fx.push(Effect::HideStrip { after_ms: 4000 });
                }
            }
            Ok(text) => {
                self.set_job(job, JobState::Ready(text.clone()), fx);
                if current {
                    let target = self.jobs[&job].target.clone();
                    self.state = State::Delivering { job };
                    fx.push(Effect::Paste { job, target, text });
                }
            }
            Err(e) => {
                if e.needs_sign_in() {
                    self.readiness.session = Session::Expired;
                }
                let problem = match e {
                    TranscribeError::SignedOut | TranscribeError::SessionExpired => Problem::SignInToSend { job },
                    TranscribeError::Offline => Problem::WaitingForNetwork { job },
                    TranscribeError::Blocked | TranscribeError::Timeout | TranscribeError::Server(_) => {
                        Problem::Unreachable { job }
                    }
                };
                let job_state = match &problem {
                    Problem::WaitingForNetwork { .. } => JobState::WaitingForNetwork,
                    _ => JobState::Failed(e),
                };
                self.set_job(job, job_state, fx);
                if current {
                    self.state = State::NeedsYou(problem);
                }
            }
        }
    }

    fn on_delivered(&mut self, job: JobId, result: Delivery, fx: &mut Vec<Effect>) {
        let Some(text) = self.ready_text(job) else { return };
        match result {
            Delivery::Pasted => {
                self.set_job(job, JobState::Delivered(text), fx);
                self.state = State::Delivered { job };
                fx.push(Effect::HideStrip { after_ms: 2000 });
            }
            Delivery::TargetChanged(now_in) => self.state = State::NeedsYou(Problem::TargetChanged { job, now_in }),
            Delivery::Failed => self.state = State::NeedsYou(Problem::PasteFailed { job }),
        }
    }

    fn on_readiness(&mut self, r: Readiness, fx: &mut Vec<Effect>) {
        let was = self.readiness;
        self.readiness = r;

        // Signed back in: resend the dictation that was waiting for it, into the original app.
        if was.session != Session::Valid && r.session == Session::Valid {
            match self.state.clone() {
                State::NeedsYou(Problem::SignInToSend { job }) => return self.retry(job, fx),
                State::Blocked(Blocker::SignedOut) => {
                    self.state = State::Idle;
                    fx.push(Effect::HideStrip { after_ms: 0 });
                }
                _ => {}
            }
        }

        // Back online: send everything that was waiting. The one on screen goes first.
        if was.network == Network::Down && r.network == Network::Up {
            let waiting: Vec<JobId> = self
                .jobs
                .values()
                .filter(|j| j.state == JobState::WaitingForNetwork)
                .map(|j| j.id)
                .collect();
            for job in waiting {
                if matches!(self.state, State::NeedsYou(Problem::WaitingForNetwork { job: j }) if j == job) {
                    self.retry(job, fx);
                } else {
                    self.set_job(job, JobState::Transcribing, fx);
                    fx.push(Effect::Transcribe { job });
                }
            }
        }
    }
}
