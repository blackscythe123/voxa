//! Regression tests from the bug hunt of 2026-11-28. Each one failed on the code before its fix
//! and asserts the behaviour that must hold.

use voxa_ffi::*;

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("voxa-ffi-rw-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

fn ready(network: NetworkState) -> ReadinessInfo {
    ReadinessInfo { session: SessionState::Valid, network, mic: MicState::Granted }
}

fn start(flow: &VoxaFlow, app: &str) -> (u64, String) {
    flow.handle(FlowEvent::MicPressed { target: FfiTarget { app: app.into(), window: 1 }, private: false })
        .into_iter()
        .find_map(|e| match e {
            FlowEffect::StartCapture { job, pcm_path } => Some((job, pcm_path)),
            _ => None,
        })
        .expect("starts capture")
}

/// A take recorded offline waits in History with its recording. Then the process dies in the
/// middle of a later history.json write (power cut, kernel panic, battery pull), leaving the file
/// torn. At the next keyboard start, VoxaFlow::new sweeps "orphan" audio using that unreadable
/// History, and the unsent recording is deleted for good.
#[test]
fn a_damaged_history_file_does_not_delete_unsent_recordings() {
    let dir = tmp("sweep");
    let pcm;
    {
        let flow = VoxaFlow::new(dir.clone(), ready(NetworkState::Down));
        let (_job, p) = start(&flow, "ZZ Notes");
        std::fs::write(&p, vec![7u8; 32_000]).unwrap();
        flow.handle(FlowEvent::Stop);
        pcm = p;
    }
    assert!(std::path::Path::new(&pcm).exists(), "precondition: the waiting take keeps its audio");
    let hist = std::path::Path::new(&dir).join("history.json");
    let bytes = std::fs::read(&hist).unwrap();
    std::fs::write(&hist, &bytes[..bytes.len() / 2]).unwrap(); // torn write

    let _next = VoxaFlow::new(dir.clone(), ready(NetworkState::Up));
    assert!(std::path::Path::new(&pcm).exists(), "the only copy of an unsent dictation was deleted by the orphan sweep");
}

/// ChatGPT answers 2xx with a JSON shape the client doesn't know (it reads only "text",
/// "transcript" or "message"). voxa-chatgpt turns that into Ok(""), which the flow treats as
/// "no speech", and on Android the recording is then deleted. Here the shell passes the "" the
/// client returns.
#[test]
fn an_unreadable_transcription_reply_keeps_the_recording() {
    let dir = tmp("shape");
    let flow = VoxaFlow::new(dir.clone(), ready(NetworkState::Up));
    let (job, pcm) = start(&flow, "ZZ Mail");
    std::fs::write(&pcm, vec![7u8; 32_000]).unwrap();
    flow.handle(FlowEvent::Stop);
    flow.handle(FlowEvent::Transcribed { job, text: String::new() });
    assert!(std::path::Path::new(&pcm).exists(), "an empty 2xx transcription deleted a real recording (no way to retry)");
}
