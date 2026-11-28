//! voxa-core's dictation state machine, for Kotlin. The keyboard feeds [`FlowEvent`]s in and
//! carries out the returned [`FlowEffect`]s, exactly as the desktop does; these types only mirror
//! the core's so UniFFI can carry them. The flow also keeps the saved history in step with its
//! jobs (the desktop's `sync_history`).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use voxa_core::flow::Delivery;
use voxa_core::{Blocker, Effect, Event, Flow, JobId, JobState, Mic, Network, Problem, Readiness, Session, State, Target, TranscribeError};

use voxa_store::{Entry, History, Status};

use crate::{TranscribeFailure, now_ms};

/// History reasons for takes still in flight. If the process dies they are found on the next
/// start: a take cut off while recording becomes [`NOT_SENT`], ready for "Send now"; one cut off
/// while it was being written out goes back to waiting, so the background send finishes it.
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
    /// The words that came back. Empty text is never taken as "no speech" (it is an answer
    /// without a transcript, a server problem): the recording is kept, to send again.
    Transcribed { job: u64, text: String },
    /// The device's own check found the recording silent: nothing is kept.
    NoSpeech { job: u64 },
    TranscribeFailed { job: u64, error: TranscribeFailure },
    /// The keyboard didn't run the upload it was asked for, or lost it (an exception): the take
    /// goes back to waiting or failed. Every `Transcribe` ends in an answer or this.
    UploadSkipped { job: u64 },
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
            FlowEvent::Transcribed { job, text } if text.trim().is_empty() => Event::Transcribed { job: JobId(job), result: Err(TranscribeError::Server(0)) },
            FlowEvent::Transcribed { job, text } => Event::Transcribed { job: JobId(job), result: Ok(text) },
            FlowEvent::NoSpeech { job } => Event::Transcribed { job: JobId(job), result: Ok(String::new()) },
            FlowEvent::UploadSkipped { job } => Event::UploadSkipped { job: JobId(job) },
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
    /// Upload the recording with `VoxaCore::transcribe_take(entry, ...)`, then send the answer
    /// back (`Transcribed`, `NoSpeech`, `TranscribeFailed`, or `UploadSkipped` if it never ran).
    /// `entry`: the History entry (0 for an incognito take); the core writes the result there
    /// itself, so it is kept even if the keyboard goes away meanwhile.
    Transcribe { job: u64, pcm_path: String, app: String, entry: u64 },
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
    /// This flow holds the entry's send claim for its upload (see [`crate::claims`]).
    claimed: bool,
    /// An incognito take's recording was deleted.
    audio_gone: bool,
}

struct Inner {
    flow: Flow,
    dir: PathBuf,
    meta: BTreeMap<JobId, JobMeta>,
    /// The entry as this flow last wrote it, by entry id. An entry that differs was changed by
    /// someone else (History's edit, delete or Send now, the background send): the flow never
    /// writes its older view over that.
    written: Arc<Mutex<HashMap<u64, Entry>>>,
}

/// What to do with an upload the flow asks for.
enum Upload {
    Go { entry: u64 },
    /// History already has the words (sent from History or in the background meanwhile).
    Adopt(String),
    /// Deleted, or being sent by someone else right now.
    Skip,
}

/// The dictation flow for the keyboard. One per keyboard service.
#[derive(uniffi::Object)]
pub struct VoxaFlow {
    inner: Mutex<Inner>,
}

#[uniffi::export]
impl VoxaFlow {
    /// `data_dir`: the app's files folder (recordings and history live there). Creating the flow
    /// also recovers takes a previous process left half done (see [`RECORDING`]), and deletes
    /// recordings no History entry keeps (incognito takes cut off by a process death).
    #[uniffi::constructor]
    pub fn new(data_dir: String, readiness: ReadinessInfo) -> Arc<Self> {
        let dir = PathBuf::from(data_dir);
        recover_interrupted(&dir);
        sweep_orphan_audio(&dir);
        Arc::new(Self {
            inner: Mutex::new(Inner { flow: Flow::new(readiness.into()), dir, meta: BTreeMap::new(), written: Arc::new(Mutex::new(HashMap::new())) }),
        })
    }

    /// Carry out one event. History is kept in step for the jobs this event changed only, on a
    /// writer thread (the keyboard's main thread never waits for the disk).
    pub fn handle(&self, event: FlowEvent) -> Vec<FlowEffect> {
        let line;
        let out;
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let inner = &mut *g;
            let private = matches!(event, FlowEvent::MicPressed { private: true, .. });
            let described = crate::diag::event(&event);
            let mut queue: VecDeque<Event> = VecDeque::from([event.into()]);
            let mut effects_out = Vec::new();
            let mut dirty: BTreeSet<JobId> = BTreeSet::new();
            while let Some(ev) = queue.pop_front() {
                // The answer to this flow's upload: its claim on the entry ends.
                if let Event::Transcribed { job, .. } | Event::UploadSkipped { job } = &ev {
                    inner.release_claim(*job);
                }
                for e in inner.flow.handle(ev) {
                    let fx = match e {
                        Effect::ShowStrip => FlowEffect::ShowStatus,
                        Effect::HideStrip { after_ms } => FlowEffect::HideStatus { after_ms },
                        Effect::StartCapture { job } => {
                            let entry = unique_entry_id();
                            let pcm = inner.dir.join("audio").join(format!("{entry}-{}.pcm", job.0));
                            let _ = std::fs::create_dir_all(inner.dir.join("audio"));
                            let pcm = pcm.to_string_lossy().to_string();
                            inner.meta.insert(job, JobMeta { entry, pcm: pcm.clone(), private, ..JobMeta::default() });
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
                        Effect::Transcribe { job } => match inner.before_upload(job) {
                            Upload::Go { entry } => FlowEffect::Transcribe {
                                job: job.0,
                                pcm_path: inner.meta.get(&job).map(|m| m.pcm.clone()).unwrap_or_default(),
                                app: inner.flow.job(job).map(|j| j.target.app.clone()).unwrap_or_default(),
                                entry,
                            },
                            Upload::Adopt(text) => {
                                queue.push_back(Event::Transcribed { job, result: Ok(text) });
                                continue;
                            }
                            Upload::Skip => {
                                queue.push_back(Event::UploadSkipped { job });
                                continue;
                            }
                        },
                        Effect::Paste { job, target, text } => FlowEffect::Paste { job: job.0, target: target.into(), text },
                        Effect::PasteHere { job, text } => FlowEffect::PasteHere { job: job.0, text },
                        Effect::OpenSignIn => FlowEffect::OpenSignIn,
                        Effect::OpenMicSettings => FlowEffect::OpenMicSettings,
                        Effect::OpenStorage => FlowEffect::OpenStorage,
                        Effect::SaveJob { job } => {
                            dirty.insert(job);
                            continue;
                        }
                    };
                    effects_out.push(fx);
                }
            }
            inner.sync_history(&dirty);
            inner.tidy_private_audio();
            let fx: Vec<String> = effects_out.iter().map(crate::diag::effect).collect();
            line = format!("flow: {described} -> {} | effects [{}]", crate::diag::state(&FlowState::from(inner.flow.state())), fx.join(", "));
            out = effects_out;
        }
        // Logged after the lock is released: a log sink that calls back into the flow can't
        // deadlock it.
        crate::diag::log(|| line);
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
        g.sync_history(&BTreeSet::from([JobId(job)]));
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

    /// The History entry a dictation is kept as, or None for an incognito take (never kept,
    /// never notified about).
    pub fn entry_for_job(&self, job: u64) -> Option<u64> {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // A cancelled take has no entry any more.
        if g.flow.job(JobId(job)).is_some_and(|j| matches!(j.state, JobState::Discarded)) {
            return None;
        }
        g.meta.get(&JobId(job)).filter(|m| !m.private).map(|m| m.entry)
    }

    /// Whether the keyboard is working on this entry right now: recording it or writing it out.
    /// Anything else (waiting for the network, failed, ready) may be sent by the background send
    /// or History; the send claim keeps any two senders from uploading it at once.
    pub fn owns(&self, entry_id: u64) -> bool {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.meta.iter().any(|(job, m)| m.entry == entry_id && g.flow.job(*job).is_some_and(|j| matches!(j.state, JobState::Recording | JobState::Transcribing)))
    }

    /// Whether a history entry belongs to a dictation the flow is still working on.
    pub fn is_live(&self, entry_id: u64) -> bool {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.meta.iter().any(|(job, m)| m.entry == entry_id && g.flow.is_in_progress(*job))
    }
}

/// voxa-core's strip wording, unchanged except where it is false on a phone: there is no
/// clipboard to restore after inserting, the mic is allowed in Android's settings, and it is the
/// phone's storage that is full.
pub fn status_text(flow: &Flow) -> Option<StatusText> {
    let t = voxa_core::copy::strip_text(flow)?;
    let mut st = StatusText { title: t.title, detail: t.detail, action: t.action, dismissable: t.dismissable };
    match flow.state() {
        State::Blocked(Blocker::MicDenied) => st.detail = Some("Allow it in Android settings".into()),
        State::Blocked(Blocker::MicMissing) => st.action = None,
        State::Blocked(Blocker::CantSave) => st.detail = Some("Your phone's storage is full".into()),
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

/// Takes left half done by a process that died: one cut off while recording becomes "Not sent";
/// one that was being written out (the user had pressed Stop) waits for the background send.
fn recover_interrupted(dir: &std::path::Path) {
    History::edit(dir, |h| {
        let mut changed = false;
        for e in h.entries.iter_mut() {
            let next = match &e.status {
                Status::Waiting(r) if r == RECORDING => NOT_SENT,
                Status::Waiting(r) if r == WRITING => crate::notices::WAITING_FOR_NETWORK,
                _ => continue,
            };
            e.status = Status::Waiting(next.into());
            changed = true;
        }
        (changed, ())
    });
}

/// Delete recordings in `audio/` that no History entry keeps. Only incognito takes are never
/// in History; if the process died during one, its audio is found and deleted here. Nothing is
/// deleted while History can't be read cleanly, a recording made before History was found
/// damaged is kept, and so is one an upload is reading now (the keyboard restarted mid-upload).
fn sweep_orphan_audio(dir: &std::path::Path) {
    let Ok(files) = std::fs::read_dir(dir.join("audio")) else { return };
    let names: Vec<(std::ffi::OsString, PathBuf)> = files
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().ends_with(".pcm"))
        .filter(|f| !crate::claims::is_being_read(&f.path()))
        .map(|f| (f.file_name(), f.path()))
        .collect();
    if names.is_empty() {
        return;
    }
    let doomed: Vec<PathBuf> = History::edit(dir, |h| {
        let kept: std::collections::HashSet<std::ffi::OsString> =
            h.entries.iter().filter_map(|e| e.audio.as_ref()?.file_name().map(|n| n.to_os_string())).collect();
        let doomed = names.into_iter().filter(|(n, _)| !kept.contains(n) && h.may_delete_unlisted_audio(&n.to_string_lossy())).map(|(_, p)| p).collect();
        (false, doomed)
    });
    let removed = doomed.iter().filter(|p| std::fs::remove_file(p).is_ok()).count();
    if removed > 0 {
        crate::diag::log(|| format!("deleted {removed} recordings no dictation keeps"));
    }
}

impl Drop for VoxaFlow {
    /// The keyboard is going away: what it queued for History is written now.
    fn drop(&mut self) {
        History::flush();
    }
}

/// A new History entry's id: the ms recording started, made unique in this process (two takes
/// in one millisecond would share an entry and a recording).
fn unique_entry_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now_ms();
    let prev = LAST.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| Some(last.max(now.saturating_sub(1)) + 1)).unwrap_or(0);
    prev.max(now.saturating_sub(1)) + 1
}

/// Whether the flow's view of an entry may be written over what History holds now.
fn sent(s: &Status) -> bool {
    matches!(s, Status::Ready | Status::Pasted)
}

impl Inner {
    fn release_claim(&mut self, job: JobId) {
        if let Some(m) = self.meta.get_mut(&job) {
            if m.claimed {
                m.claimed = false;
                crate::claims::release(&self.dir, m.entry);
            }
        }
    }

    /// Before the keyboard uploads a take: History may already have its words (sent from History
    /// or in the background), it may be deleted, or someone else may be sending it right now.
    /// Otherwise the flow claims it, so nobody else uploads it until the answer is back.
    fn before_upload(&mut self, job: JobId) -> Upload {
        let Some(m) = self.meta.get(&job).cloned() else { return Upload::Go { entry: 0 } };
        if m.private {
            return Upload::Go { entry: 0 };
        }
        let now = History::edit(&self.dir, |h| (false, h.get(m.entry).cloned()));
        match now {
            None => Upload::Skip,
            Some(e) if sent(&e.status) && !e.text.trim().is_empty() => Upload::Adopt(e.text),
            Some(_) if m.claimed || crate::claims::try_claim(&self.dir, m.entry) => {
                if let Some(m) = self.meta.get_mut(&job) {
                    m.claimed = true;
                }
                Upload::Go { entry: m.entry }
            }
            Some(_) => Upload::Skip,
        }
    }

    /// Mirror the jobs this event changed into the saved history, from the first moment of
    /// recording (a keyboard's process can die at any time). Never brings back an entry that was
    /// deleted, never writes an unsent state over words someone else got, and keeps text the user
    /// edited. Written on History's writer thread.
    fn sync_history(&mut self, dirty: &BTreeSet<JobId>) {
        // (entry id, what the flow says, may insert, recording to delete with it)
        let mut items: Vec<(u64, Option<Entry>, bool, Option<PathBuf>)> = Vec::new();
        for id in dirty {
            let (Some(job), Some(m)) = (self.flow.job(*id), self.meta.get(id)) else { continue };
            if m.private {
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
                JobState::WaitingForNetwork => Some(entry(String::new(), Status::Waiting(crate::notices::WAITING_FOR_NETWORK.into()))),
                JobState::Ready(t) | JobState::Delivered(t) if t.trim().is_empty() => None,
                JobState::Ready(t) => Some(entry(t.clone(), Status::Ready)),
                JobState::Delivered(t) => Some(entry(t.clone(), Status::Pasted)),
                JobState::Failed(e) if e.needs_sign_in() => Some(entry(String::new(), Status::Waiting(crate::notices::WAITING_FOR_SIGN_IN.into()))),
                // No speech: nothing worth keeping.
                JobState::Failed(TranscribeError::Server(204)) => None,
                JobState::Failed(e) => Some(entry(String::new(), Status::Failed(e.reason().into()))),
                JobState::Discarded => None,
            };
            let drop_audio = matches!(job.state, JobState::Discarded | JobState::Failed(TranscribeError::Server(204))).then(|| PathBuf::from(&m.pcm));
            items.push((m.entry, next, job.state == JobState::Recording, drop_audio));
        }
        if items.is_empty() {
            return;
        }
        let written = self.written.clone();
        History::edit_later(&self.dir, move |h| {
            let mut written = written.lock().unwrap_or_else(|e| e.into_inner());
            let mut changed = false;
            for (id, next, may_insert, drop_audio) in items {
                let now = h.get(id).cloned();
                let ours = written.get(&id);
                // Changed by someone else since this flow last wrote it (or deleted).
                let others = match (&now, ours) {
                    (Some(n), Some(o)) => n != o,
                    (Some(_), None) => true,
                    (None, Some(_)) => true,
                    (None, None) => false,
                };
                match next {
                    Some(mut want) => {
                        match &now {
                            // Deleted (or never there): only a take that just started is added.
                            None if !(may_insert && !others) => continue,
                            None => {}
                            Some(n) if others && sent(&n.status) && !sent(&want.status) => continue,
                            Some(n) if others && sent(&n.status) => want.text = n.text.clone(),
                            Some(_) => {}
                        }
                        changed |= h.upsert(want.clone());
                        written.insert(id, want);
                    }
                    None => {
                        if now.as_ref().is_some_and(|n| others && sent(&n.status)) {
                            continue;
                        }
                        changed |= h.remove(id);
                        written.remove(&id);
                        if let Some(a) = drop_audio {
                            let _ = std::fs::remove_file(a);
                        }
                    }
                }
            }
            changed
        });
    }

    /// Incognito takes are never kept: the recording goes as soon as nothing can send it again.
    /// It stays while recording, being written out, waiting for the network or a sign-in (both
    /// are sent again on their own), or while the strip still offers to try again.
    fn tidy_private_audio(&mut self) {
        let on_screen = match self.flow.state() {
            State::NeedsYou(p) => Some(p.job()),
            _ => None,
        };
        for (job, m) in self.meta.iter_mut().filter(|(_, m)| m.private && !m.audio_gone) {
            let keep = self.flow.is_uploading(*job)
                || match self.flow.job(*job).map(|j| &j.state) {
                    Some(JobState::Recording | JobState::Transcribing | JobState::WaitingForNetwork) => true,
                    Some(JobState::Failed(e)) if e.needs_sign_in() => true,
                    Some(JobState::Failed(TranscribeError::Server(204))) => false,
                    Some(JobState::Failed(_)) => on_screen == Some(*job),
                    _ => false,
                };
            if !keep {
                let _ = std::fs::remove_file(&m.pcm);
                m.audio_gone = true;
            }
        }
    }
}

// Regression test from the bug hunt of 2026-11-28.
#[cfg(test)]
mod regression_tests {
    use super::*;
    use crate::VoxaCore;

    /// `resend` and `send_waiting` claim an entry in `VoxaCore::sending`, but the keyboard's own
    /// upload (the flow's `Transcribe` effect, then `transcribe_recording`) never does. A Failed
    /// take whose notification Retry / History "Send now" is uploading right now can be retried
    /// from the keyboard's "Try again" too: the same recording is uploaded twice at once, and the
    /// later answer overwrites the entry.
    #[test]
    fn the_keyboard_never_uploads_an_entry_the_core_is_already_sending() {
        let dir = std::env::temp_dir().join(format!("voxa-ffi-claim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let d = dir.to_string_lossy().to_string();
        let ready = ReadinessInfo { session: SessionState::Valid, network: NetworkState::Up, mic: MicState::Granted };
        let flow = VoxaFlow::new(d.clone(), ready);
        let core = VoxaCore::new(d);
        let fx = flow.handle(FlowEvent::MicPressed { target: FfiTarget { app: "Notes".into(), window: 1 }, private: false });
        let job = fx.iter().find_map(|e| if let FlowEffect::StartCapture { job, .. } = e { Some(*job) } else { None }).unwrap();
        flow.handle(FlowEvent::Stop);
        flow.handle(FlowEvent::TranscribeFailed { job, error: crate::TranscribeFailure::Timeout });
        let entry = flow.entry_for_job(job).unwrap();
        let _held = core.claim(entry).expect("the notification's Retry is sending it");
        let fx = flow.handle(FlowEvent::Action); // the keyboard's "Try again" at the same time
        assert!(!fx.iter().any(|e| matches!(e, FlowEffect::Transcribe { .. })), "second concurrent upload of entry {entry}: {fx:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
