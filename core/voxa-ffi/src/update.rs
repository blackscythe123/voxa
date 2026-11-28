//! Self update for Kotlin (notify/Updater.kt): the same verifier, version rules and capped,
//! verified download the desktop uses (`voxa-update`). Kotlin never verifies a signature itself
//! (java.security Ed25519 needs API 33; minSdk is 29).
//!
//! Trap: `core_version()` is the workspace version, not the APK's. The caller always passes
//! `BuildConfig.VERSION_CODE`; nothing here uses this crate's own version for the comparison.

use voxa_update::{Channel, CheckState, Origin, UpdateError, fetch::Client, keys, policy};

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

/// One check. Always returned, also when the check failed, because the schedule state changes
/// either way: store `state` as it is (SharedPreferences "update", key "check_state").
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UpdateCheck {
    pub offer: Option<UpdateOffer>,
    /// The highest feed serial verified so far.
    pub serial: u64,
    pub paused: bool,
    /// False when nothing went to the network: an automatic check that isn't due, or a second
    /// manual check within a minute (show the last result again).
    pub checked: bool,
    /// "" when the check worked, else "offline", "refused", "bad_file", "no_space" or "failed".
    pub failure: String,
    /// The updated schedule, ETag and cached feed (JSON). Opaque to Kotlin.
    pub state: String,
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

fn failure_code(f: UpdateFailure) -> &'static str {
    match f {
        UpdateFailure::Offline => "offline",
        UpdateFailure::Refused => "refused",
        UpdateFailure::BadFile => "bad_file",
        UpdateFailure::NoSpace => "no_space",
        UpdateFailure::Failed => "failed",
    }
}

/// The owner's cheap check rules, all in `voxa_update::CheckState`: at most one automatic check a
/// day at this install's own time of day (with 1 h, 4 h, then next day backoff), a manual check
/// at most once a minute, and a conditional request (a 304 reuses the stored feed, verified again).
fn check_with(origin: Origin, keys: &[(u8, [u8; 32])], abis: &[String], version_code: u64, state_json: &str, now_ms: u64, manual: bool) -> UpdateCheck {
    let mut state: CheckState = serde_json::from_str(state_json).unwrap_or_default();
    state.init_random(now_ms);
    let wait = if manual { !state.manual_allowed(now_ms) } else { !state.due(now_ms) };
    let done = |state: &CheckState, offer: Option<UpdateOffer>, paused: bool, checked: bool, failure: &str| UpdateCheck {
        offer,
        serial: state.highest_serial,
        paused,
        checked,
        failure: failure.to_string(),
        state: serde_json::to_string(state).unwrap_or_default(),
    };
    if wait {
        return done(&state, None, false, false, "");
    }
    let client = match Client::new(origin, &version_name(version_code), PLATFORM) {
        Ok(c) => c,
        Err(e) => return done(&state, None, false, false, failure_code(e.into())),
    };
    match client.check_with_state(keys, Channel::Android, &mut state, now_ms, manual) {
        Ok(m) => {
            let offer = policy::offer_android(&m, version_code, abis).map(Into::into);
            done(&state, offer, m.paused, true, "")
        }
        Err(e) => done(&state, None, false, true, failure_code(e.into())),
    }
}

/// "1.0.0" from versionCode 10000, for the User-Agent only.
fn version_name(code: u64) -> String {
    format!("{}.{}.{}", code / 10000, code / 100 % 100, code % 100)
}

/// Check the Android feed, if the cheap check rules allow it now. `abis`: `Build.SUPPORTED_ABIS`,
/// best first; the universal APK is the fallback. `version_code`: `BuildConfig.VERSION_CODE`.
/// `state_json`: what the last call returned in `state` ("" the first time). `manual`: the user
/// pressed "Check for updates".
///
/// `test_feed` and `test_key` are for debug builds only: Kotlin passes them behind
/// BuildConfig.DEBUG (so R8 drops that path from release) to read a LOCAL test feed signed with a
/// TEST key. A test feed on plain http must be loopback or the emulator's host alias. With both
/// None (always, in release) this is the published feed and the compiled keys.
#[uniffi::export]
pub fn update_check(abis: Vec<String>, version_code: u64, state_json: String, now_ms: u64, manual: bool, test_feed: Option<String>, test_key: Option<String>) -> UpdateCheck {
    let test = test_feed.and_then(|url| Some((Origin::test(&url)?, test_key_bytes(test_key.as_deref()?).ok()?)));
    match test {
        Some((origin, key)) => check_with(origin, &[(1, key)], &abis, version_code, &state_json, now_ms, manual),
        None => check_with(Origin::github(Channel::Android), &keys::KEYS, &abis, version_code, &state_json, now_ms, manual),
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

/// False while the compiled update keys are the all-zero placeholders: this build can never verify
/// a feed, so the app says "Updates are off in this build" and never checks (a release build
/// refuses to compile that way unless VOXA_ALLOW_PLACEHOLDER_KEYS=1).
#[uniffi::export]
pub fn update_keys_ready() -> bool {
    !keys::placeholders()
}

/// The last look at the feed right before PackageInstaller (see [`update_confirm`]).
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UpdateConfirm {
    /// The feed is paused (the kill switch), offers another file, or isn't Voxa's: drop the
    /// staged file and don't install.
    pub withdrawn: bool,
    /// The check state with the new validators and serial (store it like `UpdateCheck.state`).
    pub state: String,
}

/// Right before the install is committed: one quick conditional request (3 s to connect, 6 s in
/// all) to confirm the feed still offers this exact file. No network is not a withdrawal (the
/// staged file was verified against a signed feed). `test_feed`/`test_key`: as for [`update_check`].
#[uniffi::export]
pub fn update_confirm(offer: UpdateOffer, state_json: String, test_feed: Option<String>, test_key: Option<String>) -> UpdateConfirm {
    let mut state: CheckState = serde_json::from_str(&state_json).unwrap_or_default();
    let test = test_feed.and_then(|url| Some((Origin::test(&url)?, test_key_bytes(test_key.as_deref()?).ok()?)));
    let (origin, keyset): (Origin, Vec<(u8, [u8; 32])>) = match test {
        Some((o, k)) => (o, vec![(1, k)]),
        None => (Origin::github(Channel::Android), keys::KEYS.to_vec()),
    };
    let withdrawn = match Client::quick(origin, &version_name(offer.version_code), PLATFORM) {
        Ok(c) => c.confirm_offer(&keyset, Channel::Android, &mut state, &(&offer).into()) == voxa_update::fetch::Confirm::Withdrawn,
        Err(_) => false,
    };
    UpdateConfirm { withdrawn, state: serde_json::to_string(&state).unwrap_or_default() }
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
        checked: true,
        failure: String::new(),
        state: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_with_placeholder_keys_reports_updates_off() {
        assert_eq!(update_keys_ready(), !keys::placeholders());
    }

    #[test]
    fn no_network_at_the_last_look_never_withdraws_a_verified_file() {
        let offer = UpdateOffer {
            version: "1.0.1".into(),
            version_code: 10001,
            abi: "android-arm64-v8a".into(),
            url: format!("{}://127.0.0.1:1/f/Voxa-Android-1.0.1-arm64-v8a.apk", "http"),
            sha256: "a".repeat(64),
            size: 10,
            notes_url: String::new(),
            serial: 1,
            signer_sha256: String::new(),
            auto: true,
        };
        let r = update_confirm(offer, String::new(), Some(format!("{}://127.0.0.1:1/f/android.json", "http")), Some("01".repeat(32)));
        assert!(!r.withdrawn);
        assert!(serde_json::from_str::<CheckState>(&r.state).is_ok());
    }

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
    fn a_second_check_right_away_does_no_network() {
        let url = format!("{}://example.com/android.json", "http");
        // A public plain-http test feed is not a test origin: the published feed is used instead,
        // and it is never fetched again within the minute.
        assert!(Origin::test(&url).is_none());
        let first = check_with(Origin::test(&format!("{}://127.0.0.1:1/f/android.json", "http")).unwrap(), &[(1, [1; 32])], &[], 10000, "", 1_000_000, true);
        assert!(first.checked);
        assert_eq!(first.failure, "offline");
        let again = check_with(Origin::test(&format!("{}://127.0.0.1:1/f/android.json", "http")).unwrap(), &[(1, [1; 32])], &[], 10000, &first.state, 1_000_000 + 5_000, true);
        assert!(!again.checked, "a second manual check within a minute does no network");
    }
}

#[cfg(test)]
mod schedule_tests {
    use super::*;

    #[test]
    fn an_automatic_check_that_is_not_due_does_no_network() {
        let origin = || Origin::test(&format!("{}://127.0.0.1:1/f/android.json", "http")).unwrap();
        // The first call picks this install's slot and waits for it (no network yet unless due).
        let first = check_with(origin(), &[(1, [1; 32])], &[], 10000, "", 1_000_000, false);
        let st: CheckState = serde_json::from_str(&first.state).unwrap();
        assert!(st.slot_ms.is_some());
        if !first.checked {
            assert_eq!(first.failure, "");
        }
        // Right after any automatic attempt, the next one is not due.
        let again = check_with(origin(), &[(1, [1; 32])], &[], 10000, &first.state, 1_000_000 + 60_000, false);
        if first.checked {
            assert!(!again.checked);
        }
        // Bad stored JSON starts afresh, never crashes.
        let fresh = check_with(origin(), &[(1, [1; 32])], &[], 10000, "{not json", 1_000_000, false);
        assert!(serde_json::from_str::<CheckState>(&fresh.state).is_ok());
    }
}
