//! The HTTPS client: no cookies, no identifiers, a `Voxa/<version> (<platform>)` User-Agent, 10 s
//! to connect, 30 s idle per read, at most 5 redirects, and every hop on an allowed host.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Channel, CheckState, MAX_FEED_BYTES, MAX_REDIRECTS, Manifest, Offer, Origin, UpdateError, stage, verify_feed};

pub struct Client {
    http: reqwest::blocking::Client,
    origin: Origin,
}

impl Client {
    /// `version` and `platform` only ("1.0.0", "Windows"): nothing that identifies the user.
    pub fn new(origin: Origin, version: &str, platform: &str) -> Result<Self, UpdateError> {
        Self::with_timeouts(origin, version, platform, Duration::from_secs(10), Duration::from_secs(30))
    }

    /// The last look at the feed right before an install: short timeouts, so nobody waits on a
    /// dead network (3 s to connect, 6 s in all).
    pub fn quick(origin: Origin, version: &str, platform: &str) -> Result<Self, UpdateError> {
        Self::with_timeouts(origin, version, platform, Duration::from_secs(3), Duration::from_secs(6))
    }

    fn with_timeouts(origin: Origin, version: &str, platform: &str, connect: Duration, total: Duration) -> Result<Self, UpdateError> {
        let policy_origin = origin.clone();
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if policy_origin.hop_allowed(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.error("redirect to a host that is not allowed")
            }
        });
        let http = reqwest::blocking::Client::builder()
            .user_agent(format!("Voxa/{version} ({platform})"))
            .connect_timeout(connect)
            // Blocking reqwest applies this to the request and to each body read: 30 s idle.
            .timeout(total)
            .redirect(policy)
            .https_only(!origin.allow_plain_http)
            .referer(false)
            .build()
            .map_err(|_| UpdateError::Io("client".into()))?;
        Ok(Self { http, origin })
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::Response, UpdateError> {
        self.get_if(url, None, None)
    }

    /// A GET, conditional when validators are given: a 304 comes back as Ok with status 304.
    fn get_if(&self, url: &str, etag: Option<&str>, last_modified: Option<&str>) -> Result<reqwest::blocking::Response, UpdateError> {
        if !self.origin.hop_allowed(url) {
            return Err(UpdateError::RefusedHost);
        }
        let mut req = self.http.get(url);
        if let Some(e) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, e);
        }
        if let Some(l) = last_modified {
            req = req.header(reqwest::header::IF_MODIFIED_SINCE, l);
        }
        let resp = req.send().map_err(map_err)?;
        if !self.origin.hop_allowed(resp.url().as_str()) {
            return Err(UpdateError::RefusedHost);
        }
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) && !(status == 304 && (etag.is_some() || last_modified.is_some())) {
            return Err(UpdateError::Http(status));
        }
        Ok(resp)
    }

    /// The raw feed bytes, at most MAX_FEED_BYTES.
    pub fn fetch_feed(&self) -> Result<Vec<u8>, UpdateError> {
        let resp = self.get(&self.origin.feed_url)?;
        read_feed(resp)
    }

    /// The feed through the cache in `state`: the stored ETag / Last-Modified go with the request,
    /// and a 304 (no body) serves the stored copy. Returns the bytes and whether they were fresh.
    /// GitHub's release asset host answers these with 304 (checked 2026-11-28).
    pub fn fetch_feed_cached(&self, state: &mut CheckState) -> Result<(Vec<u8>, bool), UpdateError> {
        let cached = state.cached_feed();
        let (etag, lm) = if cached.is_some() { (state.etag.clone(), state.last_modified.clone()) } else { (None, None) };
        let resp = self.get_if(&self.origin.feed_url, etag.as_deref(), lm.as_deref())?;
        if resp.status().as_u16() == 304 {
            return cached.map(|c| (c, false)).ok_or(UpdateError::Http(304));
        }
        let header = |n: reqwest::header::HeaderName| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
        let (new_etag, new_lm) = (header(reqwest::header::ETAG), header(reqwest::header::LAST_MODIFIED));
        let body = read_feed(resp)?;
        state.store_feed(&body, new_etag, new_lm);
        Ok((body, true))
    }

    /// A scheduled or manual check through the cache, updating `state` (serial, schedule,
    /// backoff). The caller decides first with `state.due()` / `state.manual_allowed()`.
    pub fn check_with_state(&self, keys: &[(u8, [u8; 32])], channel: Channel, state: &mut CheckState, now_ms: u64, manual: bool) -> Result<Manifest, UpdateError> {
        let result = self
            .fetch_feed_cached(state)
            .and_then(|(bytes, _)| verify_feed(&bytes, keys, channel, state.highest_serial, &self.origin));
        // A refused feed is an answer (the check worked), not a reason to retry within the hour.
        let ok = match &result {
            Ok(_) => true,
            Err(e) => e.is_refusal(),
        };
        if let Ok(m) = &result {
            state.highest_serial = crate::policy::next_serial(state.highest_serial, m.serial);
        }
        state.record(now_ms, ok, manual);
        result
    }

    /// Right before an install: one conditional request (a 304 when nothing changed) to confirm
    /// the feed still offers the staged file. `Withdrawn` when the feed is paused (the kill
    /// switch), names another file, or can't be confirmed as Voxa's; `Unknown` when the network
    /// didn't answer (the staged file is verified, so the install may go ahead).
    pub fn confirm_offer(&self, keys: &[(u8, [u8; 32])], channel: Channel, state: &mut CheckState, staged: &Offer) -> Confirm {
        let result = self
            .fetch_feed_cached(state)
            .and_then(|(bytes, _)| verify_feed(&bytes, keys, channel, state.highest_serial, &self.origin));
        match result {
            Ok(m) => {
                state.highest_serial = crate::policy::next_serial(state.highest_serial, m.serial);
                if crate::policy::still_offered(&m, staged) { Confirm::StillOffered } else { Confirm::Withdrawn }
            }
            Err(e) if e.is_refusal() => Confirm::Withdrawn,
            Err(_) => Confirm::Unknown,
        }
    }

    /// Fetch and verify the feed: the verified manifest, or why not.
    pub fn check(&self, keys: &[(u8, [u8; 32])], channel: Channel, highest_serial: u64) -> Result<Manifest, UpdateError> {
        let bytes = self.fetch_feed()?;
        verify_feed(&bytes, keys, channel, highest_serial, &self.origin)
    }

    /// Download an offer into `dir`, verified while streaming. The declared size is checked
    /// against the cap before any byte is requested.
    pub fn download(&self, offer: &Offer, dir: &Path) -> Result<PathBuf, UpdateError> {
        if offer.size > crate::policy::cap_for(&offer.asset_key) {
            return Err(UpdateError::TooLarge);
        }
        // Once per version: a file already staged and still verified is never fetched again.
        if let Some(done) = stage::already_staged(dir, offer) {
            return Ok(done);
        }
        let v = crate::Version::parse(&offer.version).ok_or(UpdateError::BadManifest)?;
        if !crate::feed::asset_url_ok(&offer.url, &v, &self.origin) {
            return Err(UpdateError::BadManifest);
        }
        let resp = self.get(&offer.url)?;
        if let Some(len) = resp.content_length() {
            if len > offer.size {
                return Err(UpdateError::TooLarge);
            }
        }
        stage::stage_from(resp, dir, offer)
    }
}

/// What the last look at the feed before an install found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    StillOffered,
    Withdrawn,
    Unknown,
}

fn read_feed(resp: reqwest::blocking::Response) -> Result<Vec<u8>, UpdateError> {
    let mut out = Vec::new();
    resp.take(MAX_FEED_BYTES + 1).read_to_end(&mut out).map_err(|_| UpdateError::Offline)?;
    if out.len() as u64 > MAX_FEED_BYTES {
        return Err(UpdateError::TooLarge);
    }
    Ok(out)
}

fn map_err(e: reqwest::Error) -> UpdateError {
    if e.is_redirect() {
        UpdateError::RefusedHost
    } else if let Some(s) = e.status() {
        UpdateError::Http(s.as_u16())
    } else {
        UpdateError::Offline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::tests::{TEST_SEED_K1, desktop_payload, sign, test_keys};
    use std::io::Write;
    use std::net::TcpListener;

    /// A tiny local HTTP server: each request path maps to (status, extra headers, body).
    fn serve(routes: Vec<(&'static str, u16, String, Vec<u8>)>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming().take(20) {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, headers, body) = routes
                    .iter()
                    .find(|r| r.0 == path)
                    .map(|r| (r.1, r.2.clone(), r.3.clone()))
                    .unwrap_or((404, String::new(), b"no".to_vec()));
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        format!("{}://{addr}", "http")
    }

    fn origin(base: &str) -> Origin {
        Origin::test(&format!("{base}/feed/desktop.json")).unwrap()
    }

    #[test]
    fn a_redirect_to_an_unlisted_host_or_http_is_refused() {
        // Redirect off the allowed host.
        let base = serve(vec![("/feed/desktop.json", 302, "Location: http://127.0.0.2:9/evil.json\r\n".into(), vec![])]);
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        assert_eq!(c.fetch_feed(), Err(UpdateError::RefusedHost));
        // Production never makes a plain http request at all.
        let gh = Client::new(Origin::github(Channel::Desktop), "1.0.0", "Test").unwrap();
        let plain = format!("{}://github.com/blackscythe123/voxa/releases/download/update-feed/desktop.json", "http");
        assert_eq!(gh.get(&plain).err(), Some(UpdateError::RefusedHost));
        assert_eq!(gh.get("https://example.com/x").err(), Some(UpdateError::RefusedHost));
        // Too many redirects.
        let loopy = serve(vec![("/feed/desktop.json", 302, "Location: /feed/desktop.json\r\n".into(), vec![])]);
        let c = Client::new(origin(&loopy), "1.0.0", "Test").unwrap();
        assert!(c.fetch_feed().is_err());
    }

    #[test]
    fn a_missing_feed_is_couldnt_check_not_an_alarm() {
        let base = serve(vec![]);
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        let e = c.check(&test_keys(), Channel::Desktop, 0).unwrap_err();
        assert_eq!(e, UpdateError::Http(404));
        assert!(!e.is_refusal());
        let off = Client::new(Origin::test(&format!("{}://127.0.0.1:1/f/desktop.json", "http")).unwrap(), "1.0.0", "Test").unwrap();
        assert_eq!(off.fetch_feed(), Err(UpdateError::Offline));
    }

    /// A local server that answers the feed with an ETag (304 when it matches) and the installer,
    /// counting the requests each path gets.
    fn counting_server(feed: Vec<u8>, installer: Vec<u8>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("{}://{}", "http", l.local_addr().unwrap());
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = log.clone();
        std::thread::spawn(move || {
            for s in l.incoming().take(30) {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let conditional = req.contains("if-none-match: \"v1\"");
                seen.lock().unwrap().push(format!("{path}{}", if conditional { " (if-none-match)" } else { "" }));
                if path.ends_with("desktop.json") && conditional {
                    let _ = write!(s, "HTTP/1.1 304 Not Modified\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let body = if path.ends_with("desktop.json") { feed.clone() } else { installer.clone() };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        (base, log)
    }

    fn local_feed(base: &str, installer: &[u8]) -> Vec<u8> {
        let mut p = desktop_payload("1.0.1", 7);
        p["notes_url"] = format!("{base}/feed/notes").into();
        p["assets"] = serde_json::json!({ "windows-x64": { "url": format!("{base}/feed/Voxa-Setup-1.0.1.exe"),
            "sha256": crate::stage::sha256_hex(installer), "size": installer.len() } });
        sign(TEST_SEED_K1, 1, &serde_json::to_vec(&p).unwrap())
    }

    #[test]
    fn a_304_keeps_the_cached_feed() {
        let feed = b"{\"key_id\":1,\"payload\":\"x\",\"sig\":\"y\"}".to_vec();
        let (base, log) = counting_server(feed.clone(), Vec::new());
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        let mut st = CheckState::default();
        st.init(1_000, 0);
        let (first, fresh) = c.fetch_feed_cached(&mut st).unwrap();
        assert!(fresh);
        assert_eq!(first, feed);
        assert_eq!(st.etag.as_deref(), Some("\"v1\""));
        let (second, fresh) = c.fetch_feed_cached(&mut st).unwrap();
        assert!(!fresh, "a 304 served the stored copy");
        assert_eq!(second, feed);
        let reqs = log.lock().unwrap().clone();
        assert_eq!(reqs, vec!["/feed/desktop.json".to_string(), "/feed/desktop.json (if-none-match)".to_string()]);
        // Without a stored body, no validators are sent (a 304 could not be served).
        let mut empty = CheckState::default();
        empty.etag = Some("\"v1\"".into());
        let (_, fresh) = c.fetch_feed_cached(&mut empty).unwrap();
        assert!(fresh);
    }

    #[test]
    fn a_verified_download_is_never_fetched_again() {
        let installer = vec![9u8; 50_000];
        let (base, log) = counting_server(Vec::new(), installer.clone());
        let o = Offer {
            version: "1.0.1".into(),
            asset_key: crate::ASSET_WINDOWS.into(),
            url: format!("{base}/feed/Voxa-Setup-1.0.1.exe"),
            sha256: crate::stage::sha256_hex(&installer),
            size: installer.len() as u64,
            auto: true,
            ..Offer::default()
        };
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        let dir = std::env::temp_dir().join(format!("voxa-update-once-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p1 = c.download(&o, &dir).unwrap();
        let p2 = c.download(&o, &dir).unwrap();
        let p3 = c.download(&o, &dir).unwrap();
        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
        assert_eq!(log.lock().unwrap().len(), 1, "one download for the version");
        // A tampered staged file is not "verified": it is fetched again (and replaced).
        std::fs::write(&p1, b"tampered").unwrap();
        let p4 = c.download(&o, &dir).unwrap();
        assert!(crate::stage::verify_file(&p4, &o).is_ok());
        assert_eq!(log.lock().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_check_with_state_records_backoff_and_serial() {
        let installer = vec![3u8; 10];
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("{}://{}", "http", l.local_addr().unwrap());
        let feed = local_feed(&base, &installer);
        std::thread::spawn(move || {
            for s in l.incoming().take(3) {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", feed.len());
                let _ = s.write_all(&feed);
            }
        });
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        let mut st = CheckState::default();
        st.init(1_000, 0);
        let m = c.check_with_state(&test_keys(), Channel::Desktop, &mut st, 5_000, false).unwrap();
        assert_eq!(m.serial, 7);
        assert_eq!(st.highest_serial, 7);
        assert_eq!(st.failures, 0);
        assert!(!st.due(5_000 + crate::schedule::HOUR_MS));
        // Offline: a failure that backs off an hour.
        let off = Client::new(Origin::test(&format!("{}://127.0.0.1:1/f/desktop.json", "http")).unwrap(), "1.0.0", "Test").unwrap();
        let mut st2 = CheckState::default();
        st2.init(1_000, 0);
        assert!(off.check_with_state(&test_keys(), Channel::Desktop, &mut st2, 9_000, false).is_err());
        assert_eq!(st2.failures, 1);
        assert_eq!(st2.next_due_ms, 9_000 + crate::schedule::HOUR_MS);
    }

    /// Serves `feed` at /feed/desktop.json for every request.
    fn feed_server(feed: Vec<u8>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("{}://{}", "http", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming().take(5) {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", feed.len());
                let _ = s.write_all(&feed);
            }
        });
        base
    }

    #[test]
    fn a_paused_feed_stops_a_staged_install_at_the_last_check() {
        let installer = vec![5u8; 64];
        let staged_for = |base: &str| {
            let m: Manifest = serde_json::from_value({
                let mut p = desktop_payload("1.0.1", 7);
                p["notes_url"] = format!("{base}/feed/notes").into();
                p["assets"] = serde_json::json!({ "windows-x64": { "url": format!("{base}/feed/Voxa-Setup-1.0.1.exe"),
                    "sha256": crate::stage::sha256_hex(&installer), "size": installer.len() } });
                p
            })
            .unwrap();
            crate::policy::offer(&m, crate::Version(1, 0, 0), crate::ASSET_WINDOWS).unwrap()
        };
        // The live feed still offers the file.
        let live = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let b = format!("{}://{}", "http", l.local_addr().unwrap());
            let feed = local_feed(&b, &installer);
            std::thread::spawn(move || {
                for s in l.incoming().take(3) {
                    let Ok(mut s) = s else { continue };
                    let mut buf = [0u8; 4096];
                    let _ = s.read(&mut buf);
                    let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", feed.len());
                    let _ = s.write_all(&feed);
                }
            });
            b
        };
        let c = Client::quick(origin(&live), "1.0.0", "Test").unwrap();
        let mut st = CheckState::default();
        assert_eq!(c.confirm_offer(&test_keys(), Channel::Desktop, &mut st, &staged_for(&live)), Confirm::StillOffered);
        assert_eq!(st.highest_serial, 7);
        // The owner published a paused feed: the staged install is withdrawn.
        let paused = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let b = format!("{}://{}", "http", l.local_addr().unwrap());
            let mut p = desktop_payload("1.0.1", 8);
            p["notes_url"] = format!("{b}/feed/notes").into();
            p["paused"] = true.into();
            p["assets"] = serde_json::json!({ "windows-x64": { "url": format!("{b}/feed/Voxa-Setup-1.0.1.exe"),
                "sha256": crate::stage::sha256_hex(&installer), "size": installer.len() } });
            let feed = sign(TEST_SEED_K1, 1, &serde_json::to_vec(&p).unwrap());
            std::thread::spawn(move || {
                for s in l.incoming().take(3) {
                    let Ok(mut s) = s else { continue };
                    let mut buf = [0u8; 4096];
                    let _ = s.read(&mut buf);
                    let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", feed.len());
                    let _ = s.write_all(&feed);
                }
            });
            b
        };
        let c = Client::quick(origin(&paused), "1.0.0", "Test").unwrap();
        let mut st = CheckState::default();
        assert_eq!(c.confirm_offer(&test_keys(), Channel::Desktop, &mut st, &staged_for(&paused)), Confirm::Withdrawn);
        // A feed that isn't Voxa's withdraws it too; no network is only "unknown".
        let forged = feed_server(sign(crate::feed::tests::TEST_SEED_OTHER, 1, b"{}"));
        let c = Client::quick(origin(&forged), "1.0.0", "Test").unwrap();
        assert_eq!(c.confirm_offer(&test_keys(), Channel::Desktop, &mut CheckState::default(), &staged_for(&forged)), Confirm::Withdrawn);
        let off = Client::quick(Origin::test(&format!("{}://127.0.0.1:1/f/desktop.json", "http")).unwrap(), "1.0.0", "Test").unwrap();
        assert_eq!(off.confirm_offer(&test_keys(), Channel::Desktop, &mut CheckState::default(), &staged_for(&live)), Confirm::Unknown);
    }

    #[test]
    fn a_signed_local_feed_checks_and_downloads_end_to_end() {
        let installer = vec![42u8; 100_000];
        let sha = crate::stage::sha256_hex(&installer);
        // The base URL is only known after binding, so serve twice: first find a port.
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let base = format!("{}://{addr}", "http");
        let mut p = desktop_payload("1.0.1", 7);
        p["notes_url"] = format!("{base}/feed/notes").into();
        p["assets"]["windows-x64"] = serde_json::json!({ "url": format!("{base}/feed/Voxa-Setup-1.0.1.exe"), "sha256": sha, "size": installer.len() });
        p["assets"]["macos-universal"] = serde_json::json!({ "url": format!("{base}/feed/Voxa-1.0.1-mac.dmg"), "sha256": sha, "size": 1 });
        let feed = sign(TEST_SEED_K1, 1, &serde_json::to_vec(&p).unwrap());
        let inst = installer.clone();
        std::thread::spawn(move || {
            for s in l.incoming().take(4) {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let body = if req.contains("desktop.json") { feed.clone() } else { inst.clone() };
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        let c = Client::new(origin(&base), "1.0.0", "Test").unwrap();
        let m = c.check(&test_keys(), Channel::Desktop, 0).unwrap();
        let o = crate::policy::offer(&m, crate::Version(1, 0, 0), crate::ASSET_WINDOWS).unwrap();
        let dir = std::env::temp_dir().join(format!("voxa-update-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = c.download(&o, &dir).unwrap();
        assert!(crate::stage::verify_file(&path, &o).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
