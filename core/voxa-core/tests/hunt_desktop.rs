//! Regression tests from the desktop bug hunt of 2026-11-28 (h1 to h5 there). Each failed on the
//! code before its fix. Where a test replays what the desktop shell does with the core's
//! effects, the shell step is named in a comment.

use voxa_core::flow::Delivery;
use voxa_core::*;

fn vscode() -> Target {
    Target { app: "VS Code".into(), window: 1 }
}
fn ready() -> Readiness {
    Readiness { session: Session::Valid, network: Network::Up, mic: Mic::Granted }
}
fn transcribes(fx: &[Effect], job: JobId) -> bool {
    fx.contains(&Effect::Transcribe { job })
}
fn take(f: &mut Flow) -> JobId {
    f.handle(Event::Hotkey { target: vscode() });
    match f.state() {
        State::Recording { job, .. } => *job,
        s => panic!("expected Recording, got {s:?}"),
    }
}

/// H1. One session check can say "signed in" and "online" at once. Both the take waiting for
/// the sign-in and the takes waiting for the network go out.
#[test]
fn signed_in_and_back_online_in_one_check_sends_every_waiting_take() {
    let mut f = Flow::new(ready());
    // A: its upload drops (Offline): waiting for the network. The user closes the strip.
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Err(TranscribeError::Offline) });
    f.handle(Event::Dismiss);
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a));
    // B: recorded and sent; ChatGPT answers 401.
    let b = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: b, result: Err(TranscribeError::SessionExpired) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::SignInToSend { job: b }));
    // A's background upload drops again: the core marks the network Down.
    f.handle(Event::Transcribed { job: a, result: Err(TranscribeError::Offline) });
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork);
    assert_eq!(f.readiness().network, Network::Down);
    // The next check: signed in AND online, in one Readiness event.
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, b), "B is retried after sign-in");
    assert!(transcribes(&fx, a), "A must be sent too now that the network is back: {fx:?}");
}

/// A take that waited for the sign-in in the background (not on the strip) goes out once the
/// sign-in is back, and is never pasted.
#[test]
fn background_takes_waiting_for_the_sign_in_go_out_after_sign_in() {
    let mut f = Flow::new(ready());
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Err(TranscribeError::SessionExpired) });
    f.handle(Event::Dismiss);
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a), "{fx:?}");
    let fx = f.handle(Event::Transcribed { job: a, result: Ok("hi".into()) });
    assert!(!fx.iter().any(|e| matches!(e, Effect::Paste { .. })));
    assert_eq!(f.job(a).unwrap().state, JobState::Ready("hi".into()));
}

/// H2. Cancel on a retry's "Writing it out" keeps the take as failed, but its upload keeps
/// running. When that upload succeeds, the text is kept (never pasted), so the recording is not
/// uploaded a second time.
#[test]
fn text_from_a_cancelled_retry_is_kept_so_the_recording_is_not_uploaded_again() {
    let mut f = Flow::new(ready());
    let job = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    assert!(transcribes(&f.handle(Event::Action), job), "Retry uploads");
    f.handle(Event::Cancel);
    let fx = f.handle(Event::Transcribed { job, result: Ok("Hello there".into()) });
    assert!(!fx.iter().any(|e| matches!(e, Effect::Paste { .. } | Effect::PasteHere { .. })));
    let kept = &f.job(job).unwrap().state;
    assert_eq!(kept, &JobState::Ready("Hello there".into()), "the text that came back is kept (now {kept:?})");
    assert!(!transcribes(&f.handle(Event::Retry { job }), job), "no second upload of a recording whose text is already back");
}

/// H3. Mic trouble found when a background take is sent acts on that take only. It never
/// cancels the take the user is recording (whose recording would be deleted).
#[test]
fn a_background_take_with_mic_trouble_never_cancels_the_take_being_recorded() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let a = take(&mut f);
    f.handle(Event::Stop); // shell: the mic gave no sound
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork);
    f.handle(Event::Dismiss);
    let b = take(&mut f);
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a), "A goes out in the background");
    // Shell, running Effect::Transcribe { job: A }: the job-scoped answer.
    let fx = f.handle(Event::MicTrouble { job: a, mic: Mic::Stopped });
    assert!(!fx.contains(&Effect::DiscardCapture { job: b }), "B is discarded by A's mic trouble");
    assert!(!fx.contains(&Effect::StopCapture { job: b }));
    assert!(matches!(f.state(), State::Recording { job, .. } if *job == b));
    assert_eq!(f.readiness().mic, Mic::Granted, "B's mic is fine");
    assert!(!f.is_uploading(a));
}

/// Mic trouble on the take on the strip still says so there, as before.
#[test]
fn mic_trouble_on_the_take_on_the_strip_shows_why() {
    let mut f = Flow::new(ready());
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::MicTrouble { job: a, mic: Mic::Stopped });
    assert_eq!(f.state(), &State::Blocked(Blocker::MicStopped));
    assert_eq!(f.job(a).unwrap().state, JobState::Discarded, "nothing was recorded");
}

/// H4. Every Transcribe gets an answer: the mic-trouble answer frees the job, so a waiting
/// take is uploaded again the next time the network comes back instead of being wedged.
#[test]
fn a_transcribe_answered_with_mic_trouble_never_wedges_the_job() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let a = take(&mut f);
    f.handle(Event::Stop);
    assert_eq!(f.state(), &State::NeedsYou(Problem::WaitingForNetwork { job: a }));
    assert!(transcribes(&f.handle(Event::Action), a), "Retry now");
    f.handle(Event::MicTrouble { job: a, mic: Mic::Stopped });
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork, "the kept take stays kept");
    assert!(!f.is_uploading(a));
    f.handle(Event::Readiness(Readiness { network: Network::Down, mic: Mic::Granted, ..ready() }));
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a), "back online must upload A again (state now {:?}, uploading {})", f.job(a).unwrap().state, f.is_uploading(a));
}

/// H5. A no-speech take and a dismissed unpasted take are finished: not in progress, so History
/// can delete them (and "Delete all" removes their text).
#[test]
fn finished_jobs_are_not_in_progress() {
    let mut f = Flow::new(ready());
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Ok(String::new()) });
    let b = take(&mut f);
    assert!(f.in_progress(b), "the take being recorded is in progress");
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: b, result: Ok("private words".into()) });
    f.handle(Event::Delivered { job: b, result: Delivery::TargetChanged(Target { app: "Slack".into(), window: 9 }) });
    assert!(f.in_progress(b), "asking where to paste: still in progress");
    f.handle(Event::Dismiss);
    assert!(!f.in_progress(a), "no-speech take counted as in progress: {:?}", f.job(a).unwrap().state);
    assert!(!f.in_progress(b), "dismissed Ready take counted as in progress, so Delete all keeps its text");
}

/// Hunt 6. Text corrected in History before it was pasted is the text that gets pasted.
#[test]
fn an_edit_before_pasting_is_what_gets_pasted() {
    let mut f = Flow::new(ready());
    let job = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("teh text".into()) });
    f.handle(Event::Delivered { job, result: Delivery::TargetChanged(Target { app: "Slack".into(), window: 9 }) });
    f.handle(Event::Edit { job, text: "the text".into() });
    let fx = f.handle(Event::Focus { target: vscode() });
    assert!(fx.contains(&Effect::Paste { job, target: vscode(), text: "the text".into() }), "{fx:?}");
    f.handle(Event::Delivered { job, result: Delivery::Pasted });
    assert_eq!(f.job(job).unwrap().state, JobState::Delivered("the text".into()));
}

/// Hunt 13. "Write it out again" on this run's pasted dictation uploads it in the background;
/// its new text never pastes, and a failure keeps the old text.
#[test]
fn write_it_out_again_works_for_this_runs_dictations() {
    let mut f = Flow::new(ready());
    let job = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("first".into()) });
    f.handle(Event::Delivered { job, result: Delivery::Pasted });
    f.handle(Event::Dismiss);
    let fx = f.handle(Event::Rewrite { job });
    assert!(transcribes(&fx, job), "{fx:?}");
    assert_eq!(f.state(), &State::Idle, "never on the strip");
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    assert_eq!(f.job(job).unwrap().state, JobState::Delivered("first".into()), "a failed rewrite keeps the text");
    f.handle(Event::Rewrite { job });
    let fx = f.handle(Event::Transcribed { job, result: Ok("second".into()) });
    assert!(!fx.iter().any(|e| matches!(e, Effect::Paste { .. })));
    assert_eq!(f.job(job).unwrap().state, JobState::Ready("second".into()));
}
