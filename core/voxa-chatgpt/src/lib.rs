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

    /// Save the session where the sign-in window keeps it (same format). Atomic and safe to call
    /// from several threads or processes at once: each write goes to its own temporary file, is
    /// flushed to disk, then renamed over `session.json`, so a reader never sees a mix of two.
    /// Callers that must not let an older header win hold [`SESSION_FILE_LOCK`] around their
    /// whole take, check and save.
    pub fn save_to_voxa_dir(&self, dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&SavedSession { cookie_header: self.cookie_header.clone(), user_agent: self.user_agent.clone() })?;
        write_unique_then_rename(&dir.join("session.json"), &bytes)
    }

    /// Load the session the current Electron app saved (`%APPDATA%\voxa`), for migration and testing.
    pub fn from_electron_dir(dir: &Path) -> Option<Self> {
        let cookies: Vec<StoredCookie> = serde_json::from_slice(&std::fs::read(dir.join("chatgpt-cookies.json")).ok()?).ok()?;
        let cookie_header = cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ");
        let device_id = std::fs::read_to_string(dir.join("chatgpt-device-id.txt")).unwrap_or_default().trim().to_string();
        Some(Self { cookie_header, user_agent: DEFAULT_UA.into(), device_id })
    }
}

/// Held around every take, check and save of the saved session in this process, so two saves
/// never interleave and an older header is never written after a newer one.
pub static SESSION_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Write `bytes` to a temporary file with a name no other writer uses, flush it to disk, then
/// rename it over `path`. A failed rename removes the temporary file and reports the error.
pub fn write_unique_then_rename(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!("{name}.{}-{n}-{nanos}.tmp", std::process::id()));
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    let result = written.and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// NextAuth splits a large session token across `__Secure-next-auth.session-token.0`, `.1`, ...
/// Whether a cookie header holds a complete session token (one cookie, or every chunk from 0).
pub fn has_session_token(cookie_header: &str) -> bool {
    const BASE: &str = "__Secure-next-auth.session-token";
    let pairs: Vec<(&str, &str)> = cookie_header
        .split(';')
        .filter_map(|c| {
            let (n, v) = c.trim().split_once('=')?;
            Some((n.trim(), v.trim()))
        })
        .collect();
    if pairs.iter().any(|(n, v)| *n == BASE && !v.is_empty()) {
        return true;
    }
    let mut chunks: Vec<usize> = pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .filter_map(|(n, _)| n.strip_prefix(BASE)?.strip_prefix('.')?.parse().ok())
        .collect();
    chunks.sort_unstable();
    !chunks.is_empty() && chunks.iter().enumerate().all(|(i, c)| i == *c)
}

pub struct ChatGpt {
    http: Client,
    session_id: String,
    /// The cookie header after ChatGPT's latest Set-Cookie headers on a successful reply, tied to
    /// the header that was sent: ChatGPT re-issues the session token (90 days from now) on use.
    /// Callers save it with `take_refreshed`, or the saved sign-in dies 90 days after the
    /// original sign-in. Cookies refreshed for one session are never merged into another.
    refreshed: std::sync::Mutex<Option<Refreshed>>,
}

/// Re-issued cookies waiting to be saved: `merged` was built from `source`, the header sent.
#[derive(Clone)]
struct Refreshed {
    source: String,
    merged: String,
}

/// What a session check found. Only `NoToken` (ChatGPT answered 2xx without an access token, and
/// it wasn't Cloudflare) is a definite sign-out; a 401 or 403 may be a rate limit or a region
/// block, so it is reported as `Failed(SessionExpired)` and is never definite.
#[derive(Debug)]
pub enum SessionAnswer {
    Valid(Account),
    NoToken,
    Failed(TranscribeError),
}

/// Merge `Set-Cookie` headers into a `Cookie` header. Cookies are replaced by name; an expired or
/// emptied cookie is removed. The session token is split in chunks (`.0`, `.1`, ...): when any
/// chunk is re-issued, chunks that weren't re-issued are dropped, so an old chunk never gets glued
/// onto a new token.
pub fn merge_set_cookies(cookie_header: &str, set_cookies: &[&str]) -> String {
    merge_set_cookies_at(cookie_header, set_cookies, unix_now())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// [`merge_set_cookies`] with an explicit clock (seconds since 1970), for tests.
pub fn merge_set_cookies_at(cookie_header: &str, set_cookies: &[&str], now: i64) -> String {
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
        let mut max_age: Option<i64> = None;
        let mut expires: Option<i64> = None;
        for a in parts {
            let a = a.trim();
            let lower = a.to_ascii_lowercase();
            if let Some(x) = lower.strip_prefix("max-age=") {
                // RFC 6265 5.2.2: a value that isn't a number is ignored; one too large to hold
                // means "a very long time", never "delete".
                let x = x.trim();
                let digits = x.strip_prefix('-').unwrap_or(x);
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    max_age = Some(x.parse::<i64>().unwrap_or(if x.starts_with('-') { 0 } else { i64::MAX }));
                }
            } else if lower.starts_with("expires=") {
                expires = parse_http_date(&a[8..]);
            }
        }
        // Max-Age wins over Expires (RFC 6265 5.3); a date in the past deletes the cookie.
        let expired = v.is_empty()
            || match max_age {
                Some(x) => x <= 0,
                None => expires.is_some_and(|t| t <= now),
            };
        let n = n.trim().to_string();
        // The same name twice in one response (another Domain or Path): a live value wins over a
        // deletion, whatever the order.
        if let Some(i) = set.iter().position(|(s, _, _)| *s == n) {
            if expired && !set[i].2 {
                continue;
            }
            set.remove(i);
        }
        set.push((n, v.to_string(), expired));
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

/// Parse a cookie `Expires` date ("Sat, 28 Nov 2026 08:59:41 GMT", or with dashes as in
/// "Sat, 28-Nov-2026 08:59:41 GMT") into seconds since 1970. None if it can't be read.
pub fn parse_http_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let s = s.split_once(',').map(|(_, r)| r).unwrap_or(s);
    let parts: Vec<&str> = s.split([' ', '-']).filter(|p| !p.is_empty()).collect();
    let [day, mon, year, time, ..] = parts.as_slice() else { return None };
    let day: i64 = day.parse().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let m = MONTHS.iter().position(|x| mon.to_ascii_lowercase().starts_with(x))? as i64 + 1;
    let mut y: i64 = year.parse().ok()?;
    if y < 100 {
        y += if y < 70 { 2000 } else { 1900 };
    }
    // Dates come from the network: anything outside a real calendar is rejected, so the
    // arithmetic below can never overflow.
    if !(1601..=9999).contains(&y) {
        return None;
    }
    let hms: Vec<i64> = time.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let [h, mi, sec] = hms.as_slice() else { return None };
    if !(0..24).contains(h) || !(0..60).contains(mi) || !(0..61).contains(sec) {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let (yy, mm) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec)
}

/// Whether a merged header is worth saving over the sent one: something other than Cloudflare's
/// bot cookie (`__cf_bm`, new on nearly every reply) changed.
fn worth_saving(sent: &str, merged: &str) -> bool {
    let meaningful = |h: &str| {
        let mut v: Vec<String> = h.split(';').map(|c| c.trim().to_string()).filter(|c| !c.is_empty() && !c.starts_with("__cf_bm=")).collect();
        v.sort();
        v
    };
    meaningful(sent) != meaningful(merged)
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

/// Time allowed to upload and transcribe `bytes` of audio: 30 s, plus 60 s per started MB (a
/// slow ~130 kbit/s uplink, and ChatGPT's own time to transcribe a long take). A 70 s dictation
/// (2.3 MB of WAV) gets 210 s; it timed out at 70 s on a slow network before. The cap, 30
/// minutes, sits above what the largest upload ([`MAX_UPLOAD_BYTES`]) needs on that uplink, so
/// the premise holds for every upload. No connection at all still fails within the 10 s connect
/// timeout.
pub fn upload_timeout(bytes: usize) -> Duration {
    let mb = (bytes as u64).div_ceil(1_000_000);
    Duration::from_secs((30 + mb * 60).min(30 * 60))
}

/// The largest audio file sent in one request. ChatGPT's transcription takes about 25 MB; longer
/// recordings are sent in parts below this size (see `voxa_store::audio::upload_parts`).
pub const MAX_UPLOAD_BYTES: usize = 20_000_000;

/// Waits before trying again when the connection to ChatGPT could not even be opened (nothing
/// was sent). Any failure after the audio started going out is never retried here: one press
/// of Send never uploads the same audio twice.
pub const CONNECT_RETRY_WAITS_MS: [u64; 2] = [1500, 4000];

/// The wait before try number `attempt + 2`, or None to stop. Only a failure to connect (nothing
/// uploaded) is tried again.
pub fn retry_wait(nothing_sent: bool, attempt: usize) -> Option<Duration> {
    if !nothing_sent {
        return None;
    }
    CONNECT_RETRY_WAITS_MS.get(attempt).map(|ms| Duration::from_millis(*ms))
}

/// The audio for one request, shared between tries without copying it.
struct SharedAudio(std::sync::Arc<Vec<u8>>);

impl AsRef<[u8]> for SharedAudio {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl ChatGpt {
    /// Remember ChatGPT's re-issued cookies from a successful response (see `refreshed`). Merged
    /// onto cookies waiting from the same sent header only; anything else is replaced. Never logs
    /// values.
    fn note_cookies(&self, s: &Session, headers: &HeaderMap) {
        let sets: Vec<&str> = headers.get_all(reqwest::header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).collect();
        self.note_set_cookies(&s.cookie_header, &sets);
    }

    fn note_set_cookies(&self, sent: &str, sets: &[&str]) {
        if sets.is_empty() {
            return;
        }
        let mut r = self.refreshed.lock().unwrap_or_else(|e| e.into_inner());
        let base = match r.as_ref() {
            Some(w) if w.source == sent => w.merged.clone(),
            _ => sent.to_string(),
        };
        let merged = merge_set_cookies(&base, sets);
        if worth_saving(sent, &merged) {
            *r = Some(Refreshed { source: sent.to_string(), merged });
        } else if r.as_ref().is_some_and(|w| w.source != sent) {
            *r = None;
        }
    }

    /// The cookie header with ChatGPT's latest re-issued cookies, once, if they were refreshed
    /// from `current` (the session's header now) and still hold a whole session token. Cookies
    /// refreshed for a session that has since been replaced or signed out are dropped.
    pub fn take_refreshed(&self, current: &str) -> Option<String> {
        let w = self.refreshed.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        (w.source == current && has_session_token(&w.merged)).then_some(w.merged)
    }

    /// Forget re-issued cookies waiting to be saved (sign-in, sign-out, a new session).
    pub fn clear_refreshed(&self) {
        *self.refreshed.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            // Fails only if the TLS backend can't start; the plain client still works (every
            // request sets its own time limit or keeps the default).
            .unwrap_or_default();
        let session_id = format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default());
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
        // A small GET: a dropped connection is worth two more tries.
        let mut attempt = 0;
        let v = loop {
            match self.session_json(s) {
                Err(TranscribeError::Offline) if attempt < CONNECT_RETRY_WAITS_MS.len() => {
                    std::thread::sleep(Duration::from_millis(CONNECT_RETRY_WAITS_MS[attempt]));
                    attempt += 1;
                }
                r => break r?,
            }
        };
        // An empty session object ({}) means signed out.
        v.get("accessToken").and_then(|t| t.as_str()).map(str::to_string).ok_or(TranscribeError::SessionExpired)
    }

    /// Who is signed in, from the same session call (no extra request). Never includes tokens.
    pub fn account(&self, s: &Session) -> Result<Account, TranscribeError> {
        match self.check(s) {
            SessionAnswer::Valid(a) => Ok(a),
            SessionAnswer::NoToken => Err(TranscribeError::SessionExpired),
            SessionAnswer::Failed(e) => Err(e),
        }
    }

    /// The session check, telling a definite sign-out (`NoToken`) from a rejection that may be
    /// something else (`Failed(SessionExpired)` for a 401 or 403).
    pub fn check(&self, s: &Session) -> SessionAnswer {
        match self.session_json(s) {
            Ok(v) if v.get("accessToken").and_then(|t| t.as_str()).is_some() => SessionAnswer::Valid(Account::from_session(&v)),
            Ok(_) => SessionAnswer::NoToken,
            Err(e) => SessionAnswer::Failed(e),
        }
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
        let body = resp.text().map_err(|e| classify_send_error(&e))?;
        if let Some(e) = TranscribeError::from_response(status, is_html(&headers, &body), headers.contains_key("cf-mitigated")) {
            return Err(e);
        }
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| TranscribeError::Server(status))?;
        // Only a reply that carries a token refreshes the saved cookies: a failed or signed-out
        // reply may delete the token, and saving that would turn a blip into a sign-out.
        if v.get("accessToken").and_then(|t| t.as_str()).is_some() {
            self.note_cookies(s, &headers);
        }
        Ok(v)
    }

    /// Turn recorded speech into text. `audio` is a complete file (e.g. WAV), `file_name` carries
    /// its extension. The audio is uploaded at most once: only a failure to connect is tried
    /// again (see [`retry_wait`]).
    pub fn transcribe(&self, s: &Session, audio: Vec<u8>, file_name: &str, mime: &str) -> Result<String, TranscribeError> {
        let token = self.access_token(s)?;
        let limit = upload_timeout(audio.len());
        let audio = std::sync::Arc::new(audio);
        let mut attempt = 0;
        let resp = loop {
            let len = audio.len() as u64;
            let reader = std::io::Cursor::new(SharedAudio(audio.clone()));
            let part = multipart::Part::reader_with_length(reader, len)
                .file_name(file_name.to_string())
                .mime_str(mime)
                .map_err(|_| TranscribeError::Server(0))?;
            let form = multipart::Form::new().part("file", part).text("model", "whisper-1");
            let sent = self
                .base(self.http.post(TRANSCRIBE_URL), s)
                .bearer_auth(&token)
                .header("oai-client-build-number", "5503767")
                .header("x-openai-target-path", "/backend-api/transcribe")
                .header("x-openai-target-route", "/backend-api/transcribe")
                .multipart(form)
                .timeout(limit)
                .send();
            match sent {
                Ok(r) => break r,
                Err(e) => match retry_wait(e.is_connect(), attempt) {
                    Some(wait) => {
                        std::thread::sleep(wait);
                        attempt += 1;
                    }
                    None => return Err(classify_send_error(&e)),
                },
            }
        };
        drop(audio);
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let body = resp.text().map_err(|e| classify_send_error(&e))?;
        if let Some(e) = transcribe_reply_error(status, is_html(&headers, &body), headers.contains_key("cf-mitigated")) {
            return Err(e);
        }
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| TranscribeError::Server(status))?;
        self.note_cookies(s, &headers);
        transcript_from(&v).ok_or(TranscribeError::Server(status))
    }
}

/// How a reply to the transcribe request failed, if it did. The access token was accepted just
/// before, so a 401 or 403 here is about this request (a rate limit, a region or plan refusal),
/// not a sign-out: it never blocks recording or asks the user to sign in again.
pub fn transcribe_reply_error(status: u16, body_is_html: bool, cf_mitigated: bool) -> Option<TranscribeError> {
    match TranscribeError::from_response(status, body_is_html, cf_mitigated) {
        Some(TranscribeError::SessionExpired) => Some(TranscribeError::Server(status)),
        e => e,
    }
}

/// The transcript in a 2xx reply, or None when the reply has none: a reply in a shape this
/// client doesn't know is a server problem (the recording is kept and can be sent again), never
/// "no speech". An empty string is a real answer: ChatGPT heard nothing.
pub fn transcript_from(v: &serde_json::Value) -> Option<String> {
    ["text", "transcript"].iter().find_map(|k| v.get(*k).and_then(|t| t.as_str())).map(str::to_string)
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
        assert_eq!(upload_timeout(0), Duration::from_secs(30));
        assert_eq!(upload_timeout(200_000), Duration::from_secs(90));
        // A 71 s take (16 kHz mono 16-bit WAV, about 2.27 MB) on a slow network.
        assert_eq!(upload_timeout(2_272_000), Duration::from_secs(210));
        // A 10-minute WAV is about 19 MB: capped at 15 minutes.
        assert_eq!(upload_timeout(19_200_000), Duration::from_secs(30 + 20 * 60));
        assert_eq!(upload_timeout(100_000_000), Duration::from_secs(30 * 60));
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
    use super::{merge_set_cookies, merge_set_cookies_at, parse_http_date};

    #[test]
    fn refreshed_token_replaces_all_chunks_and_expired_cookies_go() {
        let jar = "a=1; __Secure-next-auth.session-token.0=OLD0; __Secure-next-auth.session-token.1=OLD1; __cf_bm=x";
        let merged = merge_set_cookies(jar, &[
            "__Secure-next-auth.session-token.0=NEW0; Path=/; Max-Age=7776000; Secure",
            "__cf_bm=y; Expires=Fri, 28 Nov 2036 08:59:41 GMT",
            "a=; Max-Age=0",
        ]);
        // The token came back as one chunk: the old second chunk must not survive.
        assert_eq!(merged, "__Secure-next-auth.session-token.0=NEW0; __cf_bm=y");
    }

    #[test]
    fn a_past_expires_date_deletes_and_a_future_one_keeps() {
        let now = parse_http_date("Sat, 28 Nov 2026 12:00:00 GMT").unwrap();
        let merged = merge_set_cookies_at("a=1; b=2", &["a=x; Expires=Thu, 01 Jan 1970 00:00:00 GMT", "b=y; Expires=Sun, 28-Nov-2027 08:59:41 GMT"], now);
        assert_eq!(merged, "b=y");
    }

    #[test]
    fn a_live_value_wins_over_a_deletion_of_the_same_name() {
        let both = ["t=new; Path=/", "t=; Path=/api; Max-Age=0"];
        assert_eq!(merge_set_cookies_at("t=old", &both, 0), "t=new");
        let reversed = ["t=; Path=/api; Max-Age=0", "t=new; Path=/"];
        assert_eq!(merge_set_cookies_at("t=old", &reversed, 0), "t=new");
    }

    #[test]
    fn http_dates_parse() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_http_date("Sat, 28 Nov 2026 08:59:41 GMT"), Some(1_795_856_381));
        assert_eq!(parse_http_date("nonsense"), None);
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
        let h = gpt.take_refreshed(&s.cookie_header).expect("ChatGPT re-issued cookies");
        let names: Vec<&str> = h.split("; ").filter_map(|c| c.split('=').next()).collect();
        eprintln!("refreshed header has {} cookies; token chunks: {:?}", names.len(), names.iter().filter(|n| n.contains("session-token")).collect::<Vec<_>>());
        eprintln!("token changed: {}", h.split("; ").filter(|c| c.contains("session-token")).collect::<Vec<_>>() != s.cookie_header.split("; ").filter(|c| c.contains("session-token")).collect::<Vec<_>>());
        let s2 = Session { cookie_header: h, ..s };
        let a = gpt.account(&s2).expect("second check with only the refreshed cookies");
        eprintln!("second check with the refreshed sign-in: OK ({} chars of name)", a.name.len());
    }
}

#[cfg(test)]
mod refresh_slot_tests {
    use super::*;

    const T0: &str = "__Secure-next-auth.session-token=OLD; __cf_bm=a";
    const T1: &str = "__Secure-next-auth.session-token=NEW; __cf_bm=b";

    #[test]
    fn refreshed_cookies_are_only_adopted_by_the_session_they_came_from() {
        let gpt = ChatGpt::new();
        gpt.note_set_cookies(T0, &["__Secure-next-auth.session-token=R0; Max-Age=7776000"]);
        // The user signed in again meanwhile: the old session's refresh must not leak into it.
        assert_eq!(gpt.take_refreshed(T1), None);
        assert_eq!(gpt.take_refreshed(T0), None, "taken once, even when dropped");
        gpt.note_set_cookies(T0, &["__Secure-next-auth.session-token=R0; Max-Age=7776000"]);
        assert_eq!(gpt.take_refreshed(T0).as_deref(), Some("__cf_bm=a; __Secure-next-auth.session-token=R0"));
    }

    #[test]
    fn a_new_sessions_cookies_never_merge_onto_an_old_slot() {
        let gpt = ChatGpt::new();
        gpt.note_set_cookies(T0, &["__Secure-next-auth.session-token=R0; Max-Age=7776000"]);
        // First reply for the new sign-in: built from the new header, not the old slot.
        gpt.note_set_cookies(T1, &["extra=1"]);
        let h = gpt.take_refreshed(T1).expect("new session refreshed");
        assert!(h.contains("session-token=NEW") && !h.contains("R0") && !h.contains("OLD"), "{h}");
    }

    #[test]
    fn clear_drops_waiting_cookies() {
        let gpt = ChatGpt::new();
        gpt.note_set_cookies(T0, &["__Secure-next-auth.session-token=R0; Max-Age=7776000"]);
        gpt.clear_refreshed();
        assert_eq!(gpt.take_refreshed(T0), None);
    }

    #[test]
    fn only_a_new_cf_bm_is_not_worth_saving() {
        let gpt = ChatGpt::new();
        gpt.note_set_cookies(T0, &["__cf_bm=zzz; Max-Age=1800"]);
        assert_eq!(gpt.take_refreshed(T0), None);
    }

    #[test]
    fn a_header_without_a_whole_token_is_never_handed_out() {
        let gpt = ChatGpt::new();
        gpt.note_set_cookies(T0, &["__Secure-next-auth.session-token=; Max-Age=0"]);
        assert_eq!(gpt.take_refreshed(T0), None);
    }

    #[test]
    fn session_token_must_be_whole() {
        assert!(has_session_token("a=1; __Secure-next-auth.session-token=x"));
        assert!(has_session_token("__Secure-next-auth.session-token.0=x; __Secure-next-auth.session-token.1=y"));
        assert!(!has_session_token("__Secure-next-auth.session-token.1=y"));
        assert!(!has_session_token("__Secure-next-auth.session-token="));
        assert!(!has_session_token(""));
    }

    #[test]
    fn concurrent_saves_never_leave_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("voxa-session-save-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    for j in 0..25 {
                        let s = Session { cookie_header: format!("__Secure-next-auth.session-token={}", "x".repeat(200 + i * 50 + j)), user_agent: "ua".into(), device_id: String::new() };
                        s.save_to_voxa_dir(&dir).expect("save");
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let s = Session::from_voxa_dir(&dir).expect("the file is whole");
        assert!(has_session_token(&s.cookie_header));
        let leftovers = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).count();
        assert_eq!(leftovers, 0, "no temporary files left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
