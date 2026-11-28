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
    assert_eq!(f.state(), &State::Recording { job, offline: true, paused: false });
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
    // Owner bug, 2026-11-22: a dropped upload said "sends when you're back" but never did,
    // because the network was never marked down, so its coming back never resent it.
    assert_eq!(f.readiness().network, Network::Down);
    let fx = f.handle(Event::Readiness(ready()));
    assert!(has(&fx, &Effect::Transcribe { job }), "sends itself when the network is back");
}

#[test]
fn offline_strip_offers_retry_now_and_history_can_send_it() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Offline) });
    assert_eq!(strip_text(&f).unwrap().action.as_deref(), Some("Retry now"));
    let fx = f.handle(Event::Action);
    assert!(has(&fx, &Effect::Transcribe { job }), "Retry now tries at once");
    assert_ne!(f.job(job).unwrap().state, JobState::Discarded, "nothing is thrown away");
    // Fails again, the strip is closed; History's Send now still works.
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Offline) });
    f.handle(Event::Dismiss);
    assert_eq!(f.job(job).unwrap().state, JobState::WaitingForNetwork, "closing keeps it waiting");
    let fx = f.handle(Event::Retry { job });
    assert!(has(&fx, &Effect::Transcribe { job }));
}

#[test]
fn history_retry_never_interrupts_a_recording() {
    let mut f = Flow::new(ready());
    let old = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: old, result: Err(TranscribeError::Server(500)) });
    f.handle(Event::Dismiss);
    let now = started(&mut f);
    let fx = f.handle(Event::Retry { job: old });
    assert!(fx.iter().all(|e| !matches!(e, Effect::Transcribe { .. })));
    assert_eq!(f.state(), &State::Recording { job: now, offline: false, paused: false });
}

#[test]
fn going_back_to_where_you_started_pastes_there_with_no_question() {
    // Owner case, 2026-11-22: started in VS Code, stopped in Brave, went back to VS Code.
    let mut f = Flow::new(ready());
    let job = started(&mut f); // in WhatsApp
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("hi".into()) });
    f.handle(Event::Delivered { job, result: Delivery::TargetChanged(slack()) });
    let fx = f.handle(Event::Focus { target: whatsapp() });
    assert!(has(&fx, &Effect::Paste { job, target: whatsapp(), text: "hi".into() }), "pastes where it started");
    f.handle(Event::Delivered { job, result: Delivery::Pasted });
    let j = f.job(job).unwrap();
    assert_eq!((j.target.clone(), j.started_in.clone()), (whatsapp(), None), "recorded where it started, no move");
}

#[test]
fn moving_on_to_a_third_app_updates_the_question() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("hi".into()) });
    f.handle(Event::Delivered { job, result: Delivery::TargetChanged(slack()) });
    let gmail = Target { app: "Gmail".into(), window: 3 };
    let fx = f.handle(Event::Focus { target: gmail.clone() });
    assert!(fx.iter().all(|e| !matches!(e, Effect::Paste { .. } | Effect::PasteHere { .. })));
    let t = strip_text(&f).unwrap();
    assert_eq!((t.title.as_str(), t.action.as_deref()), ("You moved to Gmail", Some("Paste in Gmail")));
    assert_eq!(t.detail.as_deref(), Some("Go back to WhatsApp and it pastes there"));
    let fx = f.handle(Event::Action);
    assert!(has(&fx, &Effect::PasteHere { job, text: "hi".into() }));
    assert_eq!(f.job(job).unwrap().target, gmail, "recorded where it really went");
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
    // Owner bug, 2026-11-22: started in VS Code, pasted in Brave, History said VS Code.
    let j = f.job(job).unwrap();
    assert_eq!(j.target, slack(), "the text now belongs to the app it was pasted into");
    assert_eq!(j.started_in.as_ref().map(|t| t.app.as_str()), Some("WhatsApp"), "and remembers where it started");
    f.handle(Event::Delivered { job, result: Delivery::Pasted });
    assert_eq!(f.job(job).unwrap().target, slack());
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

#[test]
fn cancelling_a_retry_keeps_the_recording() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    assert_eq!(f.job(job).unwrap().state, JobState::Failed(TranscribeError::Timeout));
    f.handle(Event::Retry { job });
    assert_eq!(f.state(), &State::Transcribing { job });
    f.handle(Event::Cancel);
    assert_eq!(f.state(), &State::Idle);
    assert_eq!(f.job(job).unwrap().state, JobState::Failed(TranscribeError::Timeout), "still kept, can be sent later");
}

#[test]
fn cancelling_a_fresh_take_while_writing_drops_it() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Cancel);
    assert_eq!(f.job(job).unwrap().state, JobState::Discarded);
}

#[test]
fn a_late_result_after_cancelling_a_retry_changes_nothing() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::Timeout) });
    f.handle(Event::Retry { job });
    f.handle(Event::Cancel);
    let fx = f.handle(Event::Transcribed { job, result: Ok("late".into()) });
    assert!(fx.is_empty());
    assert_eq!(f.job(job).unwrap().state, JobState::Failed(TranscribeError::Timeout));
}

#[test]
fn a_duplicate_result_never_unpastes_a_dictation() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Ok("hello".into()) });
    f.handle(Event::Delivered { job, result: Delivery::Pasted });
    f.handle(Event::Transcribed { job, result: Ok("hello again".into()) });
    assert_eq!(f.job(job).unwrap().state, JobState::Delivered("hello".into()));
}

#[test]
fn a_stopped_mic_or_full_disk_blocks_with_its_own_words() {
    let mut f = Flow::new(Readiness { mic: Mic::Stopped, ..ready() });
    f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(f.state(), &State::Blocked(Blocker::MicStopped));
    assert_eq!(strip_text(&f).unwrap().title, "The microphone stopped");
    let mut f = Flow::new(Readiness { mic: Mic::CantSave, ..ready() });
    f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(strip_text(&f).unwrap().title, "Can't save the recording");
}

#[test]
fn a_full_disk_offers_to_free_up_space() {
    let mut f = Flow::new(Readiness { mic: Mic::CantSave, ..ready() });
    f.handle(Event::Hotkey { target: whatsapp() });
    assert_eq!(strip_text(&f).unwrap().action.as_deref(), Some("Free up space"));
    assert!(f.handle(Event::Action).contains(&Effect::OpenStorage));
}

// Pause / resume (spec approved 2026-11-28): one take, mic off while paused, no new state.

fn captures(fx: &[Effect]) -> usize {
    fx.iter()
        .filter(|e| {
            matches!(
                e,
                Effect::StartCapture { .. }
                    | Effect::StopCapture { .. }
                    | Effect::PauseCapture { .. }
                    | Effect::ResumeCapture { .. }
                    | Effect::DiscardCapture { .. }
            )
        })
        .count()
}

#[test]
fn pause_then_resume_keeps_one_take() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    let fx = f.handle(Event::Pause);
    assert_eq!(fx, vec![Effect::PauseCapture { job }]);
    assert_eq!(f.state(), &State::Recording { job, offline: false, paused: true });
    assert_eq!(f.job(job).unwrap().state, JobState::Recording, "crash recovery keeps the audio");
    let fx = f.handle(Event::Resume);
    assert_eq!(fx, vec![Effect::ResumeCapture { job }]);
    assert_eq!(f.state(), &State::Recording { job, offline: false, paused: false });
    assert_eq!(strip_text(&f).unwrap().title, "Listening");
    let fx = f.handle(Event::Stop);
    assert!(has(&fx, &Effect::Transcribe { job }));
    assert_eq!(f.jobs().count(), 1, "still one take");
}

#[test]
fn pause_and_resume_are_idempotent() {
    let mut f = Flow::new(ready());
    assert!(f.handle(Event::Pause).is_empty(), "nothing to pause in Idle");
    assert!(f.handle(Event::Resume).is_empty());
    let job = started(&mut f);
    assert!(f.handle(Event::Resume).is_empty(), "Resume when not paused does nothing");
    assert_eq!(f.handle(Event::Pause).len(), 1);
    assert!(f.handle(Event::Pause).is_empty(), "Pause twice gives one effect");
    assert_eq!(f.handle(Event::Resume).len(), 1);
    assert!(f.handle(Event::Resume).is_empty());
    f.handle(Event::Stop);
    assert_eq!(f.state(), &State::Transcribing { job });
    assert!(f.handle(Event::Pause).is_empty(), "nothing to pause while writing it out");
    f.handle(Event::Transcribed { job, result: Ok("   ".into()) });
    assert!(matches!(f.state(), State::NeedsYou(_)));
    assert!(f.handle(Event::Pause).is_empty(), "nothing to pause on a needs-you state");
    assert!(f.handle(Event::Resume).is_empty());
}

#[test]
fn stop_while_paused_sends() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    let fx = f.handle(Event::Stop);
    assert!(has(&fx, &Effect::StopCapture { job }), "the shell closes the paused capture");
    assert!(has(&fx, &Effect::Transcribe { job }));
    assert_eq!(f.state(), &State::Transcribing { job });

    // The network went down during the pause: decided at Stop, so it waits.
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    f.handle(Event::Readiness(Readiness { network: Network::Down, ..ready() }));
    let fx = f.handle(Event::Stop);
    assert!(has(&fx, &Effect::StopCapture { job }));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Transcribe { .. })));
    assert_eq!(f.state(), &State::NeedsYou(Problem::WaitingForNetwork { job }));
}

#[test]
fn hotkey_while_paused_stops() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    let fx = f.handle(Event::Hotkey { target: slack() });
    assert!(has(&fx, &Effect::StopCapture { job }));
    assert!(has(&fx, &Effect::Transcribe { job }));
    assert_eq!(f.jobs().count(), 1, "no second take");
}

#[test]
fn cancel_while_paused_discards_whole_take() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    let fx = f.handle(Event::Cancel);
    assert!(has(&fx, &Effect::DiscardCapture { job }));
    assert!(has(&fx, &Effect::HideStrip { after_ms: 0 }));
    assert_eq!(f.job(job).unwrap().state, JobState::Discarded);
    assert_eq!(f.state(), &State::Idle);
}

#[test]
fn retry_is_refused_while_paused() {
    let mut f = Flow::new(ready());
    let old = started(&mut f);
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job: old, result: Err(TranscribeError::Server(500)) });
    f.handle(Event::Dismiss);
    let now = started(&mut f);
    f.handle(Event::Pause);
    let fx = f.handle(Event::Retry { job: old });
    assert!(fx.is_empty());
    assert_eq!(f.state(), &State::Recording { job: now, offline: false, paused: true });
}

#[test]
fn readiness_changes_while_paused_keep_it_paused() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    let paused = State::Recording { job, offline: false, paused: true };
    for r in [
        Readiness { network: Network::Down, ..ready() },
        Readiness { session: Session::Expired, ..ready() },
        Readiness { mic: Mic::Stopped, ..ready() },
        ready(),
    ] {
        let fx = f.handle(Event::Readiness(r));
        assert_eq!(captures(&fx), 0, "no capture effects on {r:?}");
        assert_eq!(f.state(), &paused);
    }
    // An expired sign-in shows up after Stop, as sign in to send.
    f.handle(Event::Readiness(Readiness { session: Session::Expired, ..ready() }));
    f.handle(Event::Stop);
    f.handle(Event::Transcribed { job, result: Err(TranscribeError::SessionExpired) });
    assert_eq!(f.state(), &State::NeedsYou(Problem::SignInToSend { job }));
}

#[test]
fn paused_copy() {
    for network in [Network::Up, Network::Down] {
        let mut f = Flow::new(Readiness { network, ..ready() });
        started(&mut f);
        f.handle(Event::Pause);
        let t = strip_text(&f).unwrap();
        assert_eq!(t.title, "Paused", "online and offline takes");
        assert_eq!(t.detail.as_deref(), Some("The microphone is off"));
        assert_eq!(t.action.as_deref(), Some("Stop"));
        assert!(!t.dismissable);
    }
    assert_eq!(voxa_core::copy::PAUSE_LABEL, "Pause");
    assert_eq!(voxa_core::copy::RESUME_LABEL, "Resume");
}

#[test]
fn focus_while_paused_is_ignored() {
    let mut f = Flow::new(ready());
    let job = started(&mut f);
    f.handle(Event::Pause);
    assert!(f.handle(Event::Focus { target: slack() }).is_empty());
    assert_eq!(f.state(), &State::Recording { job, offline: false, paused: true });
    f.handle(Event::Stop);
    let fx = f.handle(Event::Transcribed { job, result: Ok("hi".into()) });
    assert!(has(&fx, &Effect::Paste { job, target: whatsapp(), text: "hi".into() }), "pastes where it started");
}
