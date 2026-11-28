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
    /// The feed's newest version ("" when it offers nothing to this device).
    pub latest: String,
    pub paused: bool,
}

/// Mirrors `voxa_update::UpdateError`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum UpdateFailure {
    #[error("offline")]
    Offline,
    #[error("http {status}")]
    Http { status: u16 },
    #[error("refused host")]
    RefusedHost,
    #[error("bad envelope")]
    BadEnvelope,
    #[error("bad signature")]
    BadSignature,
    #[error("unknown key")]
    UnknownKey,
    #[error("wrong channel")]
    WrongChannel,
    #[error("replay")]
    Replay,
    #[error("bad manifest")]
    BadManifest,
    #[error("not newer")]
    NotNewer,
    #[error("too large")]
    TooLarge,
    #[error("hash mismatch")]
    HashMismatch,
    #[error("disk full")]
    DiskFull,
    #[error("io")]
    Io,
}

impl UpdateFailure {
    /// The feed couldn't be confirmed as Voxa's (the "refused" copy, not "couldn't check").
    pub fn refused(&self) -> bool {
        matches!(self, Self::BadSignature | Self::UnknownKey | Self::WrongChannel | Self::Replay | Self::BadManifest)
    }
}

impl From<UpdateError> for UpdateFailure {
    fn from(e: UpdateError) -> Self {
        match e {
            UpdateError::Offline => Self::Offline,
            UpdateError::Http(status) => Self::Http { status },
            UpdateError::RefusedHost => Self::RefusedHost,
            UpdateError::BadEnvelope => Self::BadEnvelope,
            UpdateError::BadSignature => Self::BadSignature,
            UpdateError::UnknownKey => Self::UnknownKey,
            UpdateError::WrongChannel => Self::WrongChannel,
            UpdateError::Replay => Self::Replay,
            UpdateError::BadManifest => Self::BadManifest,
            UpdateError::NotNewer => Self::NotNewer,
            UpdateError::TooLarge => Self::TooLarge,
            UpdateError::HashMismatch => Self::HashMismatch,
            UpdateError::DiskFull => Self::DiskFull,
            UpdateError::Io(_) => Self::Io,
        }
    }
}

/// Whether a failure is a refusal (Settings shows the "couldn't confirm it came from Voxa" copy
/// after a manual check).
#[uniffi::export]
pub fn update_failure_refused(failure: UpdateFailure) -> bool {
    failure.refused()
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
        latest: offer.as_ref().map(|o| o.version.clone()).unwrap_or_default(),
        offer: offer.map(Into::into),
        serial: policy::next_serial(highest_serial, m.serial),
        paused: m.paused,
    })
}

/// "1.0.0" from versionCode 10000, for the User-Agent only.
fn version_name(code: u64) -> String {
    format!("{}.{}.{}", code / 10000, code / 100 % 100, code % 100)
}

/// Check the published Android feed. `abis`: `Build.SUPPORTED_ABIS`, best first; the universal
/// APK is the fallback. `version_code`: `BuildConfig.VERSION_CODE`. `highest_serial`: the
/// highest serial verified before (0 the first time).
#[uniffi::export]
pub fn update_check(abis: Vec<String>, version_code: u64, highest_serial: u64) -> Result<UpdateCheck, UpdateFailure> {
    check_with(Origin::github(Channel::Android), &keys::KEYS, &abis, version_code, highest_serial)
}

/// Download a verified offer into `dir` (filesDir/updates) on the published origin. Returns the
/// staged file's path. Older staged files are deleted once it is in place.
#[uniffi::export]
pub fn update_download(offer: UpdateOffer, dir: String) -> Result<String, UpdateFailure> {
    download_with(Origin::github(Channel::Android), &offer, &dir)
}

fn download_with(origin: Origin, offer: &UpdateOffer, dir: &str) -> Result<String, UpdateFailure> {
    let client = Client::new(origin, &version_name(offer.version_code), PLATFORM)?;
    let dir = std::path::Path::new(dir);
    let path = client.download(&offer.into(), dir)?;
    voxa_update::stage::clean_except(dir, Some(&path));
    Ok(path.to_string_lossy().into_owned())
}

/// Check a staged file again (size and SHA-256) immediately before it goes to PackageInstaller.
#[uniffi::export]
pub fn update_verify_file(path: String, offer: UpdateOffer) -> Result<(), UpdateFailure> {
    Ok(voxa_update::stage::verify_file(std::path::Path::new(&path), &(&offer).into())?)
}

/// Debug builds only (Kotlin calls this behind BuildConfig.DEBUG, so R8 drops it from release):
/// a LOCAL test feed signed with a TEST key. `feed_url` must be loopback or the emulator's host
/// alias when it is plain http.
#[uniffi::export]
pub fn update_check_test(feed_url: String, key_hex: String, abis: Vec<String>, version_code: u64, highest_serial: u64) -> Result<UpdateCheck, UpdateFailure> {
    let origin = Origin::test(&feed_url).ok_or(UpdateFailure::RefusedHost)?;
    check_with(origin, &[(1, test_key(&key_hex)?)], &abis, version_code, highest_serial)
}

/// Debug builds only: [`update_download`] from the LOCAL test feed's origin.
#[uniffi::export]
pub fn update_download_test(feed_url: String, offer: UpdateOffer, dir: String) -> Result<String, UpdateFailure> {
    let origin = Origin::test(&feed_url).ok_or(UpdateFailure::RefusedHost)?;
    download_with(origin, &offer, &dir)
}

fn test_key(hex: &str) -> Result<[u8; 32], UpdateFailure> {
    voxa_update::hex_decode(hex.trim()).and_then(|v| v.try_into().ok()).ok_or(UpdateFailure::UnknownKey)
}

/// True while the compiled update keys are placeholders (no update can ever verify).
#[uniffi::export]
pub fn update_keys_are_placeholders() -> bool {
    keys::placeholders()
}

/// Verify a feed's bytes with one given key (instrumented tests: signature vectors through FFI).
#[uniffi::export]
pub fn update_verify_feed_bytes(feed: Vec<u8>, key_hex: String, abis: Vec<String>, version_code: u64, highest_serial: u64) -> Result<UpdateCheck, UpdateFailure> {
    let key = test_key(&key_hex)?;
    let origin = Origin::github(Channel::Android);
    let m = voxa_update::verify_feed(&feed, &[(1, key), (2, key)], Channel::Android, highest_serial, &origin)?;
    let offer = policy::offer_android(&m, version_code, &abis);
    Ok(UpdateCheck {
        latest: offer.as_ref().map(|o| o.version.clone()).unwrap_or_default(),
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
    fn every_update_error_maps_to_a_failure() {
        assert_eq!(UpdateFailure::from(UpdateError::Http(404)), UpdateFailure::Http { status: 404 });
        assert!(UpdateFailure::from(UpdateError::BadSignature).refused());
        assert!(!UpdateFailure::from(UpdateError::Offline).refused());
        assert_eq!(UpdateFailure::from(UpdateError::Io("x".into())), UpdateFailure::Io);
    }

    #[test]
    fn a_test_feed_on_a_public_plain_host_is_refused() {
        let url = format!("{}://example.com/android.json", "http");
        assert_eq!(update_check_test(url, "00".repeat(32), vec![], 10000, 0).unwrap_err(), UpdateFailure::RefusedHost);
    }
}
