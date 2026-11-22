//! One test per scenario in docs/rebuild/01-flow-audit.md ("Target loop for the rebuild").

use voxa_core::copy::strip_text;
use voxa_core::flow::Delivery;
use voxa_core::*;

fn whatsapp() -> Target {
    Target { app: "WhatsApp".into(), window: 1 }
}
fn slack() -> Target {
    Target { app: "Slack".into(), window: 2 }
}
fn ready() -> Readiness {
    Readiness { session: Session::Valid, network: Network::Up, mic: Mic::Granted }
}
fn has(fx: &[Effect], want: &Effect) -> bool {
    fx.contains(want)
}
fn started(flow: &mut Flow) -> JobId {
    flow.handle(Event::Hotkey { target: whatsapp() });
    match flow.state() {
        State::Recording { job, .. } => *job,
        s => panic!("expected Recording, got {s:?}"),
    }
}

#[test]
fn happy_path_records_transcribes_pastes_into_original_app() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    let fx = f.handle(Event::Stop);
    assert!(has(&fx, &Effect::StopCapture { job }));
    assert!(has(&fx, &Effect::Transcribe { job }));
    let fx = f.handle(Event::Transcribed { job, result: Ok("Running late".into()) });
    assert!(has(&fx, &Effect::Paste { job, target: whatsapp(), text: "Running late".into() }));
    let fx = f.handle(Event::Delivered { job, result: Delivery::Pasted });
    assert_eq!(f.state(), &State::Delivered { job });
    assert!(has(&fx, &Effect::HideStrip { after_ms: 2000 }));
    assert_eq!(strip_text(&f).unwrap().title, "Pasted in WhatsApp");
}

#[test]
fn expired_session_is_caught_before_talking_and_nothing_is_recorded() {
    let mut f = Flow::new(Readiness { session: Session::Expired, ..ready() });
    let fx = f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(f.state(), &State::Blocked(Blocker::SignedOut));
    assert!(!fx.iter().any(|e| matches!(e, Effect::StartCapture { .. })), "must not record");
    assert_eq!(f.jobs().count(), 0);
    let t = strip_text(&f).unwrap();
    assert_eq!(t.title, "Sign in to ChatGPT to dictate");
    assert_eq!(t.action.as_deref(), Some("Sign in"));
    assert!(t.dismissable);
    assert!(has(&f.handle(Event::Action), &Effect::OpenSignIn));
    // Signing in returns to Idle, ready in the same field.
    let fx = f.handle(Event::Readiness(ready()));
    assert_eq!(f.state(), &State::Idle);
    assert!(has(&fx, &Effect::HideStrip { after_ms: 0 }));
}

#[test]
fn owner_whatsapp_case_expired_mid_dictation_resends_into_whatsapp_after_sign_in() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::SessionExpired) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::SignInToSend { job }));
    assert_eq!(f.readiness().session, Session::Expired);
    let t = strip_text(&f).unwrap();
    assert_eq!(t.title, "Sign in to send this");
    assert!(t.detail.unwrap().contains("WhatsApp"));
    assert!(has(&f.handle(Event::Action), &Effect::OpenSignIn));
    // No trip to History: signing in resends automatically...
    let fx = f.handle(Event::Readiness(ready()));
    assert_eq!(f.state(), &State::Transcribing { job });
    assert!(has(&fx, &Effect::Transcribe { job }));
    // ...and the text goes to WhatsApp, not wherever focus is now.
    let fx = f.handle(Event::Transcribed { job, result: Ok("Late, start without me".into()) });
    assert!(has(&fx, &Effect::Paste { job, target: whatsapp(), text: "Late, start without me".into() }));
}

#[test]
fn cloudflare_block_never_asks_to_sign_in() {
    let e = TranscribeError::from_response(403, true, false).unwrap();
    assert_eq!(e, TranscribeError::Blocked);
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(e) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::Unreachable { job }));
    assert_eq!(f.readiness().session, Session::Valid, "a block must not mark the session expired");
    assert_eq!(strip_text(&f).unwrap().action.as_deref(), Some("Retry"));
    let fx = f.handle(Event::Action);
    assert!(has(&fx, &Effect::Transcribe { job }));
}

#[test]
fn json_403_is_an_expired_session() {
    assert_eq!(TranscribeError::from_response(403, false, false), Some(TranscribeError::SessionExpired));
    assert_eq!(TranscribeError::from_response(200, false, false), None);
    assert_eq!(TranscribeError::from_response(502, false, false), Some(TranscribeError::Server(502)));
    assert_eq!(TranscribeError::from_response(200, false, true), Some(TranscribeError::Blocked));
}

#[test]
fn offline_still_records_and_sends_itself_when_back_online() {
    let mut f = Flow::new(Readiness { network: Network::Down, ..ready() });
    let job = started(&mut f);
    assert_eq!(f.state(), &State::Recording { job, offline: true });
    assert_eq!(strip_text(&f).unwrap().title, "Offline, keeping it");
    let fx = f.handle(Event::Stop);
    assert!(!fx.iter().any(|e| matches!(e, Effect::Transcribe { .. })));
    assert_eq!(f.state(), &State::NeedsYou(Problem::WaitingForNetwork { job }));
    let fx = f.handle(Event::Readiness(ready()));
    assert!(has(&fx, &Effect::Transcribe { job }));
    assert_eq!(f.state(), &State::Transcribing { job });
}

#[test]
fn going_offline_during_transcription_queues_the_job() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Offline) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::WaitingForNetwork { job }));
    assert_eq!(f.job(job).unwrap().state, JobState::WaitingForNetwork);
}

#[test]
fn switching_apps_asks_instead_of_pasting_into_the_wrong_chat() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("hi".into()) });
    f.handle(Event::Delivered { job, result: Delivery::TargetChanged(slack()) });
    let t = strip_text(&f).unwrap();
    assert_eq!(t.title, "You moved to Slack");
    assert_eq!(t.action.as_deref(), Some("Paste in Slack"));
    let fx = f.handle(Event::Action);
    assert!(has(&fx, &Effect::PasteHere { job, text: "hi".into() }));
}

#[test]
fn no_speech_offers_no_retry_and_closes_itself() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    let fx = f.handle(Event::Transcribed { job, result: Ok("   ".into()) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::NoSpeech { job }));
    assert_eq!(strip_text(&f).unwrap().action, None);
    assert!(has(&fx, &Effect::HideStrip { after_ms: 4000 }));
}

#[test]
fn cancel_discards_the_recording() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    let fx = f.handle(Event::Cancel);
    assert!(has(&fx, &Effect::DiscardCapture { job }));
    assert_eq!(f.state(), &State::Idle);
    assert_eq!(f.job(job).unwrap().state, JobState::Discarded);
}

#[test]
fn mic_blocked_is_caught_before_talking() {
    let mut f = Flow::new(Readiness { mic: Mic::Denied, ..ready() });
    f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(f.state(), &State::Blocked(Blocker::MicDenied));
    assert!(has(&f.handle(Event::Action), &Effect::OpenMicSettings));
}

#[test]
fn dismiss_keeps_the_job_for_history() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    f.handle(Event::Dismiss);
    assert_eq!(f.state(), &State::Idle);
    assert_eq!(f.job(job).unwrap().state, JobState::Failed(TranscribeError::Timeout));
}

#[test]
fn hotkey_while_recording_stops_and_while_writing_is_ignored() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(f.state(), &State::Transcribing { job });
    let fx = f.handle(Event::Hotkey { target: slack() });
    assert!(fx.is_empty(), "no second dictation while one is being written out");
    assert_eq!(f.jobs().count(), 1);
}

#[test]
fn late_result_for_a_cancelled_job_does_not_paste() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Cancel);
    let fx = f.handle(Event::Transcribed { job, result: Ok("too late".into()) });
    assert!(!fx.iter().any(|e| matches!(e, Effect::Paste { .. })));
    assert_eq!(f.state(), &State::Idle);
}
