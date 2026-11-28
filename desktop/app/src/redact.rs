//! Log lines for the core's events, effects and job states without what the user said: a
//! transcript is logged as its length only, in every build. Everything else is logged as is.

use voxa_core::{Effect, Event, JobState};

fn chars(t: &str) -> String {
    format!("<{} chars>", t.chars().count())
}

pub fn event(e: &Event) -> String {
    match e {
        Event::Transcribed { job, result: Ok(t) } => format!("Transcribed {{ job: {job:?}, result: Ok({}) }}", chars(t)),
        Event::Edit { job, text } => format!("Edit {{ job: {job:?}, text: {} }}", chars(text)),
        other => format!("{other:?}"),
    }
}

pub fn effect(e: &Effect) -> String {
    match e {
        Effect::Paste { job, target, text } => format!("Paste {{ job: {job:?}, app: {:?}, text: {} }}", target.app, chars(text)),
        Effect::PasteHere { job, text } => format!("PasteHere {{ job: {job:?}, text: {} }}", chars(text)),
        other => format!("{other:?}"),
    }
}

pub fn effects(fx: &[Effect]) -> String {
    format!("[{}]", fx.iter().map(effect).collect::<Vec<_>>().join(", "))
}

pub fn job_state(s: &JobState) -> String {
    match s {
        JobState::Ready(t) => format!("Ready({})", chars(t)),
        JobState::Delivered(t) => format!("Delivered({})", chars(t)),
        other => format!("{other:?}"),
    }
}

/// A transcript, as its length.
pub fn text(t: &str) -> String {
    chars(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxa_core::{JobId, Target};

    #[test]
    fn transcripts_never_reach_the_log() {
        let secret = "my secret plans for friday";
        let job = JobId(3);
        let lines = [
            event(&Event::Transcribed { job, result: Ok(secret.into()) }),
            event(&Event::Edit { job, text: secret.into() }),
            effects(&[Effect::Paste { job, target: Target { app: "Notes".into(), window: 1 }, text: secret.into() }, Effect::PasteHere { job, text: secret.into() }]),
            job_state(&JobState::Ready(secret.into())),
            job_state(&JobState::Delivered(secret.into())),
            text(secret),
        ];
        for l in &lines {
            assert!(!l.contains("secret"), "{l}");
        }
        assert!(lines[0].contains("<26 chars>"), "{}", lines[0]);
        assert!(event(&Event::Stop).contains("Stop"));
    }
}
