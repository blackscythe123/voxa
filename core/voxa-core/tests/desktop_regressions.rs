//! Regression tests from the desktop bug hunt (2026-11-28). Each failed on the code before its fix
//! and describes the behaviour that must hold. Where a test replays what the desktop shell (desktop/app/src/main.rs)
//! does with the core's effects, the shell step is named in a comment with its line.

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

/// H1. One session check can say "signed in" and "online" at once (after the sign-in window
/// closes, or after a 401 blip while the network flapped). `on_readiness` returns early after
/// retrying the SignInToSend job, so the jobs waiting for the network are skipped, and since the
/// core now believes it is online, no later check ever flips Down -> Up for them again. They show
/// "Sends when you're back online" while online, until another outage.
#[test]
fn h1_signed_in_and_back_online_in_one_check_still_sends_the_waiting_takes() {
    let mut f = Flow::new(ready());
    // A: its upload drops (Offline): waiting for the network. The user closes the strip.
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Err(TranscribeError::Offline) });
    f.handle(Event::Dismiss);
    // A check says online: A goes out in the background.
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a));
    // B: recorded and sent; ChatGPT answers 401 (expired, or a rate limit).
    let b = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: b, result: Err(TranscribeError::SessionExpired) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::SignInToSend { job: b }));
    // A's background upload drops again: the core marks the network Down.
    f.handle(Event::Transcribed { job: a, result: Err(TranscribeError::Offline) });
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork);
    assert_eq!(f.readiness().network, Network::Down);
    // The next check (main.rs:579-581): signed in AND online, in one Readiness event.
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, b), "B is retried after sign-in");
    assert!(transcribes(&fx, a), "A must be sent too now that the network is back: {fx:?}");
}

/// H2. Cancel on a retry's "Writing it out" strip (the owner's slow-network case) keeps the take
/// as failed, but the upload keeps running. When it then SUCCEEDS, `on_transcribed` drops the
/// text because the job is no longer Transcribing. The user has to press Retry, which uploads the
/// same recording again on the same slow network: the work was done once and thrown away.
#[test]
fn h2_text_from_a_cancelled_retry_is_kept_so_the_recording_is_not_uploaded_again() {
    let mut f = Flow::new(ready());
    let job = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    assert!(transcribes(&f.handle(Event::Action), job), "Retry uploads");
    f.handle(Event::Cancel);
    // The upload that was running comes back with the text.
    f.handle(Event::Transcribed { job, result: Ok("Hello there".into()) });
    let kept = &f.job(job).unwrap().state;
    assert_eq!(kept, &JobState::Ready("Hello there".into()), "the text that came back is kept (now {kept:?})");
    // And Send now must not upload the recording a second time.
    assert!(!transcribes(&f.handle(Event::Retry { job }), job), "no second upload of a recording whose text is already back");
}

/// H3. The desktop's Transcribe handler turned "the mic gave nothing" (trouble recorded at
/// StopCapture) into a bare `Event::Cancel`, which acts on whatever is on the strip. Here the
/// troubled take A was recorded offline, so its Transcribe only runs when the network comes back,
/// while the user is recording B. The core can't tell that Cancel from the user's own, so the
/// rule is a job-scoped answer: the shell reports `UploadSkipped { job: A }` and B is untouched.
/// (Changed from the hunt's proof, which replayed the shell's Cancel itself.)
#[test]
fn h3_a_background_take_with_mic_trouble_never_cancels_the_take_being_recorded() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let a = take(&mut f);
    f.handle(Event::Stop);
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork);
    f.handle(Event::Dismiss);
    let b = take(&mut f); // the user dictates B
    let fx = f.handle(Event::Readiness(ready())); // a check: back online
    assert!(transcribes(&fx, a), "A goes out in the background");
    let fx = f.handle(Event::UploadSkipped { job: a });
    assert!(!fx.contains(&Effect::DiscardCapture { job: b }), "B, the take being recorded, is discarded by A's mic trouble");
    assert!(matches!(f.state(), State::Recording { job, .. } if *job == b));
    assert!(!f.is_uploading(a), "A is no longer waited for");
}

/// H4. A Transcribe the shell never answers kept the job in `uploading` for the rest of the run:
/// the next time the network came back it was set to Transcribing with no upload asked for. The
/// shell must end every Transcribe in `Transcribed` or `UploadSkipped`; with that answer the job
/// goes back to waiting and is sent when the network returns. (Changed from the hunt's proof,
/// which expected a second upload while the first one was still unanswered: that would break the
/// rule that one recording is never in flight twice.)
#[test]
fn h4_a_transcribe_the_shell_never_answers_wedges_the_job_until_restart() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let a = take(&mut f);
    f.handle(Event::Stop);
    assert_eq!(f.state(), &State::NeedsYou(Problem::WaitingForNetwork { job: a }));
    assert!(transcribes(&f.handle(Event::Action), a), "Retry now");
    f.handle(Event::UploadSkipped { job: a });
    assert_eq!(f.job(a).unwrap().state, JobState::WaitingForNetwork);
    f.handle(Event::Readiness(Readiness { network: Network::Down, ..ready() }));
    let fx = f.handle(Event::Readiness(ready()));
    assert!(transcribes(&fx, a), "back online must upload A again (state now {:?}, uploading {})", f.job(a).unwrap().state, f.is_uploading(a));
}

/// H5. A take that came back with no speech, and a Ready take the user dismissed, will never be
/// worked on again without the user asking. The shell's "live" check counted every state but
/// Delivered and Discarded, so History hid their delete button and "Delete all" kept their text.
/// The core now says what is in progress (`in_progress`); the shells use it.
#[test]
fn h5_finished_jobs_are_not_in_progress() {
    let mut f = Flow::new(ready());
    let a = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: a, result: Ok(String::new()) });
    let b = take(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: b, result: Ok("private words".into()) });
    f.handle(Event::Delivered { job: b, result: Delivery::TargetChanged(Target { app: "Slack".into(), window: 9 }) });
    assert!(f.in_progress(b), "still asking where to paste");
    f.handle(Event::Dismiss);
    assert!(!f.in_progress(a), "no-speech take counted as in progress: {:?}", f.job(a).unwrap().state);
    assert!(!f.in_progress(b), "dismissed Ready take counted as in progress, so Delete all keeps its text");
}
