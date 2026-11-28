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

#[derive(serde::Deserialize, serde::Serialize)]
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

    /// Save the session where the sign-in window keeps it (atomically; same format).
    pub fn save_to_voxa_dir(&self, dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&SavedSession { cookie_header: self.cookie_header.clone(), user_agent: self.user_agent.clone() })?;
        let tmp = dir.join("session.json.tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, dir.join("session.json"))
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
    /// The cookie header after ChatGPT's latest Set-Cookie headers, when it differs from what the
    /// caller sent: ChatGPT re-issues the session token (90 days from now) on use. Callers save it
    /// with `take_refreshed`, or the saved sign-in dies 90 days after the original sign-in.
    refreshed: std::sync::Mutex<Option<String>>,
}

/// Merge `Set-Cookie` headers into a `Cookie` header. Cookies are replaced by name; an expired or
/// emptied cookie is removed. The session token is split in chunks (`.0`, `.1`, ...): when any
/// chunk is re-issued, chunks that weren't re-issued are dropped, so an old chunk never gets glued
/// onto a new token.
pub fn merge_set_cookies(cookie_header: &str, set_cookies: &[&str]) -> String {
    const TOKEN: &str = "__Secure-next-auth.session-token";
    let mut jar: Vec<(String, String)> = cookie_header
        .split(';')
        .filter_map(|c| {
            let (n, v) = c.trim().split_once('=')?;
            (!n.is_empty()).then(|| (n.to_string(), v.to_string()))
        })
        .collect();
    let mut set: Vec<(String, String, bool)> = Vec::new();
    for sc in set_cookies {
        let mut parts = sc.split(';');
        let Some((n, v)) = parts.next().and_then(|nv| nv.trim().split_once('=')) else { continue };
        let expired = v.is_empty()
            || parts.any(|a| {
                let a = a.trim().to_ascii_lowercase();
                a.strip_prefix("max-age=").and_then(|x| x.trim().parse::<i64>().ok()).is_some_and(|x| x <= 0)
            });
        set.push((n.trim().to_string(), v.to_string(), expired));
    }
    if set.iter().any(|(n, _, _)| n.starts_with(TOKEN)) {
        jar.retain(|(n, _)| !n.starts_with(TOKEN) || set.iter().any(|(s, _, _)| s == n));
    }
    for (n, v, expired) in set {
        jar.retain(|(k, _)| *k != n);
        if !expired {
            jar.push((n, v));
        }
    }
    jar.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ")
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

/// Time allowed to upload and transcribe `bytes` of audio: 30 s, plus 20 s per MB (a slow
/// ~400 kbit/s uplink), at most 15 minutes. The client's 20 s default is for small requests;
/// a long dictation would never fit in it.
pub fn upload_timeout(bytes: usize) -> Duration {
    let secs = 30 + (bytes as u64 / 1_000_000) * 20;
    Duration::from_secs(secs.min(15 * 60))
}

impl ChatGpt {
    /// Remember ChatGPT's re-issued cookies from a response (see `refreshed`). Never logs values.
    fn note_cookies(&self, s: &Session, headers: &HeaderMap) {
        let sets: Vec<&str> = headers.get_all(reqwest::header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).collect();
        if sets.is_empty() {
            return;
        }
        let mut r = self.refreshed.lock().unwrap_or_else(|e| e.into_inner());
        let base = r.clone().unwrap_or_else(|| s.cookie_header.clone());
        let merged = merge_set_cookies(&base, &sets);
        if merged != s.cookie_header {
            *r = Some(merged);
        }
    }

    /// The cookie header with ChatGPT's latest re-issued cookies, once; save it as the session.
    pub fn take_refreshed(&self) -> Option<String> {
        self.refreshed.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("http client");
        let session_id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        Self { http, session_id , refreshed: std::sync::Mutex::new(None) }
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
        let v = self.session_json(s)?;
        // An empty session object ({}) means signed out.
        v.get("accessToken").and_then(|t| t.as_str()).map(str::to_string).ok_or(TranscribeError::SessionExpired)
    }

    /// Who is signed in, from the same session call (no extra request). Never includes tokens.
    pub fn account(&self, s: &Session) -> Result<Account, TranscribeError> {
        let v = self.session_json(s)?;
        if v.get("accessToken").is_none() {
            return Err(TranscribeError::SessionExpired);
        }
        Ok(Account::from_session(&v))
    }

    fn session_json(&self, s: &Session) -> Result<serde_json::Value, TranscribeError> {
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
        self.note_cookies(s, &headers);
        let body = resp.text().map_err(|e| classify_send_error(&e))?;
        if let Some(e) = TranscribeError::from_response(status, is_html(&headers, &body), headers.contains_key("cf-mitigated")) {
            return Err(e);
        }
        serde_json::from_str(&body).map_err(|_| TranscribeError::Server(status))
    }

    /// Turn recorded speech into text. `audio` is a complete file (e.g. WAV), `file_name` carries its extension.
    pub fn transcribe(&self, s: &Session, audio: Vec<u8>, file_name: &str, mime: &str) -> Result<String, TranscribeError> {
        let token = self.access_token(s)?;
        let limit = upload_timeout(audio.len());
        let part = multipart::Part::bytes(audio).file_name(file_name.to_string()).mime_str(mime).map_err(|_| TranscribeError::Server(0))?;
        let form = multipart::Form::new().part("file", part).text("model", "whisper-1");
        let resp = self
            .base(self.http.post(TRANSCRIBE_URL), s)
            .bearer_auth(token)
            .header("oai-client-build-number", "5503767")
            .header("x-openai-target-path", "/backend-api/transcribe")
            .header("x-openai-target-route", "/backend-api/transcribe")
            .multipart(form)
            .timeout(limit)
            .send()
            .map_err(|e| classify_send_error(&e))?;
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        self.note_cookies(s, &headers);
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

/// The signed-in ChatGPT account, as shown in Settings.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Account {
    pub name: String,
    pub email: String,
    /// Profile picture URL.
    pub image: String,
    /// "free", "plus", "pro", "team"...
    pub plan: String,
}

impl Account {
    pub fn from_session(v: &serde_json::Value) -> Self {
        let user = &v["user"];
        let s = |x: &serde_json::Value| x.as_str().unwrap_or("").to_string();
        Self {
            name: s(&user["name"]),
            email: s(&user["email"]),
            image: if user["image"].is_string() { s(&user["image"]) } else { s(&user["picture"]) },
            plan: s(&v["account"]["planType"]),
        }
    }

    /// "ChatGPT Plus", "ChatGPT Free".
    pub fn plan_label(&self) -> String {
        let mut p = self.plan.clone();
        if let Some(f) = p.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        if p.is_empty() { "ChatGPT".into() } else { format!("ChatGPT {p}") }
    }
}

#[cfg(test)]
mod account_tests {
    use super::Account;

    #[test]
    fn account_from_session_json() {
        let v = serde_json::json!({
            "user": {"name": "Sam Example", "email": "sam@example.com", "image": "https://cdn.example/a.png"},
            "account": {"planType": "plus"},
            "accessToken": "secret"
        });
        let a = Account::from_session(&v);
        assert_eq!(a.name, "Sam Example");
        assert_eq!(a.plan_label(), "ChatGPT Plus");
        assert!(!format!("{a:?}").contains("secret"), "never carries the token");
    }
}

#[cfg(test)]
mod timeout_tests {
    use super::upload_timeout;
    use std::time::Duration;

    #[test]
    fn upload_time_grows_with_the_recording() {
        assert_eq!(upload_timeout(200_000), Duration::from_secs(30));
        // A 10-minute WAV at 16 kHz mono 16-bit is about 19 MB.
        assert_eq!(upload_timeout(19_200_000), Duration::from_secs(30 + 19 * 20));
        assert_eq!(upload_timeout(500_000_000), Duration::from_secs(15 * 60));
    }
}

#[cfg(test)]
mod cookie_probe {
    use super::*;

    /// Manual, read-only: does ChatGPT send refreshed session cookies? Prints only cookie NAMES and
    /// their expiry attributes, and the session's "expires" date; never any cookie value.
    /// cargo test -p voxa-chatgpt cookie_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn what_chatgpt_sends_back() {
        let dir = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("voxa-next");
        let s = Session::from_voxa_dir(&dir).expect("signed in on desktop");
        let stored: Vec<&str> = s.cookie_header.split("; ").filter_map(|c| c.split('=').next()).collect();
        eprintln!("cookie names Voxa sends ({}): {}", stored.len(), stored.join(", "));
        let gpt = ChatGpt::new();
        let resp = gpt.base(gpt.http.get(SESSION_URL), &s).header("Cache-Control", "no-cache").send().expect("request");
        eprintln!("HTTP {}", resp.status());
        for v in resp.headers().get_all("set-cookie") {
            let v = v.to_str().unwrap_or("");
            let name = v.split('=').next().unwrap_or("?");
            let attrs: Vec<&str> = v.split(';').skip(1).map(str::trim).filter(|a| {
                let l = a.to_ascii_lowercase();
                l.starts_with("expires") || l.starts_with("max-age")
            }).collect();
            eprintln!("  set-cookie: {name}  [{}]", attrs.join("; "));
        }
        let body: serde_json::Value = resp.json().unwrap_or_default();
        eprintln!("session expires (from ChatGPT): {}", body.get("expires").and_then(|e| e.as_str()).unwrap_or("(not given)"));
    }
}

#[cfg(test)]
mod cookie_tests {
    use super::merge_set_cookies;

    #[test]
    fn refreshed_token_replaces_all_chunks_and_expired_cookies_go() {
        let jar = "a=1; __Secure-next-auth.session-token.0=OLD0; __Secure-next-auth.session-token.1=OLD1; __cf_bm=x";
        let merged = merge_set_cookies(jar, &[
            "__Secure-next-auth.session-token.0=NEW0; Path=/; Max-Age=7776000; Secure",
            "__cf_bm=y; Expires=Sat, 28 Nov 2026 08:59:41 GMT",
            "a=; Max-Age=0",
        ]);
        // The token came back as one chunk: the old second chunk must not survive.
        assert_eq!(merged, "__Secure-next-auth.session-token.0=NEW0; __cf_bm=y");
    }

    #[test]
    fn nothing_to_merge_keeps_the_header() {
        assert_eq!(merge_set_cookies("a=1; b=2", &[]), "a=1; b=2");
        assert_eq!(merge_set_cookies("a=1; b=2", &["c=3"]), "a=1; b=2; c=3");
    }
}

#[cfg(test)]
mod refresh_live {
    use super::*;

    /// Manual, read-only (nothing is saved): the refreshed cookie header from one request must
    /// work for the next. Prints cookie names only.
    /// cargo test -p voxa-chatgpt refresh_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn refreshed_sign_in_works_for_the_next_request() {
        let dir = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("voxa-next");
        let s = Session::from_voxa_dir(&dir).expect("signed in on desktop");
        let gpt = ChatGpt::new();
        gpt.account(&s).expect("first check");
        let h = gpt.take_refreshed().expect("ChatGPT re-issued cookies");
        let names: Vec<&str> = h.split("; ").filter_map(|c| c.split('=').next()).collect();
        eprintln!("refreshed header has {} cookies; token chunks: {:?}", names.len(), names.iter().filter(|n| n.contains("session-token")).collect::<Vec<_>>());
        eprintln!("token changed: {}", h.split("; ").filter(|c| c.contains("session-token")).collect::<Vec<_>>() != s.cookie_header.split("; ").filter(|c| c.contains("session-token")).collect::<Vec<_>>());
        let s2 = Session { cookie_header: h, ..s };
        let a = gpt.account(&s2).expect("second check with only the refreshed cookies");
        eprintln!("second check with the refreshed sign-in: OK ({} chars of name)", a.name.len());
    }
}
