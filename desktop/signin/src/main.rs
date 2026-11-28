//! Voxa's ChatGPT sign-in window. Opens chatgpt.com's login page in WebView2, waits until the
//! session cookie exists, saves the cookies together with the browser's exact User-Agent
//! (Cloudflare ties its clearance cookie to it) to %APPDATA%\voxa-next\session.json, and exits.
//!
//! Exit code 0: signed in and saved. 1: the user closed the window. 2: could not start.

#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

const LOGIN_URL: &str = "https://chatgpt.com/auth/login";
const SESSION_COOKIE: &str = "__Secure-next-auth.session-token";

/// Save the captured session where Voxa reads it. A sign-in after a deliberate sign-out also
/// clears the signed-out marker, whichever way the window was opened (Settings, toast, strip),
/// so Voxa keeps saving the sign-in ChatGPT re-issues as it's used.
fn save_session(dir: &std::path::Path, bytes: &[u8]) -> bool {
    // Its own temporary name: the main app may be saving a refreshed sign-in at the same moment.
    let tmp = dir.join(format!("session.json.signin-{}.tmp", std::process::id()));
    if std::fs::write(&tmp, bytes).is_err() || std::fs::rename(&tmp, dir.join("session.json")).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    let _ = std::fs::remove_file(dir.join("signed-out"));
    true
}

/// Voxa's data folder, or VOXA_DATA: a test copy of the app (which passes its environment on to
/// this window) must save its sign-in to its own folder, never over the real one.
fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return d.into();
    }
    let root = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join("Library").join("Application Support")
    } else {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into()))
    };
    root.join("voxa-next")
}

fn main() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let event_loop = EventLoop::new();
    let Ok(window) = WindowBuilder::new()
        .with_title("Voxa: sign in to ChatGPT")
        .with_inner_size(LogicalSize::new(480.0, 720.0))
        .build(&event_loop)
    else {
        std::process::exit(2);
    };
    // Keep the WebView profile so the user stays signed in to chatgpt.com for next time.
    let mut ctx = WebContext::new(Some(dir.join("webview")));
    let Ok(webview) = WebViewBuilder::new_with_web_context(&mut ctx).with_url(LOGIN_URL).build(&window) else {
        std::process::exit(2);
    };

    let user_agent: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let started = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(800));
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => std::process::exit(1),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if user_agent.lock().unwrap().is_none() {
                    let ua = user_agent.clone();
                    let _ = webview.evaluate_script_with_callback("navigator.userAgent", move |v| {
                        // The callback gets a JSON string literal.
                        let s: String = serde_json::from_str(&v).unwrap_or(v);
                        *ua.lock().unwrap() = Some(s);
                    });
                }
                let Ok(cookies) = webview.cookies_for_url("https://chatgpt.com") else { return };
                // The token may be split into .0, .1, ... chunks; any of them means signed in.
                let signed_in = cookies.iter().any(|c| c.name() == SESSION_COOKIE || c.name().starts_with(&format!("{SESSION_COOKIE}.")));
                let ua = user_agent.lock().unwrap().clone();
                if signed_in && started.elapsed() > Duration::from_secs(1) {
                    if let Some(ua) = ua {
                        let header = cookies.iter().map(|c| format!("{}={}", c.name(), c.value())).collect::<Vec<_>>().join("; ");
                        let saved = serde_json::json!({
                            "cookie_header": header,
                            "user_agent": ua,
                            "captured_at": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                        });
                        if save_session(&dir, &serde_json::to_vec_pretty(&saved).unwrap()) {
                            std::process::exit(0);
                        }
                    }
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    /// Hunt 15: signing in from the strip after a sign-out clears the signed-out marker.
    #[test]
    fn a_saved_sign_in_clears_the_signed_out_marker() {
        let d = std::env::temp_dir().join(format!("voxa-signin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("signed-out"), b"").unwrap();
        assert!(super::save_session(&d, b"{}"));
        assert!(d.join("session.json").exists());
        assert!(!d.join("signed-out").exists(), "the marker would block every refreshed sign-in from being kept");
    }
}
