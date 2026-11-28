//! Diagnostics from the Rust side, written to the app's log through a sink Kotlin installs.
//!
//! Privacy: never cookies, tokens, headers or session JSON; transcript text only as its length,
//! in every build.

use std::sync::RwLock;

/// Where Rust's diagnostics lines go (the app's diag log).
#[uniffi::export(callback_interface)]
pub trait LogSink: Send + Sync {
    fn log(&self, line: String);
}

struct Sink {
    sink: Box<dyn LogSink>,
}

static SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Install the log sink. `_text_preview` is ignored: transcripts are never logged, not even a
/// preview in debug builds (kept so the Kotlin call stays the same).
#[uniffi::export]
pub fn set_log_sink(sink: Box<dyn LogSink>, _text_preview: bool) {
    *SINK.write().unwrap_or_else(|e| e.into_inner()) = Some(Sink { sink });
}

pub(crate) fn log(line: impl FnOnce() -> String) {
    if let Some(s) = SINK.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        s.sink.log(line());
    }
}

/// A transcript, safe to log: its length only.
pub(crate) fn text(t: &str) -> String {
    format!("<{} chars>", t.chars().count())
}

pub(crate) fn event(e: &crate::FlowEvent) -> String {
    use crate::FlowEvent as E;
    match e {
        E::Transcribed { job, text: t } => format!("Transcribed {{ job: {job}, text: {} }}", text(t)),
        other => format!("{other:?}"),
    }
}

pub(crate) fn effect(e: &crate::FlowEffect) -> String {
    use crate::FlowEffect as F;
    match e {
        F::Paste { job, target, text: t } => format!("Paste {{ job: {job}, target: {:?}/{}, text: {} }}", target.app, target.window, text(t)),
        F::PasteHere { job, text: t } => format!("PasteHere {{ job: {job}, text: {} }}", text(t)),
        F::StartCapture { job, .. } => format!("StartCapture {{ job: {job} }}"),
        F::Transcribe { job, app, .. } => format!("Transcribe {{ job: {job}, app: {app:?} }}"),
        other => format!("{other:?}"),
    }
}

/// The flow state, without the text a problem may carry.
pub(crate) fn state(s: &crate::FlowState) -> String {
    format!("{s:?}")
}
