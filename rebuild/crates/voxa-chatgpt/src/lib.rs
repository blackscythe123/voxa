//! Talks to ChatGPT through the user's own signed-in session, the way the current Voxa apps do:
//! `GET /api/auth/session` for a short-lived access token, then `POST /backend-api/transcribe`.
//! Every failure maps to a typed `voxa_core::TranscribeError`.

use std::path::Path;
use std::time::Duration;

use reqwest::blocking::{Client, multipart};
use reqwest::header::{CONTENT_TYPE, HeaderMap};
use voxa_core::TranscribeError;

const SESSION_URL: &str = "https://chatgpt.com/api/auth/session";
const TRANSCRIBE_URL: &str = "https://chatgpt.com/backend-api/transcribe";
const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// A captured ChatGPT session. Cookies must be sent with the User-Agent they were issued to.
#[derive(Clone)]
pub struct Session {
    pub cookie_header: String,
    pub user_agent: String,
    pub device_id: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print cookie values.
        f.debug_struct("Session")
            .field("cookies", &format!("<{} bytes>", self.cookie_header.len()))
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

#[derive(serde::Deserialize)]
struct StoredCookie {
    name: String,
    value: String,
}

#[derive(serde::Deserialize)]
struct SavedSession {
    cookie_header: String,
    user_agent: String,
}

/// A stable random id for this install, sent as `oai-device-id` like a browser would.
pub fn device_id(dir: &Path) -> String {
    let path = dir.join("device-id.txt");
    if let Ok(id) = std::fs::read_to_string(&path) {
        if !id.trim().is_empty() {
            return id.trim().to_string();
        }
    }
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        ^ ((std::process::id() as u128) << 64);
    let h = format!("{n:032x}");
    let id = format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32]);
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(&path, &id);
    id
}

impl Session {
    /// The session Voxa's own sign-in window saved (`voxa-next\session.json`).
    pub fn from_voxa_dir(dir: &Path) -> Option<Self> {
        let saved: SavedSession = serde_json::from_slice(&std::fs::read(dir.join("session.json")).ok()?).ok()?;
        if saved.cookie_header.is_empty() {
            return None;
        }
        Some(Self { cookie_header: saved.cookie_header, user_agent: saved.user_agent, device_id: device_id(dir) })
    }

    /// Load the session the current Electron app saved (`%APPDATA%\voxa`), for migration and testing.
    pub fn from_electron_dir(dir: &Path) -> Option<Self> {
        let cookies: Vec<StoredCookie> = serde_json::from_slice(&std::fs::read(dir.join("chatgpt-cookies.json")).ok()?).ok()?;
        let cookie_header = cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ");
        let device_id = std::fs::read_to_string(dir.join("chatgpt-device-id.txt")).unwrap_or_default().trim().to_string();
        Some(Self { cookie_header, user_agent: DEFAULT_UA.into(), device_id })
    }
}

pub struct ChatGpt {
    http: Client,
    session_id: String,
}

fn classify_send_error(e: &reqwest::Error) -> TranscribeError {
    if e.is_timeout() { TranscribeError::Timeout } else { TranscribeError::Offline }
}

fn is_html(headers: &HeaderMap, body: &str) -> bool {
    headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("text/html"))
        || body.trim_start().starts_with('<')
}

impl Default for ChatGpt {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatGpt {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client");
        let session_id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        Self { http, session_id }
    }

    fn base(&self, rb: reqwest::blocking::RequestBuilder, s: &Session) -> reqwest::blocking::RequestBuilder {
        rb.header("Cookie", &s.cookie_header)
            .header("User-Agent", &s.user_agent)
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Origin", "https://chatgpt.com")
            .header("Referer", "https://chatgpt.com/")
            .header("oai-language", "en-US")
            .header("oai-device-id", &s.device_id)
            .header("oai-session-id", &self.session_id)
    }

    /// Fetch a short-lived access token. This is also the readiness check for the session.
    pub fn access_token(&self, s: &Session) -> Result<String, TranscribeError> {
        if s.cookie_header.is_empty() {
            return Err(TranscribeError::SignedOut);
        }
        let resp = self
            .base(self.http.get(SESSION_URL), s)
            .header("Cache-Control", "no-cache")
            .send()
            .map_err(|e| classify_send_error(&e))?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().map_err(|e| classify_send_error(&e))?;
        if let Some(e) = TranscribeError::from_response(status, is_html(&headers, &body), headers.contains_key("cf-mitigated")) {
            return Err(e);
        }
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| TranscribeError::Server(status))?;
        // An empty session object ({}) means signed out.
        v.get("accessToken").and_then(|t| t.as_str()).map(str::to_string).ok_or(TranscribeError::SessionExpired)
    }

    /// Turn recorded speech into text. `audio` is a complete file (e.g. WAV), `file_name` carries its extension.
    pub fn transcribe(&self, s: &Session, audio: Vec<u8>, file_name: &str, mime: &str) -> Result<String, TranscribeError> {
        let token = self.access_token(s)?;
        let part = multipart::Part::bytes(audio).file_name(file_name.to_string()).mime_str(mime).map_err(|_| TranscribeError::Server(0))?;
        let form = multipart::Form::new().part("file", part).text("model", "whisper-1");
        let resp = self
            .base(self.http.post(TRANSCRIBE_URL), s)
            .bearer_auth(token)
            .header("oai-client-build-number", "5503767")
            .header("x-openai-target-path", "/backend-api/transcribe")
            .header("x-openai-target-route", "/backend-api/transcribe")
            .multipart(form)
            .send()
            .map_err(|e| classify_send_error(&e))?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().map_err(|e| classify_send_error(&e))?;
        if let Some(e) = TranscribeError::from_response(status, is_html(&headers, &body), headers.contains_key("cf-mitigated")) {
            return Err(e);
        }
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| TranscribeError::Server(status))?;
        Ok(["text", "transcript", "message"]
            .iter()
            .find_map(|k| v.get(*k).and_then(|t| t.as_str()))
            .unwrap_or_default()
            .to_string())
    }
}
