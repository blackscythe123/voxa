//! The Android-facing API, exercised the way the keyboard and the app drive it.

use voxa_ffi::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("voxa-ffi-{name}-{}", std::process::id()));
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

fn mic(app: &str, window: u64) -> FlowEvent {
    FlowEvent::MicPressed { target: target(app, window), private: false }
}

fn start(flow: &VoxaFlow, ev: FlowEvent) -> (u64, String) {
    flow.handle(ev)
        .into_iter()
        .find_map(|e| match e {
            FlowEffect::StartCapture { job, pcm_path } => Some((job, pcm_path)),
            _ => None,
        })
        .expect("starts capture")
}

#[test]
fn dictation_round_trip_through_the_flow_saves_history() {
    let dir = tmp("flow");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let (job, pcm) = start(&flow, mic("Messages", 7));
    assert!(pcm.ends_with(".pcm") && pcm.starts_with(&dir));
    assert_eq!(flow.status().unwrap().title, "Listening");
    let core = VoxaCore::new(dir.clone());
    assert_eq!(core.history(String::new())[0].status, EntryStatus::Waiting("Recording".into()), "kept from the first moment");

    let fx = flow.handle(FlowEvent::Stop);
    assert!(fx.contains(&FlowEffect::StopCapture { job }));
    assert!(fx.iter().any(|e| matches!(e, FlowEffect::Transcribe { app, .. } if app == "Messages")));
    assert_eq!(flow.status().unwrap().title, "Writing it out");
    flow.set_recording_secs(job, 2.5);

    let fx = flow.handle(FlowEvent::Transcribed { job, text: "Hello there.".into() });
    assert_eq!(fx, vec![FlowEffect::Paste { job, target: target("Messages", 7), text: "Hello there.".into() }]);
    assert_eq!(flow.status().unwrap().title, "Pasting");

    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::Inserted });
    let st = flow.status().unwrap();
    assert_eq!(st.title, "Pasted in Messages");
    assert_eq!(st.detail, None, "no clipboard line on Android");

    let h = core.history(String::new());
    assert_eq!(h.len(), 1);
    assert_eq!((h[0].text.as_str(), h[0].status.clone(), h[0].secs), ("Hello there.", EntryStatus::Pasted, 2.5));
    assert_eq!(core.history("messages".into()).len(), 1, "search by app");
    assert!(core.history("nothing like it".into()).is_empty());
}

#[test]
fn a_take_cut_off_by_process_death_is_recovered_as_not_sent() {
    let dir = tmp("recover");
    {
        let flow = VoxaFlow::new(dir.clone(), ready());
        let (_, pcm) = start(&flow, mic("Notes", 1));
        std::fs::write(&pcm, [1u8; 3200]).unwrap();
        // The process dies here, mid-recording.
    }
    let _next = VoxaFlow::new(dir.clone(), ready());
    let h = VoxaCore::new(dir).history(String::new());
    assert_eq!(h[0].status, EntryStatus::Waiting("Not sent".into()));
    assert!(h[0].has_audio, "the audio is kept for Send now");
}

#[test]
fn private_fields_keep_nothing() {
    let dir = tmp("private");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let (job, pcm) = start(&flow, FlowEvent::MicPressed { target: target("Chrome", 3), private: true });
    std::fs::write(&pcm, [1u8; 64]).unwrap();
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: "secret".into() });
    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::Inserted });
    assert!(VoxaCore::new(dir).history(String::new()).is_empty());
    assert!(!std::path::Path::new(&pcm).exists());
}

#[test]
fn moving_fields_asks_and_going_back_inserts() {
    let flow = VoxaFlow::new(tmp("move"), ready());
    let (job, _) = start(&flow, mic("Gmail", 1));
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: "Hi".into() });
    flow.handle(FlowEvent::Delivered { job, result: DeliveryResult::TargetChanged { now_in: target("Slack", 2) } });
    let st = flow.status().unwrap();
    assert_eq!(st.title, "You moved to Slack");
    assert_eq!(st.action.as_deref(), Some("Paste in Slack"));
    let fx = flow.handle(FlowEvent::Focus { target: target("Gmail", 1) });
    assert_eq!(fx, vec![FlowEffect::Paste { job, target: target("Gmail", 1), text: "Hi".into() }]);
}

#[test]
fn signed_out_blocks_before_recording_and_errors_map() {
    let flow = VoxaFlow::new(tmp("blocked"), ReadinessInfo { session: SessionState::Expired, ..ready() });
    assert_eq!(flow.handle(mic("Notes", 1)), vec![FlowEffect::ShowStatus]);
    assert_eq!(flow.state(), FlowState::Blocked { blocker: BlockerKind::SignedOut });
    assert_eq!(flow.status().unwrap().title, "Sign in to ChatGPT to dictate");
    assert_eq!(flow.handle(FlowEvent::Action), vec![FlowEffect::OpenSignIn]);

    let flow = VoxaFlow::new(tmp("unreach"), ready());
    let (job, _) = start(&flow, mic("Notes", 1));
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::TranscribeFailed { job, error: TranscribeFailure::Blocked });
    assert_eq!(flow.status().unwrap().title, "Couldn't reach ChatGPT");
    assert!(matches!(flow.state(), FlowState::NeedsYou { problem: ProblemInfo::Unreachable { .. } }));

    let flow = VoxaFlow::new(tmp("micdenied"), ReadinessInfo { mic: MicState::Denied, ..ready() });
    flow.handle(mic("Notes", 1));
    assert_eq!(flow.status().unwrap().detail.as_deref(), Some("Allow it in Android settings"));
}

#[test]
fn cancel_discards_the_recording() {
    let dir = tmp("cancel");
    let flow = VoxaFlow::new(dir.clone(), ready());
    let (_, pcm) = start(&flow, mic("Notes", 1));
    std::fs::write(&pcm, [0u8; 64]).unwrap();
    let fx = flow.handle(FlowEvent::Cancel);
    assert!(fx.iter().any(|e| matches!(e, FlowEffect::DiscardCapture { .. })));
    assert!(!std::path::Path::new(&pcm).exists());
    assert_eq!(flow.state(), FlowState::Idle);
    assert!(VoxaCore::new(dir).history(String::new()).is_empty());
}

#[test]
fn text_pipeline() {
    assert_eq!(remove_fillers("Um, so I think, uh, we should go.".into()), "So I think, we should go.");
    assert_eq!(spoken_formatting("Dear Sam, new paragraph thanks for the notes.".into()), "Dear Sam\n\nThanks for the notes.", "the comma before a break goes");
    let words = Words {
        fixes: vec![WordPair { from: "clawed code".into(), to: "Claude Code".into() }],
        snippets: vec![WordPair { from: "my email".into(), to: "sam@example.com".into() }],
        spoken_formatting: true,
        rules: vec![AppRule { app: "WhatsApp".into(), no_final_period: true, lowercase: false }],
    };
    assert_eq!(
        clean_text("Um, ask clawed code. New line. Mail my email.".into(), true, words, "WhatsApp".into()),
        "Ask Claude Code.\nMail sam@example.com"
    );
    assert_eq!(clean_text("Um, hi.".into(), false, Words::default(), "".into()), "Um, hi.", "fillers kept unless asked");
    assert_eq!(changed_phrase("Ask clawed code now".into(), "Ask Claude Code now".into()), Some(WordPair { from: "clawed code".into(), to: "Claude Code".into() }));
}

#[test]
fn settings_and_words_persist_and_drive_finish_text() {
    let dir = tmp("settings");
    let core = VoxaCore::new(dir.clone());
    let s = core.settings();
    assert_eq!((s.remove_fillers, s.keep_days, s.onboarded, s.hold_to_talk, s.haptics, s.spoken_formatting), (false, 30, false, false, true, false));
    assert_eq!(core.finish_text("Um, hello.".into(), "".into()), "Um, hello.");
    core.set_settings(Settings { remove_fillers: true, spoken_formatting: true, ..core.settings() });
    assert_eq!(core.finish_text("Um, hello. New line. Bye.".into(), "".into()), "Hello.\nBye.");
    assert!(core.words().spoken_formatting, "spoken formatting lives in the word list, as on desktop");
    let raw = std::fs::read_to_string(std::path::Path::new(&dir).join("settings.json")).unwrap();
    assert!(raw.contains("\"hotkey\""), "desktop fields stay in the shared file");
}

#[test]
fn session_is_in_memory_and_checked_without_network_when_missing() {
    let core = VoxaCore::new(tmp("session"));
    assert!(!core.has_session());
    assert_eq!(core.check_session().session, SessionState::Expired, "no session means signed out, no network call");
    core.use_session("a=b".into(), "UA".into());
    assert!(core.has_session());
    core.sign_out();
    assert!(!core.has_session());
    assert_eq!(core.transcribe_recording("nope.pcm".into(), "x".into()), Err(TranscribeFailure::SignedOut));
}

#[test]
fn silent_recordings_are_never_sent() {
    let dir = tmp("silence");
    let core = VoxaCore::new(dir.clone());
    core.use_session("a=b".into(), "UA".into());
    let pcm = std::path::Path::new(&dir).join("quiet.pcm");
    std::fs::write(&pcm, vec![0u8; 16_000 * 2 * 2]).unwrap();
    // Silence returns "no speech" before any upload is tried.
    assert_eq!(core.transcribe_recording(pcm.to_string_lossy().into(), "x".into()), Ok(String::new()));
}

#[test]
fn meter_and_version() {
    assert_eq!(meter_level(vec![0u8; 1024]), 0.0);
    let loud: Vec<u8> = (0..512).flat_map(|i| (if i % 2 == 0 { 3000i16 } else { -3000 }).to_le_bytes()).collect();
    assert!(meter_level(loud) > 0.5);
    assert!(!core_version().is_empty());
}

#[test]
fn password_and_incognito_fields() {
    // TYPE_CLASS_TEXT | TYPE_TEXT_VARIATION_PASSWORD
    assert_eq!(field_kind(0x81, 0), FieldKind::Password);
    assert_eq!(field_kind(0xe1, 0), FieldKind::Password, "web password");
    assert_eq!(field_kind(0x12, 0), FieldKind::Password, "numeric PIN");
    assert_eq!(field_kind(0x01, 0), FieldKind::Normal);
    assert_eq!(field_kind(0x21, 0), FieldKind::Normal, "email address");
    assert_eq!(field_kind(0x01, 0x0100_0000), FieldKind::Private, "incognito");
}

#[test]
fn history_file_reads_the_desktop_format() {
    let dir = tmp("compat");
    std::fs::write(
        std::path::Path::new(&dir).join("history.json"),
        r#"[{"id": 5, "app": "Slack", "text": "", "secs": 1.0, "status": {"Waiting": "Sign in to send"}, "audio": null, "via": "Brave"}]"#,
    )
    .unwrap();
    let h = VoxaCore::new(dir).history(String::new());
    assert_eq!(h[0].status, EntryStatus::Waiting("Sign in to send".into()), "unsent entries survive pruning however old");
}

struct Collect(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
impl LogSink for Collect {
    fn log(&self, line: String) {
        self.0.lock().unwrap().push(line);
    }
}

#[test]
fn diagnostics_log_events_but_never_the_transcript() {
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    set_log_sink(Box::new(Collect(lines.clone())), false);
    let flow = VoxaFlow::new(tmp("diag"), ready());
    let (job, _) = start(&flow, mic("Signal", 9));
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: "my secret plans for friday".into() });
    let log = lines.lock().unwrap().join("\n");
    assert!(log.contains("MicPressed") && log.contains("StartCapture"), "{log}");
    assert!(log.contains("<26 chars>"), "{log}");
    assert!(!log.contains("secret"), "transcript text must never reach the log");
}
