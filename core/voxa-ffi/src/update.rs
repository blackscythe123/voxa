//! Self update for Kotlin (notify/Updater.kt): the same verifier, version rules and capped,
//! verified download the desktop uses (`voxa-update`). Kotlin never verifies a signature itself
//! (java.security Ed25519 needs API 33; minSdk is 29).
//!
//! Trap: `core_version()` is the workspace version, not the APK's. The caller always passes
//! `BuildConfig.VERSION_CODE`; nothing here uses this crate's own version for the comparison.

use voxa_update::{Channel, Origin, UpdateError, fetch::Client, keys, policy};

/// A verified update Android may install. Mirrors `voxa_update::Offer`.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UpdateOffer {
    pub version: String,
    pub version_code: u64,
    /// The feed's asset key ("android-arm64-v8a", "android-universal").
    pub abi: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub notes_url: String,
    pub serial: u64,
    /// SHA-256 of the signing certificate the APK carries ("" when the feed gives none).
    pub signer_sha256: String,
    /// False below the feed's `min_auto_from`: offered as a download from the release page only.
    pub auto: bool,
}

/// One check: the offer, if any, and the serial to remember (the highest verified so far).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UpdateCheck {
    pub offer: Option<UpdateOffer>,
    pub serial: u64,
    pub paused: bool,
}

/// What went wrong, as the app needs it (`voxa_update::UpdateError` folded into the five things
/// the screens say). The full typed error is logged by the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum UpdateFailure {
    /// No network, a timeout, a 404 or another HTTP answer, or a host outside the allow list:
    /// "Couldn't check", try again later.
    #[error("couldn't check")]
    Offline,
    /// The feed couldn't be confirmed as Voxa's (signature, key, channel, replay, bad fields).
    #[error("refused")]
    Refused,
    /// The download was too long, truncated or had the wrong SHA-256: deleted, retried next day.
    #[error("bad file")]
    BadFile,
    /// Not enough space.
    #[error("no space")]
    NoSpace,
    /// Anything else (file system).
    #[error("failed")]
    Failed,
}

impl From<UpdateError> for UpdateFailure {
    fn from(e: UpdateError) -> Self {
        crate::diag::log(|| format!("update: {e}"));
        match e {
            UpdateError::Offline | UpdateError::Http(_) | UpdateError::RefusedHost | UpdateError::BadEnvelope => Self::Offline,
            e if e.is_refusal() => Self::Refused,
            UpdateError::NotNewer | UpdateError::TooLarge | UpdateError::HashMismatch => Self::BadFile,
            UpdateError::DiskFull => Self::NoSpace,
            _ => Self::Failed,
        }
    }
}

impl From<voxa_update::Offer> for UpdateOffer {
    fn from(o: voxa_update::Offer) -> Self {
        Self {
            version: o.version,
            version_code: o.version_code,
            abi: o.asset_key,
            url: o.url,
            sha256: o.sha256,
            size: o.size,
            notes_url: o.notes_url,
            serial: o.serial,
            signer_sha256: o.signer_sha256,
            auto: o.auto,
        }
    }
}

impl From<&UpdateOffer> for voxa_update::Offer {
    fn from(o: &UpdateOffer) -> Self {
        Self {
            version: o.version.clone(),
            version_code: o.version_code,
            asset_key: o.abi.clone(),
            url: o.url.clone(),
            sha256: o.sha256.clone(),
            size: o.size,
            notes_url: o.notes_url.clone(),
            serial: o.serial,
            signer_sha256: o.signer_sha256.clone(),
            auto: o.auto,
        }
    }
}

const PLATFORM: &str = "Android";

fn check_with(origin: Origin, keys: &[(u8, [u8; 32])], abis: &[String], version_code: u64, highest_serial: u64) -> Result<UpdateCheck, UpdateFailure> {
    let client = Client::new(origin, &version_name(version_code), PLATFORM)?;
    let m = client.check(keys, Channel::Android, highest_serial)?;
    let offer = policy::offer_android(&m, version_code, abis);
    Ok(UpdateCheck {
        offer: offer.map(Into::into),
        serial: policy::next_serial(highest_serial, m.serial),
        paused: m.paused,
    })
}

/// "1.0.0" from versionCode 10000, for the User-Agent only.
fn version_name(code: u64) -> String {
    format!("{}.{}.{}", code / 10000, code / 100 % 100, code % 100)
}

/// Check the Android feed. `abis`: `Build.SUPPORTED_ABIS`, best first; the universal APK is the
/// fallback. `version_code`: `BuildConfig.VERSION_CODE`. `highest_serial`: the highest serial
/// verified before (0 the first time).
///
/// `test_feed` and `test_key` are for debug builds only: Kotlin passes them behind
/// BuildConfig.DEBUG (so R8 drops that path from release) to read a LOCAL test feed signed with a
/// TEST key. A test feed on plain http must be loopback or the emulator's host alias. With both
/// None (always, in release) this is the published feed and the compiled keys.
#[uniffi::export]
pub fn update_check(abis: Vec<String>, version_code: u64, highest_serial: u64, test_feed: Option<String>, test_key: Option<String>) -> Result<UpdateCheck, UpdateFailure> {
    match (test_feed, test_key) {
        (Some(url), Some(key)) => check_with(Origin::test(&url).ok_or(UpdateFailure::Offline)?, &[(1, test_key_bytes(&key)?)], &abis, version_code, highest_serial),
        _ => check_with(Origin::github(Channel::Android), &keys::KEYS, &abis, version_code, highest_serial),
    }
}

/// Download a verified offer into `dir` (filesDir/updates). Returns the staged file's path. Older
/// staged files are deleted once it is in place. `test_feed`: as for [`update_check`].
#[uniffi::export]
pub fn update_download(offer: UpdateOffer, dir: String, test_feed: Option<String>) -> Result<String, UpdateFailure> {
    let origin = match test_feed {
        Some(url) => Origin::test(&url).ok_or(UpdateFailure::Offline)?,
        None => Origin::github(Channel::Android),
    };
    let client = Client::new(origin, &version_name(offer.version_code), PLATFORM)?;
    let dir = std::path::Path::new(&dir);
    let path = client.download(&(&offer).into(), dir)?;
    voxa_update::stage::clean_except(dir, Some(&path));
    Ok(path.to_string_lossy().into_owned())
}

/// Check a staged file again (size and SHA-256) immediately before it goes to PackageInstaller.
#[uniffi::export]
pub fn update_verify_file(path: String, offer: UpdateOffer) -> Result<(), UpdateFailure> {
    Ok(voxa_update::stage::verify_file(std::path::Path::new(&path), &(&offer).into())?)
}

fn test_key_bytes(hex: &str) -> Result<[u8; 32], UpdateFailure> {
    voxa_update::hex_decode(hex.trim()).and_then(|v| v.try_into().ok()).ok_or(UpdateFailure::Refused)
}

/// Verify a feed's bytes with one given key (instrumented tests: signature vectors through FFI).
#[uniffi::export]
pub fn update_verify_feed_bytes(feed: Vec<u8>, key_hex: String, abis: Vec<String>, version_code: u64, highest_serial: u64) -> Result<UpdateCheck, UpdateFailure> {
    let key = test_key_bytes(&key_hex)?;
    let origin = Origin::github(Channel::Android);
    let m = voxa_update::verify_feed(&feed, &[(1, key), (2, key)], Channel::Android, highest_serial, &origin)?;
    let offer = policy::offer_android(&m, version_code, &abis);
    Ok(UpdateCheck {
        offer: offer.map(Into::into),
        serial: policy::next_serial(highest_serial, m.serial),
        paused: m.paused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_come_from_the_version_code() {
        assert_eq!(version_name(10000), "1.0.0");
        assert_eq!(version_name(10203), "1.2.3");
    }

    #[test]
    fn every_update_error_maps_to_what_the_app_says() {
        assert_eq!(UpdateFailure::from(UpdateError::Http(404)), UpdateFailure::Offline);
        assert_eq!(UpdateFailure::from(UpdateError::RefusedHost), UpdateFailure::Offline);
        for e in [UpdateError::BadSignature, UpdateError::UnknownKey, UpdateError::WrongChannel, UpdateError::Replay, UpdateError::BadManifest] {
            assert_eq!(UpdateFailure::from(e), UpdateFailure::Refused);
        }
        assert_eq!(UpdateFailure::from(UpdateError::HashMismatch), UpdateFailure::BadFile);
        assert_eq!(UpdateFailure::from(UpdateError::DiskFull), UpdateFailure::NoSpace);
        assert_eq!(UpdateFailure::from(UpdateError::Io("x".into())), UpdateFailure::Failed);
    }

    #[test]
    fn a_test_feed_on_a_public_plain_host_is_refused() {
        let url = format!("{}://example.com/android.json", "http");
        assert_eq!(update_check(vec![], 10000, 0, Some(url), Some("00".repeat(32))).unwrap_err(), UpdateFailure::Offline);
    }
}
