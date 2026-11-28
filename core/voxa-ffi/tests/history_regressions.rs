//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use voxa_ffi::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("voxa-ffi-reg-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}
fn ready() -> ReadinessInfo {
    ReadinessInfo { session: SessionState::Valid, network: NetworkState::Up, mic: MicState::Granted }
}
fn mic(private: bool) -> FlowEvent {
    FlowEvent::MicPressed { target: FfiTarget { app: "Notes".into(), window: 1 }, private }
}
fn start(flow: &VoxaFlow, private: bool) -> (u64, String) {
    flow.handle(mic(private))
        .into_iter()
        .find_map(|e| match e {
            FlowEffect::StartCapture { job, pcm_path } => Some((job, pcm_path)),
            _ => None,
        })
        .expect("starts capture")
}
fn loud_pcm(path: &str) {
    let bytes: Vec<u8> = (0..16_000).flat_map(|i: i32| (if i % 16 < 8 { 4000i16 } else { -4000 }).to_le_bytes()).collect();
    std::fs::write(path, bytes).unwrap();
}

/// A take failed ("Couldn't reach ChatGPT"); the background send's give-up run (or the
/// notification's Retry) writes it out. The keyboard's flow still remembers it as Failed, and its
/// next event (any event) mirrors that stale view back into History over the transcript.
#[test]
fn a_later_keyboard_event_does_not_overwrite_a_background_send_result() {
    let dir = tmp("overwrite");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, pcm) = start(&flow, false);
    loud_pcm(&pcm);
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Timeout });
    let entry = flow.entry_for_job(job).unwrap();
    assert!(!flow.owns(entry), "Failed takes are not the keyboard's, so the background may send them");
    assert_eq!(core.send_waiting(entry, true, Some("the words".into())), SendOutcome::Sent { app: "Notes".into() });
    assert_eq!(core.entry(entry).unwrap().text, "the words");
    flow.handle(FlowEvent::Dismiss); // the user closes the strip
    let e = core.entry(entry).unwrap();
    assert_eq!((e.text.as_str(), e.status), ("the words", EntryStatus::Ready), "the transcript was overwritten by the keyboard's stale state");
}

/// The user deletes a dictation in History; the keyboard's next event brings it back.
#[test]
fn a_deleted_entry_stays_deleted_after_the_next_keyboard_event() {
    let dir = tmp("resurrect");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, _) = start(&flow, false);
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: "secret words".into() });
    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::Inserted });
    let entry = flow.entry_for_job(job).unwrap();
    core.delete_entry(entry);
    assert!(core.entry(entry).is_none());
    flow.handle(FlowEvent::Dismiss);
    assert!(core.entry(entry).is_none(), "deleted dictation came back after the next keyboard event");
}

/// The user fixes a word in a written dictation; the keyboard's next event reverts the edit.
#[test]
fn an_edited_entry_keeps_its_edit_after_the_next_keyboard_event() {
    let dir = tmp("edit");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let core = VoxaCore::new(dir.clone());
    let (job, _) = start(&flow, false);
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: "clawed code".into() });
    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::Inserted });
    let entry = flow.entry_for_job(job).unwrap();
    core.set_entry_text(entry, "Claude Code".into()).unwrap();
    flow.handle(FlowEvent::Dismiss);
    assert_eq!(core.entry(entry).unwrap().text, "Claude Code", "the user's edit was reverted");
}

/// Incognito: the audio is deleted as soon as the job is Failed, but the strip still offers
/// "Try again" (Unreachable) and sign-in resends a SignInToSend take: the retry uploads a file
/// that no longer exists, and the dictation is lost although the strip promised to keep it.
#[test]
fn an_incognito_take_that_can_still_be_retried_keeps_its_audio() {
    let dir = tmp("private-retry");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let (job, pcm) = start(&flow, true);
    loud_pcm(&pcm);
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Timeout });
    assert!(matches!(flow.state(), FlowState::NeedsYou { problem: ProblemInfo::Unreachable { .. } }));
    let fx = flow.handle(FlowEvent::Action); // "Try again"
    let path = fx.iter().find_map(|e| match e {
        FlowEffect::Transcribe { pcm_path, .. } => Some(pcm_path.clone()),
        _ => None,
    });
    let path = path.expect("Try again uploads");
    assert!(std::path::Path::new(&path).exists(), "Try again uploads a recording that was already deleted");
}

/// "No speech" deletes the recording and its History entry. The core reports no speech for any
/// empty text, which voxa-chatgpt also returns for a 2xx reply with an unexpected JSON shape
/// (`unwrap_or_default` in `transcribe`): a response format change silently deletes recordings.
/// (The parsing half is from reading the code; this proves the deletion half.)
#[test]
fn an_empty_transcript_from_a_2xx_reply_does_not_delete_the_recording() {
    let dir = tmp("empty-deletes");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let (job, pcm) = start(&flow, false);
    loud_pcm(&pcm);
    flow.handle(FlowEvent::Stop);
    // What the keyboard dispatches when transcribe_recording returns Ok("") because ChatGPT
    // answered {"result": {"text": "..."}} (or any shape without text/transcript/message).
    flow.handle(FlowEvent::Transcribed { job, text: String::new() });
    assert!(std::path::Path::new(&pcm).exists(), "a loud recording was deleted as 'no speech'");
}
