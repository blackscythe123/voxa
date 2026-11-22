//! Can this HTTP stack reach ChatGPT with a real session? Prints outcomes only, never secrets.
//! Usage: cargo run -p voxa-chatgpt --example probe [-- path\to\audio.wav]

use voxa_chatgpt::{ChatGpt, Session};

fn main() {
    let appdata = std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA"));
    // Voxa's own sign-in first, then the current Electron app's session.
    let (dir, loaded) = match Session::from_voxa_dir(&appdata.join("voxa-next")) {
        Some(s) => (appdata.join("voxa-next"), Some(s)),
        None => (appdata.join("voxa"), Session::from_electron_dir(&appdata.join("voxa"))),
    };
    println!("session source: {}", dir.display());
    let Some(session) = loaded else {
        println!("no saved session in {}", dir.display());
        return;
    };
    println!("loaded {session:?}");
    let client = ChatGpt::new();
    let t = std::time::Instant::now();
    match client.access_token(&session) {
        Ok(tok) => println!("session check: OK, access token received ({} chars) in {:?}", tok.len(), t.elapsed()),
        Err(e) => println!("session check: {e:?} after {:?}", t.elapsed()),
    }
    if let Some(path) = std::env::args().nth(1) {
        let audio = std::fs::read(&path).expect("read audio");
        let t = std::time::Instant::now();
        match client.transcribe(&session, audio, "recording.wav", "audio/wav") {
            Ok(text) => println!("transcribe: OK in {:?}: {text:?}", t.elapsed()),
            Err(e) => println!("transcribe: {e:?} after {:?}", t.elapsed()),
        }
    }
}
