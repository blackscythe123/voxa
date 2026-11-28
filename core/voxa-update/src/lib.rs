//! Voxa's self update, shared by the desktop app and Android (through voxa-ffi).
//!
//! The order is always: fetch the signed feed, verify the signature over the raw payload bytes,
//! only then parse it, check its channel, serial and schema, decide whether it is an offer
//! ([`policy`]), download the asset to a unique temp file while hashing it, and rename it into
//! place only when its size and SHA-256 match. Nothing here installs anything: the desktop app
//! hands a verified installer to Windows, and Android hands a verified file to PackageInstaller.
//!
//! The only network peers are github.com and GitHub's release asset hosts ([`Origin::github`]).
//! Requests carry no cookies and no identifiers.

pub mod feed;
pub mod keys;
pub mod policy;
pub mod version;
mod codec;
#[cfg(feature = "net")]
pub mod fetch;
pub mod stage;

pub use codec::{b64_decode, b64_encode, hex_decode, hex_encode};
pub use feed::{Asset, Envelope, Manifest, verify_feed};
pub use policy::{Blocked, Flow, Inputs, Moment, Offer};
pub use version::Version;

/// The repo every feed, asset and release page lives under.
pub const REPO_URL: &str = "https://github.com/blackscythe123/voxa/";
/// Release assets (the versioned installer, dmg and APKs) are downloaded from here only.
pub const RELEASES_DOWNLOAD: &str = "https://github.com/blackscythe123/voxa/releases/download/";
/// The fixed prerelease tag that holds both feeds (never "latest").
pub const DESKTOP_FEED_URL: &str = "https://github.com/blackscythe123/voxa/releases/download/update-feed/desktop.json";
pub const ANDROID_FEED_URL: &str = "https://github.com/blackscythe123/voxa/releases/download/update-feed/android.json";
/// The hosts a request may reach, including every redirect hop.
pub const ALLOWED_HOSTS: [&str; 3] = ["github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"];

/// Caps are compiled in and never taken from the feed (about 2x today's artifacts).
pub const MAX_FEED_BYTES: u64 = 64 * 1024;
/// The installed Windows app is about 39 MB unpacked; the installer is smaller.
pub const MAX_WINDOWS_BYTES: u64 = 80 * 1024 * 1024;
pub const MAX_MAC_BYTES: u64 = 160 * 1024 * 1024;
pub const MAX_APK_BYTES: u64 = 15 * 1024 * 1024;
/// At most this many redirects per request.
pub const MAX_REDIRECTS: usize = 5;
/// Auto install stops after this many failed attempts at one target version.
pub const MAX_ATTEMPTS: u32 = 2;

/// The feed's platform asset keys.
pub const ASSET_WINDOWS: &str = "windows-x64";
pub const ASSET_MAC: &str = "macos-universal";
/// Android: `android-<abi>` per ABI, then this one for any device.
pub const ASSET_ANDROID_UNIVERSAL: &str = "android-universal";

/// Everything that can go wrong. Callers show "Couldn't check" (or the refused copy) and try
/// again at the next scheduled check; nothing here is an alarm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Offline,
    Http(u16),
    /// A request or redirect hop left HTTPS or the allowed hosts ("Couldn't check").
    RefusedHost,
    BadEnvelope,
    BadSignature,
    UnknownKey,
    WrongChannel,
    Replay,
    BadManifest,
    NotNewer,
    TooLarge,
    HashMismatch,
    DiskFull,
    Io(String),
}

impl UpdateError {
    /// The feed could not be confirmed as Voxa's: the "refused" copy, not "couldn't check".
    pub fn is_refusal(&self) -> bool {
        matches!(self, Self::BadSignature | Self::UnknownKey | Self::WrongChannel | Self::Replay | Self::BadManifest)
    }

    /// A short stable name for logs (never user data).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Http(_) => "http",
            Self::RefusedHost => "refused_host",
            Self::BadEnvelope => "bad_envelope",
            Self::BadSignature => "bad_signature",
            Self::UnknownKey => "unknown_key",
            Self::WrongChannel => "wrong_channel",
            Self::Replay => "replay",
            Self::BadManifest => "bad_manifest",
            Self::NotNewer => "not_newer",
            Self::TooLarge => "too_large",
            Self::HashMismatch => "hash_mismatch",
            Self::DiskFull => "disk_full",
            Self::Io(_) => "io",
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(c) => write!(f, "http {c}"),
            Self::Io(e) => write!(f, "io: {e}"),
            other => f.write_str(other.code()),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        if stage::is_disk_full(&e) { Self::DiskFull } else { Self::Io(e.kind().to_string()) }
    }
}

/// Which feed a client reads. A feed for one channel is refused on the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Desktop,
    Android,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Android => "android",
        }
    }
}

/// Where feeds and assets may come from. Production is always [`Origin::github`]; a test origin
/// exists only for test copies (desktop `test_hooks()`, Android debug builds) and is never built
/// from anything in a feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub feed_url: String,
    /// Every asset URL starts with this.
    pub asset_prefix: String,
    /// The release notes link starts with this.
    pub notes_prefix: String,
    pub hosts: Vec<String>,
    /// Test origins on a local server only; production is HTTPS on every hop.
    pub allow_plain_http: bool,
}

impl Origin {
    pub fn github(channel: Channel) -> Self {
        Self {
            feed_url: match channel {
                Channel::Desktop => DESKTOP_FEED_URL,
                Channel::Android => ANDROID_FEED_URL,
            }
            .to_string(),
            asset_prefix: RELEASES_DOWNLOAD.to_string(),
            notes_prefix: REPO_URL.to_string(),
            hosts: ALLOWED_HOSTS.iter().map(|h| h.to_string()).collect(),
            allow_plain_http: false,
        }
    }

    /// A test origin: the feed at `feed_url`, assets and notes anywhere under its folder, on that
    /// host only. Plain http is allowed only when the host is loopback or the Android emulator's
    /// host alias (10.0.2.2), never for a public host.
    pub fn test(feed_url: &str) -> Option<Self> {
        let (scheme, host, _) = split_url(feed_url)?;
        let base = &feed_url[..feed_url.rfind('/')? + 1];
        let local = matches!(host_name(host), "127.0.0.1" | "localhost" | "10.0.2.2" | "[::1]");
        if scheme != "https" && !(scheme == "http" && local) {
            return None;
        }
        Some(Self {
            feed_url: feed_url.to_string(),
            asset_prefix: base.to_string(),
            notes_prefix: base.to_string(),
            hosts: vec![host_name(host).to_string()],
            allow_plain_http: scheme == "http",
        })
    }

    /// A request (or redirect hop) to `url` is allowed: HTTPS (or a local test origin's http)
    /// and a listed host.
    pub fn hop_allowed(&self, url: &str) -> bool {
        let Some((scheme, host, _)) = split_url(url) else { return false };
        let scheme_ok = scheme == "https" || (self.allow_plain_http && scheme == "http");
        scheme_ok && self.hosts.iter().any(|h| h.eq_ignore_ascii_case(host_name(host)))
    }
}

/// `scheme://authority/rest` split, or None. Userinfo (`user@host`) is refused outright.
pub(crate) fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || !scheme.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return None;
    }
    Some((scheme, authority, path))
}

/// The host without a port.
fn host_name(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority.find(']').map_or(authority, |i| &authority[..=i]);
    }
    authority.rsplit_once(':').map_or(authority, |(h, p)| if p.bytes().all(|b| b.is_ascii_digit()) { h } else { authority })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_hops_are_https_on_the_listed_hosts_only() {
        let o = Origin::github(Channel::Desktop);
        assert!(o.hop_allowed("https://github.com/blackscythe123/voxa/releases/download/v1.0.1/Voxa-Setup-1.0.1.exe"));
        assert!(o.hop_allowed("https://release-assets.githubusercontent.com/x?sig=1"));
        assert!(o.hop_allowed("https://objects.githubusercontent.com/x"));
        assert!(!o.hop_allowed(&format!("{}://github.com/x", "http")));
        assert!(!o.hop_allowed("https://github.com.evil.example/x"));
        assert!(!o.hop_allowed("https://evil.example/github.com"));
        assert!(!o.hop_allowed("https://user@github.com/x"));
        assert!(!o.hop_allowed("ftp://github.com/x"));
    }

    #[test]
    fn a_test_origin_is_local_only_when_plain() {
        assert!(Origin::test(&format!("{}://10.0.2.2:8765/feed/android.json", "http")).is_some());
        assert!(Origin::test(&format!("{}://127.0.0.1:1/desktop.json", "http")).is_some());
        assert!(Origin::test(&format!("{}://example.com/desktop.json", "http")).is_none());
        let o = Origin::test(&format!("{}://127.0.0.1:9/f/desktop.json", "http")).unwrap();
        assert!(o.hop_allowed(&format!("{}://127.0.0.1:9/f/x.exe", "http")));
        assert!(!o.hop_allowed("https://github.com/x"));
    }
}
