//! Regression tests from the Android bug hunt of 2026-11-28. Each one failed on the code before its
//! fix and asserts the behaviour that must hold. Driven exactly the way the Android keyboard
//! (VoxaFlow, one per keyboard service, alive as long as the IME) and the app/jobs (VoxaCore on
//! the same files folder, same process) drive the core.

use voxa_ffi::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("voxa-kb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

fn ready() -> ReadinessInfo {
    ReadinessInfo { session: SessionState::Valid, network: NetworkState::Up, mic: MicState::Granted }
}

fn target(app: &str, window: u64) -> FfiTarget {
    FfiTarget { app: app.into(), window }
}

/// A loud take recorded in `app`; returns (job, history entry id).
fn record(flow: &VoxaFlow, app: &str) -> (u64, u64) {
    std::thread::sleep(std::time::Duration::from_millis(3)); // entry ids are ms timestamps
    let (job, pcm) = flow
        .handle(FlowEvent::MicPressed { target: target(app, 1), private: false })
        .into_iter()
        .find_map(|e| match e {
            FlowEffect::StartCapture { job, pcm_path } => Some((job, pcm_path)),
            _ => None,
        })
        .expect("starts capture");
    let loud: Vec<u8> = (0..32_000).flat_map(|i: i32| (if i % 16 < 8 { 4000i16 } else { -4000 }).to_le_bytes()).collect();
    std::fs::write(&pcm, loud).unwrap();
    (job, flow.entry_for_job(job).expect("kept in History"))
}

/// What VoxaKeyboard.onStartInput sends every time any field gets focus (VoxaKeyboard.kt:197).
fn focus_somewhere(flow: &VoxaFlow) {
    flow.handle(FlowEvent::Focus { target: target("Chrome", 99) });
}

/// A dictation pasted normally, then the strip dismissed.
fn pasted_take(flow: &VoxaFlow, app: &str, text: &str) -> u64 {
    let (job, entry) = record(flow, app);
    flow.handle(FlowEvent::Stop);
    flow.set_recording_secs(job, 2.0);
    flow.handle(FlowEvent::Transcribed { job, text: text.into() });
    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::Inserted });
    flow.handle(FlowEvent::Dismiss);
    entry
}

#[test]
fn h1_an_edit_made_in_history_survives_the_keyboard() {
    let dir = tmp("edit");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let entry = pasted_take(&flow, "Messages", "see you at five");

    // History > Edit > Save (DetailScreen.kt:163).
    let saved = core.set_entry_text(entry, "See you at 5:30.".into()).expect("written entry");
    assert_eq!(saved.text, "See you at 5:30.");

    // The user goes back to any app: the keyboard gets a field.
    focus_somewhere(&flow);
    assert_eq!(core.entry(entry).unwrap().text, "See you at 5:30.", "the user's edit was silently reverted by the keyboard's sync_history");
}

#[test]
fn h2_a_deleted_dictation_stays_deleted() {
    let dir = tmp("delete");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let entry = pasted_take(&flow, "Messages", "something private");

    // History > Delete (DetailScreen.kt:145).
    core.delete_entry(entry);
    assert!(core.entry(entry).is_none());

    focus_somewhere(&flow);
    assert!(core.entry(entry).is_none(), "the deleted dictation came back into History");
}

#[test]
fn h2b_clear_history_stays_cleared() {
    let dir = tmp("clear");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    pasted_take(&flow, "Messages", "one");
    pasted_take(&flow, "Notes", "two");

    // Settings > Clear history (SettingsScreen.kt:152).
    core.clear_history();
    assert!(core.history(String::new()).is_empty());

    focus_somewhere(&flow);
    assert_eq!(core.history(String::new()).len(), 0, "every dictation of this keyboard session came back after Clear history");
}

#[test]
fn h3_a_retry_sent_in_the_background_keeps_its_text() {
    let dir = tmp("retry-overwrite");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, entry) = record(&flow, "Notes");
    flow.handle(FlowEvent::Stop);
    // ChatGPT timed out after the core's three tries: "Couldn't reach ChatGPT".
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Timeout });
    flow.handle(FlowEvent::Dismiss);
    assert!(!flow.owns(entry), "not the keyboard's any more, so the notification's Retry may send it");

    // Notification "Retry" -> VoxaJobs.send(manual = true) -> sendWaiting(id, giveUp = true).
    assert_eq!(core.send_waiting(entry, true, Some("the words the user said".into())), SendOutcome::Sent { app: "Notes".into() });
    assert_eq!(core.entry(entry).unwrap().status, EntryStatus::Ready);

    focus_somewhere(&flow);
    let e = core.entry(entry).unwrap();
    assert_eq!(
        (e.status, e.text.as_str()),
        (EntryStatus::Ready, "the words the user said"),
        "the keyboard's stale Failed job overwrote the sent text (paid for, then lost)"
    );
}

/// The keyboard's upload and a History or background send of the same take exclude each other,
/// both ways: one recording is never uploaded twice at once. (Changed from the hunt's proof, which
/// asserted the strip's Retry never uploads at all, even with nobody else sending; that would
/// break "Try again" for incognito takes, which only the keyboard can send.)
#[test]
fn h4_keyboard_retry_and_a_history_or_background_send_are_not_mutually_excluded() {
    let dir = tmp("two-uploads");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, entry) = record(&flow, "Notes");
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Timeout });
    assert!(!flow.owns(entry), "failed: History and the background may send it");
    assert!(!core.is_sending(entry));

    // The keyboard's Retry claims the entry for its upload...
    let fx = flow.handle(FlowEvent::Action);
    let pcm = fx.iter().find_map(|e| match e {
        FlowEffect::Transcribe { job: j, pcm_path, entry: en, .. } if *j == job => Some((pcm_path.clone(), *en)),
        _ => None,
    });
    let (pcm, en) = pcm.expect("the strip's Retry uploads");
    assert_eq!(en, entry);
    assert!(core.is_sending(entry), "the keyboard's upload registers with the core");
    // ...so History's Send now and the background send back off while it runs.
    assert!(core.resend(entry).is_none(), "Send now while the keyboard uploads does nothing");
    assert_eq!(core.send_waiting(entry, true, Some("x".into())), SendOutcome::Busy);
    // The upload ends (the core writes its result and the claim ends).
    let text = core.transcribe_take(entry, pcm, "Notes".into(), Some("the words".into())).unwrap();
    assert!(!core.is_sending(entry));
    flow.handle(FlowEvent::Transcribed { job, text });

    // The other way round (History holds the claim, the strip's Retry backs off) is
    // flow::regression_tests::the_keyboard_never_uploads_an_entry_the_core_is_already_sending.
}

#[test]
fn h5_a_keyboard_restart_mid_upload_does_not_start_a_second_upload() {
    let dir = tmp("ime-restart");
    let core = VoxaCore::new(dir.clone());
    let entry;
    {
        let flow = VoxaFlow::new(dir.clone(), ready());
        let (_, e) = record(&flow, "Notes");
        entry = e;
        let fx = flow.handle(FlowEvent::Stop);
        assert!(fx.iter().any(|e| matches!(e, FlowEffect::Transcribe { .. })), "upload #1 starts (blocking FFI call on an IO thread)");
        // The user taps the switch-keyboard key: VoxaKeyboard.onDestroy cancels its scope; the
        // blocking upload carries on, but its answer is thrown away (VoxaKeyboard.kt:414).
    }
    // Back to Voxa keyboard a few seconds later: a new VoxaFlow in the same process.
    let _again = VoxaFlow::new(dir.clone(), ready());
    assert_eq!(core.waiting_for_network(), vec![entry], "recover_interrupted turned 'Writing it out' into waiting");
    // historyChanged -> scheduleSendIfWaiting -> VoxaJobs: keyboardOwns is false, the claim is free.
    let second = core.send_waiting(entry, false, Some("same words".into()));
    assert_eq!(second, SendOutcome::Busy, "a second upload of the same recording started while upload #1 may still be in flight");
}

#[test]
fn h6_an_offline_answer_while_online_leaves_an_owned_entry_nobody_sends() {
    let dir = tmp("owned-waiting");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, entry) = record(&flow, "Notes");
    flow.handle(FlowEvent::Stop);
    // A DNS hiccup during the upload: Offline, though Android still says the network is validated.
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Offline });
    flow.handle(FlowEvent::Dismiss);
    assert_eq!(flow.readiness().network, NetworkState::Down, "the flow now believes it is offline; the app (online = true) never changes, so pushReadiness only fixes it on the next keyboard show");
    assert_eq!(core.waiting_for_network(), vec![entry]);
    // VoxaJobs skips it (keyboardOwns), History's Send now refuses it ("It's being sent already").
    assert!(!flow.owns(entry), "an owned WaitingForNetwork entry is skipped by the background send and refused by Send now, while nothing retries it");
}

fn upload_of(fx: &[FlowEffect]) -> Option<(u64, String, u64)> {
    fx.iter().find_map(|e| match e {
        FlowEffect::Transcribe { job, pcm_path, entry, .. } => Some((*job, pcm_path.clone(), *entry)),
        _ => None,
    })
}

/// Switching keyboards while a take is being written out: the keyboard goes away, but the upload
/// carries on and the core keeps its result in History. Nobody uploads it a second time.
#[test]
fn a_keyboard_switch_mid_upload_keeps_the_result_in_history() {
    let dir = tmp("switch-keeps");
    let core = VoxaCore::new(dir.clone());
    let (entry, pcm) = {
        let flow = VoxaFlow::new(dir.clone(), ready());
        let (_, entry) = record(&flow, "Notes");
        let (_, pcm, e) = upload_of(&flow.handle(FlowEvent::Stop)).expect("upload starts");
        assert_eq!(e, entry);
        (entry, pcm)
    }; // the keyboard is destroyed; its upload runs on
    let _again = VoxaFlow::new(dir.clone(), ready());
    assert_eq!(core.send_waiting(entry, false, Some("x".into())), SendOutcome::Busy, "no second upload while the first runs");
    core.transcribe_take(entry, pcm, "Notes".into(), Some("the words".into())).unwrap();
    let e = core.entry(entry).unwrap();
    assert_eq!((e.status, e.text.as_str()), (EntryStatus::Ready, "the words"));
    assert!(!core.is_sending(entry));
}

/// The strip's "Try again" after History or the background already got the words: they are
/// pasted, and the recording is not uploaded again.
#[test]
fn a_retry_after_history_sent_it_pastes_those_words_without_uploading_again() {
    let dir = tmp("adopt");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, entry) = record(&flow, "Notes");
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Timeout });
    assert_eq!(core.send_waiting(entry, true, Some("sent from History".into())), SendOutcome::Sent { app: "Notes".into() });
    let fx = flow.handle(FlowEvent::Action);
    assert!(upload_of(&fx).is_none(), "uploaded again: {fx:?}");
    assert!(fx.iter().any(|e| matches!(e, FlowEffect::Paste { text, .. } if text == "sent from History")), "{fx:?}");
}

/// Only the device's own silence check may throw a take away; an empty reply keeps it.
#[test]
fn only_the_devices_silence_check_deletes_a_recording() {
    let dir = tmp("no-speech");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, entry) = record(&flow, "Notes");
    let (_, pcm, _) = upload_of(&flow.handle(FlowEvent::Stop)).unwrap();
    flow.handle(FlowEvent::Transcribed { job, text: String::new() });
    assert!(std::path::Path::new(&pcm).exists());
    assert_eq!(core.entry(entry).unwrap().status, EntryStatus::Failed("Couldn't reach ChatGPT".into()));
    let (job, entry) = record(&flow, "Notes");
    let (_, pcm, _) = upload_of(&flow.handle(FlowEvent::Stop)).unwrap();
    flow.handle(FlowEvent::NoSpeech { job });
    assert!(core.entry(entry).is_none(), "a silent take keeps nothing");
    assert!(!std::path::Path::new(&pcm).exists(), "nor its recording (removed with the entry, on History's writer thread)");
}

/// "N dictations are waiting and will send when you sign in": the background send picks up takes
/// waiting for a sign-in (dismissed ones, and ones from earlier runs) once signed in.
#[test]
fn a_take_waiting_for_sign_in_is_sent_in_the_background_after_sign_in() {
    let dir = tmp("sign-in-later");
    let entry;
    {
        let flow = VoxaFlow::new(dir.clone(), ready());
        let (job, e) = record(&flow, "Notes");
        entry = e;
        flow.handle(FlowEvent::Stop);
        flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::SessionExpired });
        flow.handle(FlowEvent::Dismiss);
    }
    let core = VoxaCore::new(dir.clone());
    assert_eq!(core.waiting_for_sign_in_ids(), vec![entry]);
    assert_eq!(core.send_waiting(entry, false, Some("hello".into())), SendOutcome::Sent { app: "Notes".into() });
    assert_eq!(core.entry(entry).unwrap().status, EntryStatus::Ready);
}

/// A take the keyboard is only waiting to send is not "owned": the background send may send it
/// (the claim keeps the two from uploading at once), so it is never stuck when the keyboard's
/// own view of the network is wrong.
#[test]
fn a_waiting_take_is_never_stuck_behind_the_keyboard() {
    let dir = tmp("not-owned");
    let flow = VoxaFlow::new(dir.clone(), ReadinessInfo { network: NetworkState::Down, ..ready() });
    let (_, entry) = record(&flow, "Notes");
    assert!(flow.owns(entry), "recording: the keyboard's");
    flow.handle(FlowEvent::Stop);
    assert!(!flow.owns(entry), "waiting for the network: anyone may send it");
}
