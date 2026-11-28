//! voxa-core's dictation state machine, for Kotlin. The keyboard feeds [`FlowEvent`]s in and
//! carries out the returned [`FlowEffect`]s, exactly as the desktop does; these types only mirror
//! the core's so UniFFI can carry them. The flow also keeps the saved history in step with its
//! jobs (the desktop's `sync_history`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use voxa_core::flow::Delivery;
use voxa_core::{Blocker, Effect, Event, Flow, JobId, JobState, Mic, Network, Problem, Readiness, Session, State, Target, TranscribeError};

use voxa_store::{Entry, History, Settings, Status};

use crate::{TranscribeFailure, now_ms};

/// History reasons for takes still in flight. If the process dies they are found on the next
/// start and become [`NOT_SENT`], ready for "Send now".
pub(crate) const RECORDING: &str = "Recording";
pub(crate) const WRITING: &str = "Writing it out";
pub(crate) const NOT_SENT: &str = "Not sent";

/// Where the text goes: the app (package name or label) and an opaque field identity.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiTarget {
    pub app: String,
    pub window: u64,
}

impl From<FfiTarget> for Target {
    fn from(t: FfiTarget) -> Self {
        Target { app: t.app, window: t.window }
    }
}
impl From<Target> for FfiTarget {
    fn from(t: Target) -> Self {
        FfiTarget { app: t.app, window: t.window }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SessionState {
    Valid,
    Expired,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum NetworkState {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MicState {
    Granted,
    Denied,
    Missing,
    /// The mic stopped delivering sound mid-take.
    Stopped,
    /// The recording couldn't be saved (storage full).
    CantSave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReadinessInfo {
    pub session: SessionState,
    pub network: NetworkState,
    pub mic: MicState,
}

impl From<ReadinessInfo> for Readiness {
    fn from(r: ReadinessInfo) -> Self {
        Readiness {
            session: match r.session {
                SessionState::Valid => Session::Valid,
                SessionState::Expired => Session::Expired,
                SessionState::Unknown => Session::Unknown,
            },
            network: match r.network {
                NetworkState::Up => Network::Up,
                NetworkState::Down => Network::Down,
            },
            mic: match r.mic {
                MicState::Granted => Mic::Granted,
                MicState::Denied => Mic::Denied,
                MicState::Missing => Mic::Missing,
                MicState::Stopped => Mic::Stopped,
                MicState::CantSave => Mic::CantSave,
            },
        }
    }
}

impl From<Readiness> for ReadinessInfo {
    fn from(r: Readiness) -> Self {
        ReadinessInfo {
            session: match r.session {
                Session::Valid => SessionState::Valid,
                Session::Expired => SessionState::Expired,
                Session::Unknown => SessionState::Unknown,
            },
            network: match r.network {
                Network::Up => NetworkState::Up,
                Network::Down => NetworkState::Down,
            },
            mic: match r.mic {
                Mic::Granted => MicState::Granted,
                Mic::Denied => MicState::Denied,
                Mic::Missing => MicState::Missing,
                Mic::Stopped => MicState::Stopped,
                Mic::CantSave => MicState::CantSave,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BlockerKind {
    MicDenied,
    MicMissing,
    MicStopped,
    CantSave,
    SignedOut,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ProblemInfo {
    SignInToSend { job: u64 },
    WaitingForNetwork { job: u64 },
    Unreachable { job: u64 },
    NoSpeech { job: u64 },
    TargetChanged { job: u64, now_in: FfiTarget },
    PasteFailed { job: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FlowState {
    Idle,
    Blocked { blocker: BlockerKind },
    /// `paused`: the mic is off; still the same take (Stop sends it, Cancel drops it).
    Recording { job: u64, offline: bool, paused: bool },
    Transcribing { job: u64 },
    Delivering { job: u64 },
    Delivered { job: u64 },
    NeedsYou { problem: ProblemInfo },
}

impl From<&State> for FlowState {
    fn from(s: &State) -> Self {
        match s.clone() {
            State::Idle => FlowState::Idle,
            State::Blocked(b) => FlowState::Blocked {
                blocker: match b {
                    Blocker::MicDenied => BlockerKind::MicDenied,
                    Blocker::MicMissing => BlockerKind::MicMissing,
                    Blocker::MicStopped => BlockerKind::MicStopped,
                    Blocker::CantSave => BlockerKind::CantSave,
                    Blocker::SignedOut => BlockerKind::SignedOut,
                },
            },
            State::Recording { job, offline, paused } => FlowState::Recording { job: job.0, offline, paused },
            State::Transcribing { job } => FlowState::Transcribing { job: job.0 },
            State::Delivering { job } => FlowState::Delivering { job: job.0 },
            State::Delivered { job } => FlowState::Delivered { job: job.0 },
            State::NeedsYou(p) => FlowState::NeedsYou {
                problem: match p {
                    Problem::SignInToSend { job } => ProblemInfo::SignInToSend { job: job.0 },
                    Problem::WaitingForNetwork { job } => ProblemInfo::WaitingForNetwork { job: job.0 },
                    Problem::Unreachable { job } => ProblemInfo::Unreachable { job: job.0 },
                    Problem::NoSpeech { job } => ProblemInfo::NoSpeech { job: job.0 },
                    Problem::TargetChanged { job, now_in } => ProblemInfo::TargetChanged { job: job.0, now_in: now_in.into() },
                    Problem::PasteFailed { job } => ProblemInfo::PasteFailed { job: job.0 },
                },
            },
        }
    }
}

/// How inserting the text went, reported by the keyboard.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DeliveryResult {
    Inserted,
    /// The keyboard is now in a different field or app; it did not insert.
    TargetChanged { now_in: FfiTarget },
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FlowEvent {
    /// The mic key was pressed while the user was in `target`. `private`: an incognito field;
    /// the take is never kept in History and its recording is deleted once it is done.
    MicPressed { target: FfiTarget, private: bool },
    Stop,
    Cancel,
    /// The wide key's "Pause": mic off, take kept open.
    Pause,
    /// The wide key's "Resume": mic back on, same take.
    Resume,
    /// The status line's one primary action.
    Action,
    Dismiss,
    Transcribed { job: u64, text: String },
    TranscribeFailed { job: u64, error: TranscribeFailure },
    Delivered { job: u64, result: DeliveryResult },
    Readiness { readiness: ReadinessInfo },
    Retry { job: u64 },
    Focus { target: FfiTarget },
}

impl From<FlowEvent> for Event {
    fn from(e: FlowEvent) -> Self {
        match e {
            FlowEvent::MicPressed { target, .. } => Event::Hotkey { target: target.into() },
            FlowEvent::Stop => Event::Stop,
            FlowEvent::Cancel => Event::Cancel,
            FlowEvent::Pause => Event::Pause,
            FlowEvent::Resume => Event::Resume,
            FlowEvent::Action => Event::Action,
            FlowEvent::Dismiss => Event::Dismiss,
            FlowEvent::Transcribed { job, text } => Event::Transcribed { job: JobId(job), result: Ok(text) },
            FlowEvent::TranscribeFailed { job, error } => Event::Transcribed { job: JobId(job), result: Err(error.into()) },
            FlowEvent::Delivered { job, result } => Event::Delivered {
                job: JobId(job),
                result: match result {
                    DeliveryResult::Inserted => Delivery::Pasted,
                    DeliveryResult::TargetChanged { now_in } => Delivery::TargetChanged(now_in.into()),
                    DeliveryResult::Failed => Delivery::Failed,
                },
            },
            FlowEvent::Readiness { readiness } => Event::Readiness(readiness.into()),
            FlowEvent::Retry { job } => Event::Retry { job: JobId(job) },
            FlowEvent::Focus { target } => Event::Focus { target: target.into() },
        }
    }
}

/// Work for the keyboard to do.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FlowEffect {
    ShowStatus,
    /// Return to the idle bar after the delay (0 = now). The keyboard sends `Dismiss` then, if the
    /// state has not changed.
    HideStatus { after_ms: u32 },
    StartCapture { job: u64, pcm_path: String },
    StopCapture { job: u64 },
    /// Close the mic but keep the take's file; nothing is recorded until `ResumeCapture`.
    PauseCapture { job: u64 },
    /// Reopen the mic and append to the same file, after ~300 ms of silence.
    ResumeCapture { job: u64 },
    DiscardCapture { job: u64 },
    Transcribe { job: u64, pcm_path: String, app: String },
    /// Insert into the original field; the keyboard must check it is still the one in front.
    Paste { job: u64, target: FfiTarget, text: String },
    /// Insert into whatever field is in front now (the user chose "Insert in X").
    PasteHere { job: u64, text: String },
    OpenSignIn,
    OpenMicSettings,
    /// Storage is full: open the app's settings (history, recordings).
    OpenStorage,
}

/// The words the keyboard's status line shows.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StatusText {
    pub title: String,
    pub detail: Option<String>,
    pub action: Option<String>,
    pub dismissable: bool,
}

/// What the last dictation pasted (see [`VoxaFlow::delivered`]).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DeliveredInfo {
    pub app: String,
    pub words: u32,
    /// How long the user talked.
    pub secs: f32,
    /// Seconds saved versus typing at 40 words a minute; 0 when not meaningful.
    pub saved_secs: u32,
}

#[derive(Clone, Debug, Default)]
struct JobMeta {
    /// The history entry's id (ms when recording started).
    entry: u64,
    secs: f32,
    pcm: String,
    private: bool,
}

struct Inner {
    flow: Flow,
    dir: PathBuf,
    meta: BTreeMap<JobId, JobMeta>,
}

/// The dictation flow for the keyboard. One per keyboard service.
#[derive(uniffi::Object)]
pub struct VoxaFlow {
    inner: Mutex<Inner>,
}

#[uniffi::export]
impl VoxaFlow {
    /// `data_dir`: the app's files folder (recordings and history live there). Creating the flow
    /// also recovers takes a previous process left half done (recording or being written out
    /// when it died): they become "Not sent" in History, with their audio.
    #[uniffi::constructor]
    pub fn new(data_dir: String, readiness: ReadinessInfo) -> Arc<Self> {
        let dir = PathBuf::from(data_dir);
        recover_interrupted(&dir);
        Arc::new(Self { inner: Mutex::new(Inner { flow: Flow::new(readiness.into()), dir, meta: BTreeMap::new() }) })
    }

    pub fn handle(&self, event: FlowEvent) -> Vec<FlowEffect> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let inner = &mut *g;
        let private = matches!(event, FlowEvent::MicPressed { private: true, .. });
        let described = crate::diag::event(&event);
        let effects = inner.flow.handle(event.into());
        let mut out = Vec::new();
        for e in effects {
            let fx = match e {
                Effect::ShowStrip => FlowEffect::ShowStatus,
                Effect::HideStrip { after_ms } => FlowEffect::HideStatus { after_ms },
                Effect::StartCapture { job } => {
                    let entry = now_ms();
                    let pcm = inner.dir.join("audio").join(format!("{entry}-{}.pcm", job.0));
                    let _ = std::fs::create_dir_all(inner.dir.join("audio"));
                    let pcm = pcm.to_string_lossy().to_string();
                    inner.meta.insert(job, JobMeta { entry, secs: 0.0, pcm: pcm.clone(), private });
                    FlowEffect::StartCapture { job: job.0, pcm_path: pcm }
                }
                Effect::StopCapture { job } => FlowEffect::StopCapture { job: job.0 },
                Effect::PauseCapture { job } => FlowEffect::PauseCapture { job: job.0 },
                Effect::ResumeCapture { job } => FlowEffect::ResumeCapture { job: job.0 },
                Effect::DiscardCapture { job } => {
                    if let Some(m) = inner.meta.get(&job) {
                        let _ = std::fs::remove_file(&m.pcm);
                    }
                    FlowEffect::DiscardCapture { job: job.0 }
                }
                Effect::Transcribe { job } => FlowEffect::Transcribe {
                    job: job.0,
                    pcm_path: inner.meta.get(&job).map(|m| m.pcm.clone()).unwrap_or_default(),
                    app: inner.flow.job(job).map(|j| j.target.app.clone()).unwrap_or_default(),
                },
                Effect::Paste { job, target, text } => FlowEffect::Paste { job: job.0, target: target.into(), text },
                Effect::PasteHere { job, text } => FlowEffect::PasteHere { job: job.0, text },
                Effect::OpenSignIn => FlowEffect::OpenSignIn,
                Effect::OpenMicSettings => FlowEffect::OpenMicSettings,
                Effect::OpenStorage => FlowEffect::OpenStorage,
                // History is kept in step below, after every event.
                Effect::SaveJob { .. } => continue,
            };
            out.push(fx);
        }
        inner.sync_history();
        crate::diag::log(|| {
            let fx: Vec<String> = out.iter().map(crate::diag::effect).collect();
            format!("flow: {described} -> {} | effects [{}]", crate::diag::state(&FlowState::from(inner.flow.state())), fx.join(", "))
        });
        out
    }

    pub fn state(&self) -> FlowState {
        FlowState::from(self.inner.lock().unwrap_or_else(|e| e.into_inner()).flow.state())
    }

    pub fn readiness(&self) -> ReadinessInfo {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).flow.readiness().into()
    }

    /// How long the user talked, for History (set when the keyboard stops capturing).
    pub fn set_recording_secs(&self, job: u64, secs: f32) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(m) = g.meta.get_mut(&JobId(job)) {
            m.secs = secs;
        }
        g.sync_history();
    }

    /// What the status line says now; `None` when the keyboard shows its idle bar.
    pub fn status(&self) -> Option<StatusText> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        status_text(&g.flow)
    }

    /// While the status line says "Pasted in <app>": what was pasted, for the keyboard's success
    /// state ("11 words", "That's 6 seconds of typing saved"). `None` in any other state.
    pub fn delivered(&self) -> Option<DeliveredInfo> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let State::Delivered { job } = g.flow.state() else { return None };
        let j = g.flow.job(*job)?;
        let JobState::Delivered(text) = &j.state else { return None };
        let words = voxa_store::word_count(text);
        let secs = g.meta.get(job).map(|m| m.secs).unwrap_or(0.0);
        Some(DeliveredInfo {
            app: j.target.app.clone(),
            words: words as u32,
            secs,
            saved_secs: if secs > 0.0 { voxa_store::typing_saved_secs(words, secs) } else { 0 },
        })
    }

    /// Whether a history entry belongs to a dictation the flow is still handling.
    pub fn is_live(&self, entry_id: u64) -> bool {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.meta.iter().any(|(job, m)| m.entry == entry_id && g.flow.job(*job).is_some_and(|j| !matches!(j.state, JobState::Delivered(_) | JobState::Discarded)))
    }
}

/// voxa-core's strip wording, unchanged except where it is false on a phone: there is no
/// clipboard to restore after inserting, and the mic is allowed in Android's settings.
pub fn status_text(flow: &Flow) -> Option<StatusText> {
    let t = voxa_core::copy::strip_text(flow)?;
    let mut st = StatusText { title: t.title, detail: t.detail, action: t.action, dismissable: t.dismissable };
    match flow.state() {
        State::Blocked(Blocker::MicDenied) => st.detail = Some("Allow it in Android settings".into()),
        State::Blocked(Blocker::MicMissing) => st.action = None,
        State::Delivered { .. } => st.detail = None,
        _ => {}
    }
    Some(st)
}

/// The wide key's label during a take: "Pause" while listening, "Resume" while paused.
#[uniffi::export]
pub fn pause_key_label(paused: bool) -> String {
    if paused { voxa_core::copy::RESUME_LABEL } else { voxa_core::copy::PAUSE_LABEL }.to_string()
}

/// Takes left recording or being written out by a process that died become "Not sent".
fn recover_interrupted(dir: &std::path::Path) {
    History::edit(dir, |h| {
        let mut changed = false;
        for e in h.entries.iter_mut() {
            if matches!(&e.status, Status::Waiting(r) if r == RECORDING || r == WRITING) {
                e.status = Status::Waiting(NOT_SENT.into());
                changed = true;
            }
        }
        (changed, ())
    });
}

impl Inner {
    /// Mirror this run's jobs into the saved history, as the desktop does, but from the first
    /// moment of recording (a keyboard's process can die at any time).
    fn sync_history(&mut self) {
        let jobs: Vec<voxa_core::Job> = self.flow.jobs().cloned().collect();
        let meta = self.meta.clone();
        let keep_days = Settings::load(&self.dir).keep_days;
        History::edit(&self.dir, |h| {
            let mut changed = false;
            for job in jobs {
                let Some(m) = meta.get(&job.id) else { continue };
                if m.private {
                    // Incognito field: nothing kept. The recording goes once the take is done.
                    if matches!(job.state, JobState::Delivered(_) | JobState::Discarded | JobState::Failed(_)) {
                        let _ = std::fs::remove_file(&m.pcm);
                    }
                    continue;
                }
                let entry = |text: String, status: Status| Entry {
                    id: m.entry,
                    app: job.target.app.clone(),
                    text,
                    secs: m.secs,
                    status,
                    audio: Some(PathBuf::from(&m.pcm)),
                    started_in: job.started_in.as_ref().map(|t| t.app.clone()),
                    via: None,
                };
                let next = match &job.state {
                    JobState::Recording => Some(entry(String::new(), Status::Waiting(RECORDING.into()))),
                    JobState::Transcribing => Some(entry(String::new(), Status::Waiting(WRITING.into()))),
                    JobState::WaitingForNetwork => Some(entry(String::new(), Status::Waiting("Sends when you're back online".into()))),
                    JobState::Ready(t) | JobState::Delivered(t) if t.trim().is_empty() => None,
                    JobState::Ready(t) => Some(entry(t.clone(), Status::Ready)),
                    JobState::Delivered(t) => Some(entry(t.clone(), Status::Pasted)),
                    JobState::Failed(e) if e.needs_sign_in() => Some(entry(String::new(), Status::Waiting("Sign in to send".into()))),
                    // No speech: nothing worth keeping.
                    JobState::Failed(TranscribeError::Server(204)) => None,
                    JobState::Failed(_) => Some(entry(String::new(), Status::Failed("Couldn't reach ChatGPT".into()))),
                    JobState::Discarded => None,
                };
                changed |= match next {
                    Some(e) => h.upsert(e),
                    None => {
                        // Nothing to keep (cancelled, or no speech): the recording goes too.
                        if matches!(job.state, JobState::Discarded | JobState::Failed(TranscribeError::Server(204))) {
                            let _ = std::fs::remove_file(&m.pcm);
                        }
                        h.remove(m.entry)
                    }
                };
            }
            changed |= h.prune(keep_days, now_ms()) > 0;
            (changed, ())
        });
    }
}
