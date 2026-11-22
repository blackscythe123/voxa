//! Can this HTTP stack reach ChatGPT with a real session? Prints outcomes only, never secrets.
//! Usage: cargo run -p voxa-chatgpt --example probe [-- path\to\audio.wav]

use voxa_chatgpt::{ChatGpt, Session};

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA")).join("voxa");
    let Some(session) = Session::from_electron_dir(&dir) else {
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
