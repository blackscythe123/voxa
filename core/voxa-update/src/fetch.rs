//! The HTTPS client: no cookies, no identifiers, a `Voxa/<version> (<platform>)` User-Agent, 10 s
//! to connect, 30 s idle per read, at most 5 redirects, and every hop on an allowed host.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Channel, MAX_FEED_BYTES, MAX_REDIRECTS, Manifest, Offer, Origin, UpdateError, stage, verify_feed};

pub struct Client {
    http: reqwest::blocking::Client,
    origin: Origin,
}

impl Client {
    /// `version` and `platform` only ("1.0.0", "Windows"): nothing that identifies the user.
    pub fn new(origin: Origin, version: &str, platform: &str) -> Result<Self, UpdateError> {
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
            .connect_timeout(Duration::from_secs(10))
            // Blocking reqwest applies this to the request and to each body read: 30 s idle.
            .timeout(Duration::from_secs(30))
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
        if !self.origin.hop_allowed(url) {
            return Err(UpdateError::RefusedHost);
        }
        let resp = self.http.get(url).send().map_err(map_err)?;
        if !self.origin.hop_allowed(resp.url().as_str()) {
            return Err(UpdateError::RefusedHost);
        }
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(UpdateError::Http(status));
        }
        Ok(resp)
    }

    /// The raw feed bytes, at most MAX_FEED_BYTES.
    pub fn fetch_feed(&self) -> Result<Vec<u8>, UpdateError> {
        let resp = self.get(&self.origin.feed_url)?;
        let mut out = Vec::new();
        resp.take(MAX_FEED_BYTES + 1).read_to_end(&mut out).map_err(|_| UpdateError::Offline)?;
        if out.len() as u64 > MAX_FEED_BYTES {
            return Err(UpdateError::TooLarge);
        }
        Ok(out)
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
