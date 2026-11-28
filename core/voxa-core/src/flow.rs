//! The dictation state machine. Pure: events in, effects out.

use std::collections::BTreeMap;

use crate::error::TranscribeError;
use crate::job::{Job, JobId, JobState, Target};
use crate::readiness::{Blocker, Mic, Network, Readiness, Session};

/// What the strip (desktop) or keyboard bar (Android) shows. One state at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    /// The hotkey was pressed but recording did not start. Nothing was recorded.
    Blocked(Blocker),
    /// `paused`: the mic is off and paused time is left out of the take. Still one take: every
    /// "in a take" check (Stop, Cancel, Hotkey, Retry's busy guard) treats it as recording.
    Recording { job: JobId, offline: bool, paused: bool },
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
    /// Turn the mic off mid-take without ending it (someone is talking to the user).
    Pause,
    /// Turn the mic back on and keep adding to the same take.
    Resume,
    /// The strip's one primary action.
    Action,
    /// The strip's close button, or Esc on a needs-you state.
    Dismiss,
    Transcribed { job: JobId, result: Result<String, TranscribeError> },
    Delivered { job: JobId, result: Delivery },
    Readiness(Readiness),
    /// Send a waiting or failed dictation again now (History's "Send now"). While the strip is
    /// busy with another take it goes out in the background instead of doing nothing.
    Retry { job: JobId },
    /// Write a finished dictation out again from its recording (History's "Write it out
    /// again"). Always in the background, never pasted; if it fails, the old text stays.
    Rewrite { job: JobId },
    /// The user corrected a dictation's text in History. One not pasted yet pastes the
    /// corrected text.
    Edit { job: JobId, text: String },
    /// The shell's answer to a `Transcribe` whose recording has nothing usable (the mic gave no
    /// sound, or the file couldn't be written). Acts on that job only, never on another take.
    MicTrouble { job: JobId, mic: Mic },
    /// The app in front changed while the strip asks where to paste.
    Focus { target: Target },
}

/// Work for the shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    ShowStrip,
    /// Hide after the given delay (0 = now).
    HideStrip { after_ms: u32 },
    StartCapture { job: JobId },
    StopCapture { job: JobId },
    /// Close the mic but keep the take open; paused time is left out of the recording.
    PauseCapture { job: JobId },
    /// Reopen the mic and keep writing to the same take (after a short silence).
    ResumeCapture { job: JobId },
    DiscardCapture { job: JobId },
    Transcribe { job: JobId },
    /// Paste into the original target; the shell must check it is still in front.
    Paste { job: JobId, target: Target, text: String },
    /// Paste into whatever is in front now (the user chose "Paste in X").
    PasteHere { job: JobId, text: String },
    OpenSignIn,
    OpenMicSettings,
    /// Open Voxa's settings at storage (the disk is full).
    OpenStorage,
    /// Persist the job (history, retry, offline queue).
    SaveJob { job: JobId },
}

pub struct Flow {
    state: State,
    readiness: Readiness,
    jobs: BTreeMap<JobId, Job>,
    next_id: u64,
    /// What a kept dictation (waiting or failed) was before a retry, so cancelling the retry
    /// keeps it instead of throwing the recording away.
    kept: BTreeMap<JobId, JobState>,
    /// Jobs whose upload the shell is running now (a `Transcribe` effect without its
    /// `Transcribed` answer yet). A Retry while one runs waits for that answer instead of
    /// starting a second upload of the same recording.
    uploading: std::collections::BTreeSet<JobId>,
}

impl Default for Flow {
    fn default() -> Self {
        Self::new(Readiness::default())
    }
}

impl Flow {
    pub fn new(readiness: Readiness) -> Self {
        Self { state: State::Idle, readiness, jobs: BTreeMap::new(), next_id: 1, kept: BTreeMap::new(), uploading: Default::default() }
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
            Event::Pause => self.on_pause(true, &mut fx),
            Event::Resume => self.on_pause(false, &mut fx),
            Event::Action => self.on_action(&mut fx),
            Event::Dismiss => self.on_dismiss(&mut fx),
            Event::Transcribed { job, result } => self.on_transcribed(job, result, &mut fx),
            Event::Delivered { job, result } => self.on_delivered(job, result, &mut fx),
            Event::Readiness(r) => self.on_readiness(r, &mut fx),
            Event::Retry { job } => self.on_retry(job, &mut fx),
            Event::Rewrite { job } => self.on_rewrite(job, &mut fx),
            Event::Edit { job, text } => self.on_edit(job, text, &mut fx),
            Event::MicTrouble { job, mic } => self.on_mic_trouble(job, mic, &mut fx),
            Event::Focus { target } => self.on_focus(target, &mut fx),
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
            // Pasting takes a moment: ignore. A long upload never blocks a new take: it goes on
            // in the background and its text waits in History.
            State::Delivering { .. } => {}
            _ => {
                if let Some(b) = self.readiness.blocker() {
                    self.state = State::Blocked(b);
                    fx.push(Effect::ShowStrip);
                    return;
                }
                let id = JobId(self.next_id);
                self.next_id += 1;
                self.jobs.insert(id, Job { id, target, started_in: None, state: JobState::Recording });
                let offline = self.readiness.network == Network::Down;
                self.state = State::Recording { job: id, offline, paused: false };
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
            self.transcribe(job, fx);
        }
    }

    /// Pause and Resume only flip the flag of the take on screen; repeating one does nothing.
    /// The job stays Recording either way, so crash recovery keeps the audio.
    fn on_pause(&mut self, pause: bool, fx: &mut Vec<Effect>) {
        let State::Recording { job, offline, paused } = self.state else { return };
        if paused == pause {
            return;
        }
        self.state = State::Recording { job, offline, paused: pause };
        fx.push(if pause { Effect::PauseCapture { job } } else { Effect::ResumeCapture { job } });
    }

    fn on_cancel(&mut self, fx: &mut Vec<Effect>) {
        match self.state {
            State::Recording { job, .. } => {
                fx.push(Effect::DiscardCapture { job });
                self.set_job(job, JobState::Discarded, fx);
            }
            // A fresh take is dropped; a retry of a kept one goes back to being kept.
            State::Transcribing { job } => {
                let back = self.kept.remove(&job).unwrap_or(JobState::Discarded);
                self.set_job(job, back, fx);
            }
            _ => return,
        }
        self.state = State::Idle;
        fx.push(Effect::HideStrip { after_ms: 0 });
    }

    fn on_action(&mut self, fx: &mut Vec<Effect>) {
        match self.state.clone() {
            State::Blocked(Blocker::SignedOut) => fx.push(Effect::OpenSignIn),
            State::Blocked(Blocker::MicDenied | Blocker::MicMissing) => fx.push(Effect::OpenMicSettings),
            State::Blocked(Blocker::CantSave) => fx.push(Effect::OpenStorage),
            State::Blocked(Blocker::MicStopped) => {}
            State::NeedsYou(p) => match p.clone() {
                Problem::SignInToSend { .. } => fx.push(Effect::OpenSignIn),
                Problem::Unreachable { job } => self.retry(job, fx),
                Problem::TargetChanged { job, .. } | Problem::PasteFailed { job } => {
                    // "Paste in X": the text now belongs to X. Remember where it started.
                    if let Problem::TargetChanged { now_in, .. } = &p {
                        if let Some(j) = self.jobs.get_mut(&job) {
                            let started = std::mem::replace(&mut j.target, now_in.clone());
                            j.started_in.get_or_insert(started);
                            fx.push(Effect::SaveJob { job });
                        }
                    }
                    if let Some(text) = self.ready_text(job) {
                        self.state = State::Delivering { job };
                        fx.push(Effect::PasteHere { job, text });
                    }
                }
                // Offline: "Retry now" tries at once (a dropped connection often isn't offline).
                // The close button keeps it waiting; nothing is thrown away from the strip.
                Problem::WaitingForNetwork { job } => self.retry(job, fx),
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

    /// While asking "you moved to X, paste there?": going back to where the dictation started
    /// pastes it there (no question needed); going somewhere else updates the question.
    fn on_focus(&mut self, target: Target, fx: &mut Vec<Effect>) {
        let State::NeedsYou(Problem::TargetChanged { job, now_in }) = self.state.clone() else { return };
        let Some(original) = self.jobs.get(&job).map(|j| j.target.clone()) else { return };
        if target.window == original.window {
            if let Some(text) = self.ready_text(job) {
                self.state = State::Delivering { job };
                fx.push(Effect::Paste { job, target: original, text });
            }
        } else if target != now_in {
            self.state = State::NeedsYou(Problem::TargetChanged { job, now_in: target });
        }
    }

    /// History's "Send now": only for a dictation that is waiting or failed. Never over one the
    /// user is recording or that is being written out: then it goes out in the background.
    fn on_retry(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        let retryable = matches!(self.jobs.get(&job).map(|j| &j.state), Some(JobState::WaitingForNetwork | JobState::Failed(_)));
        let busy = matches!(self.state, State::Recording { .. } | State::Transcribing { .. } | State::Delivering { .. });
        if !retryable {
            return;
        }
        if busy {
            self.send_in_background(job, fx);
        } else {
            self.retry(job, fx);
        }
    }

    /// "Write it out again" for a dictation that already has text. Never the one on the strip.
    fn on_rewrite(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        let finished = matches!(self.jobs.get(&job).map(|j| &j.state), Some(JobState::Ready(_) | JobState::Delivered(_)));
        if finished && self.strip_job() != Some(job) {
            self.send_in_background(job, fx);
        }
    }

    fn on_edit(&mut self, job: JobId, text: String, fx: &mut Vec<Effect>) {
        let next = match self.jobs.get(&job).map(|j| &j.state) {
            Some(JobState::Ready(_)) => JobState::Ready(text),
            Some(JobState::Delivered(_)) => JobState::Delivered(text),
            _ => return,
        };
        self.set_job(job, next, fx);
    }

    /// The recording of `job` has nothing usable. Only that job changes, and the strip only when
    /// it shows that job.
    fn on_mic_trouble(&mut self, job: JobId, mic: Mic, fx: &mut Vec<Effect>) {
        self.uploading.remove(&job);
        if !matches!(self.jobs.get(&job).map(|j| &j.state), Some(JobState::Transcribing)) {
            return;
        }
        let back = self.kept.remove(&job).unwrap_or(JobState::Discarded);
        self.set_job(job, back, fx);
        if self.strip_job() == Some(job) {
            self.readiness.mic = mic;
            self.state = State::Blocked(self.readiness.blocker().unwrap_or(Blocker::MicStopped));
            fx.push(Effect::ShowStrip);
        }
    }

    /// Upload a job without putting it on the strip; if that fails, what it was is kept.
    fn send_in_background(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        if let Some(before) = self.jobs.get(&job).map(|j| j.state.clone()) {
            self.kept.insert(job, before);
        }
        self.set_job(job, JobState::Transcribing, fx);
        self.transcribe(job, fx);
    }

    /// The job the strip shows now, if any.
    fn strip_job(&self) -> Option<JobId> {
        match &self.state {
            State::Recording { job, .. } | State::Transcribing { job } | State::Delivering { job } | State::Delivered { job } => Some(*job),
            State::NeedsYou(p) => Some(p.job()),
            State::Idle | State::Blocked(_) => None,
        }
    }

    /// Whether a job is still being worked on: recording, being written out or uploaded, or on
    /// the strip. A finished one (ready, failed, no speech, dismissed) is not, so it can be
    /// deleted like any other dictation.
    pub fn in_progress(&self, job: JobId) -> bool {
        let Some(j) = self.jobs.get(&job) else { return false };
        matches!(j.state, JobState::Recording | JobState::Transcribing) || self.uploading.contains(&job) || self.strip_job() == Some(job)
    }

    fn retry(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        if let Some(before) = self.jobs.get(&job).map(|j| j.state.clone()) {
            if matches!(before, JobState::WaitingForNetwork | JobState::Failed(_)) {
                self.kept.insert(job, before);
            }
        }
        self.set_job(job, JobState::Transcribing, fx);
        self.state = State::Transcribing { job };
        fx.push(Effect::ShowStrip);
        self.transcribe(job, fx);
    }

    /// Ask the shell to upload a job, unless an upload of it is still running: then its answer
    /// (which the job now waits for again) settles it, and one recording is never sent twice.
    fn transcribe(&mut self, job: JobId, fx: &mut Vec<Effect>) {
        if self.uploading.insert(job) {
            fx.push(Effect::Transcribe { job });
        }
    }

    /// Whether the shell is still uploading this job.
    pub fn is_uploading(&self, job: JobId) -> bool {
        self.uploading.contains(&job)
    }

    fn ready_text(&self, job: JobId) -> Option<String> {
        match &self.jobs.get(&job)?.state {
            JobState::Ready(t) | JobState::Delivered(t) => Some(t.clone()),
            _ => None,
        }
    }

    fn on_transcribed(&mut self, job: JobId, result: Result<String, TranscribeError>, fx: &mut Vec<Effect>) {
        self.uploading.remove(&job);
        match self.jobs.get(&job).map(|j| &j.state) {
            Some(JobState::Transcribing) => {}
            // A retry the user cancelled, whose upload still came back with the text: keep the
            // text (never pasted), so the recording isn't uploaded again for nothing.
            Some(JobState::WaitingForNetwork | JobState::Failed(_)) => {
                if let Some(text) = result.ok().filter(|t| !t.trim().is_empty()) {
                    self.set_job(job, JobState::Ready(text), fx);
                }
                return;
            }
            // A cancelled job stays cancelled; a finished one keeps its text.
            _ => return,
        }
        let kept = self.kept.remove(&job);
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
            // Writing a finished dictation out again failed: it keeps the text it had.
            Err(_) if matches!(kept, Some(JobState::Ready(_) | JobState::Delivered(_))) => {
                self.set_job(job, kept.unwrap_or(JobState::Discarded), fx);
            }
            Err(e) => {
                if e.needs_sign_in() {
                    self.readiness.session = Session::Expired;
                }
                let problem = match e {
                    TranscribeError::SignedOut | TranscribeError::SessionExpired => Problem::SignInToSend { job },
                    TranscribeError::Offline => {
                        // Remember we're offline, so the network coming back resends it.
                        self.readiness.network = Network::Down;
                        Problem::WaitingForNetwork { job }
                    }
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
        // Others that waited for the sign-in go out in the background. Never return early: the
        // same check can also say the network is back.
        if was.session != Session::Valid && r.session == Session::Valid {
            match self.state.clone() {
                State::NeedsYou(Problem::SignInToSend { job }) => self.retry(job, fx),
                State::Blocked(Blocker::SignedOut) => {
                    self.state = State::Idle;
                    fx.push(Effect::HideStrip { after_ms: 0 });
                }
                _ => {}
            }
            let waiting: Vec<JobId> = self.jobs.values().filter(|j| matches!(&j.state, JobState::Failed(e) if e.needs_sign_in())).map(|j| j.id).collect();
            for job in waiting {
                self.send_in_background(job, fx);
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
                    self.transcribe(job, fx);
                }
            }
        }
    }
}
