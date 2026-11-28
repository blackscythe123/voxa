//! Pure decisions, all unit tested: is there an offer, may the update install right now, and
//! when to stop trying.

use serde::{Deserialize, Serialize};

use crate::{ASSET_ANDROID_UNIVERSAL, ASSET_MAC, ASSET_WINDOWS, MAX_APK_BYTES, MAX_ATTEMPTS, MAX_MAC_BYTES, MAX_WINDOWS_BYTES, Manifest, Version};

/// The compiled-in size cap for an asset key.
pub fn cap_for(asset_key: &str) -> u64 {
    match asset_key {
        ASSET_WINDOWS => MAX_WINDOWS_BYTES,
        ASSET_MAC => MAX_MAC_BYTES,
        k if k.starts_with("android-") => MAX_APK_BYTES,
        _ => MAX_APK_BYTES.min(MAX_WINDOWS_BYTES),
    }
}

/// A verified update this client may take. Stored in the desktop `update.json` and passed to
/// Kotlin, so it is plain data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Offer {
    pub version: String,
    /// Android only (0 on desktop).
    #[serde(default)]
    pub version_code: u64,
    pub asset_key: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub notes_url: String,
    pub serial: u64,
    /// Android only: the signing certificate the APK must carry ("" when the feed gives none).
    #[serde(default)]
    pub signer_sha256: String,
    /// False below `min_auto_from`: offered as a manual download only, never installed silently.
    pub auto: bool,
}

/// Desktop: an offer when the feed is not paused, the version is strictly newer than `current`,
/// and the feed has this platform's asset.
pub fn offer(m: &Manifest, current: Version, asset_key: &str) -> Option<Offer> {
    if m.paused {
        return None;
    }
    let v = Version::parse(&m.version)?;
    if v <= current {
        return None;
    }
    let a = m.assets.get(asset_key)?;
    let auto = m.min_auto_from.as_deref().and_then(Version::parse).is_none_or(|min| current >= min);
    Some(Offer {
        version: m.version.clone(),
        version_code: 0,
        asset_key: asset_key.to_string(),
        url: a.url.clone(),
        sha256: a.sha256.to_ascii_lowercase(),
        size: a.size,
        notes_url: m.notes_url.clone(),
        serial: m.serial,
        signer_sha256: String::new(),
        auto,
    })
}

/// Android: compares `versionCode` only (never the name), and picks the first of the device's
/// ABIs (`Build.SUPPORTED_ABIS`, best first) that the feed has, then the universal APK.
pub fn offer_android(m: &Manifest, version_code: u64, abis: &[String]) -> Option<Offer> {
    if m.paused {
        return None;
    }
    let code = m.version_code?;
    if code <= version_code {
        return None;
    }
    let key = abis
        .iter()
        .map(|abi| format!("android-{abi}"))
        .chain(std::iter::once(ASSET_ANDROID_UNIVERSAL.to_string()))
        .find(|k| m.assets.contains_key(k))?;
    let a = m.assets.get(&key)?;
    let current = Version(
        (version_code / 10000) as u32,
        (version_code / 100 % 100) as u32,
        (version_code % 100) as u32,
    );
    let auto = m.min_auto_from.as_deref().and_then(Version::parse).is_none_or(|min| current >= min);
    Some(Offer {
        version: m.version.clone(),
        version_code: code,
        asset_key: key,
        url: a.url.clone(),
        sha256: a.sha256.to_ascii_lowercase(),
        size: a.size,
        notes_url: m.notes_url.clone(),
        serial: m.serial,
        signer_sha256: m.signer_sha256.clone().unwrap_or_default().to_ascii_lowercase(),
        auto,
    })
}

/// The dictation flow, as the gate sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Idle,
    NeedsYou,
    Recording { paused: bool },
    Transcribing,
    Delivering,
    Delivered,
}

/// When the install would happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// A `--background` start at Windows sign-in, before the hotkey is registered.
    BackgroundStart,
    /// The idle install: needs the session locked or at least 15 minutes without input.
    Idle { locked: bool, idle_ms: u64 },
    /// Tray "Quit Voxa": the user asked Voxa to close.
    Quit,
}

pub const IDLE_INSTALL_MS: u64 = 15 * 60 * 1000;
pub const HOTKEY_QUIET_MS: u64 = 5 * 60 * 1000;

/// Everything `can_install_now` looks at. The caller fills it from live state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inputs {
    pub moment: Moment,
    pub flow: Flow,
    /// Any upload waiting or live.
    pub uploads_busy: bool,
    pub signing_in: bool,
    pub mic_testing: bool,
    /// voxa-signin.exe is running.
    pub signin_running: bool,
    /// The window process is running (its mutex exists).
    pub window_open: bool,
    pub strip_visible: bool,
    /// Milliseconds since the hotkey was last used (u64::MAX when never).
    pub since_hotkey_ms: u64,
    pub auto_update: bool,
    pub paused: bool,
    /// The running version is at or above the feed's `min_auto_from` (the offer's `auto`).
    pub auto_allowed: bool,
    /// VOXA_DATA is set: a test copy never installs over the real Voxa.
    pub test_copy: bool,
    /// The running exe is the installed one (`{localappdata}\Programs\Voxa`).
    pub installed_copy: bool,
    pub attempts: u32,
}

/// Why an install waits. Logged every time an install is deferred (no user data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    Dictation,
    Uploads,
    SigningIn,
    MicTest,
    SigninOpen,
    WindowOpen,
    StripVisible,
    HotkeyRecent,
    AutoOff,
    Paused,
    ManualOnly,
    TestCopy,
    NotInstalled,
    AttemptsExhausted,
    NotIdle,
}

impl Blocked {
    pub fn code(self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::Uploads => "uploads",
            Self::SigningIn => "signing_in",
            Self::MicTest => "mic_test",
            Self::SigninOpen => "signin_open",
            Self::WindowOpen => "window_open",
            Self::StripVisible => "strip_visible",
            Self::HotkeyRecent => "hotkey_recent",
            Self::AutoOff => "auto_off",
            Self::Paused => "paused",
            Self::ManualOnly => "manual_only",
            Self::TestCopy => "test_copy",
            Self::NotInstalled => "not_installed",
            Self::AttemptsExhausted => "attempts_exhausted",
            Self::NotIdle => "not_idle",
        }
    }
}

/// The install gate. Every condition must hold; the first one that does not is returned.
pub fn can_install_now(i: &Inputs) -> Result<(), Blocked> {
    if i.test_copy {
        return Err(Blocked::TestCopy);
    }
    if !i.installed_copy {
        return Err(Blocked::NotInstalled);
    }
    if i.flow != Flow::Idle {
        return Err(Blocked::Dictation);
    }
    if i.uploads_busy {
        return Err(Blocked::Uploads);
    }
    if i.signing_in {
        return Err(Blocked::SigningIn);
    }
    if i.mic_testing {
        return Err(Blocked::MicTest);
    }
    if i.signin_running {
        return Err(Blocked::SigninOpen);
    }
    if i.window_open {
        return Err(Blocked::WindowOpen);
    }
    if i.strip_visible {
        return Err(Blocked::StripVisible);
    }
    if i.since_hotkey_ms < HOTKEY_QUIET_MS && i.moment != Moment::BackgroundStart {
        return Err(Blocked::HotkeyRecent);
    }
    if !i.auto_update {
        return Err(Blocked::AutoOff);
    }
    if i.paused {
        return Err(Blocked::Paused);
    }
    if !i.auto_allowed {
        return Err(Blocked::ManualOnly);
    }
    if attempts_exhausted(i.attempts) {
        return Err(Blocked::AttemptsExhausted);
    }
    if let Moment::Idle { locked, idle_ms } = i.moment {
        if !locked && idle_ms < IDLE_INSTALL_MS {
            return Err(Blocked::NotIdle);
        }
    }
    Ok(())
}

/// The "Restart and update" button (auto off, the user asked): the same safety checks, without
/// the auto-update, idle and attempt conditions.
pub fn can_install_on_request(i: &Inputs) -> Result<(), Blocked> {
    let asked = Inputs { auto_update: true, auto_allowed: true, attempts: 0, since_hotkey_ms: u64::MAX, moment: Moment::Quit, window_open: false, ..*i };
    can_install_now(&asked)
}

pub fn attempts_exhausted(attempts: u32) -> bool {
    attempts >= MAX_ATTEMPTS
}

/// Install only after 3 consecutive passing evaluations at least 1 s apart (a hotkey press
/// between two ticks resets it).
#[derive(Debug, Clone, Default)]
pub struct Debounce {
    passes: u32,
    last_ms: Option<u64>,
}

impl Debounce {
    pub const NEEDED: u32 = 3;
    pub const GAP_MS: u64 = 1000;

    /// Feed one evaluation; true when the install may go ahead now.
    pub fn feed(&mut self, now_ms: u64, passed: bool) -> bool {
        if !passed {
            *self = Self::default();
            return false;
        }
        match self.last_ms {
            Some(last) if now_ms.saturating_sub(last) < Self::GAP_MS => return false,
            _ => {}
        }
        self.passes += 1;
        self.last_ms = Some(now_ms);
        self.passes >= Self::NEEDED
    }
}

/// Failed install attempts for one target version. A newer target starts again at zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempts {
    pub target: String,
    pub count: u32,
}

impl Attempts {
    /// Called when an offer is staged: a newer target resets the count.
    pub fn target(&mut self, version: &str) {
        let newer = match (Version::parse(version), Version::parse(&self.target)) {
            (Some(v), Some(t)) => v > t,
            (Some(_), None) => true,
            _ => false,
        };
        if newer {
            *self = Self { target: version.to_string(), count: 0 };
        }
    }

    pub fn failed(&mut self) {
        self.count = self.count.saturating_add(1);
    }

    pub fn exhausted(&self) -> bool {
        attempts_exhausted(self.count)
    }
}

/// Serial anti-replay: the highest verified serial only ever goes up.
pub fn next_serial(highest: u64, verified: u64) -> u64 {
    highest.max(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::tests::desktop_payload;

    fn manifest(v: &str) -> Manifest {
        serde_json::from_value(desktop_payload(v, 1)).unwrap()
    }

    fn ok_inputs() -> Inputs {
        Inputs {
            moment: Moment::Idle { locked: true, idle_ms: 0 },
            flow: Flow::Idle,
            uploads_busy: false,
            signing_in: false,
            mic_testing: false,
            signin_running: false,
            window_open: false,
            strip_visible: false,
            since_hotkey_ms: u64::MAX,
            auto_update: true,
            paused: false,
            auto_allowed: true,
            test_copy: false,
            installed_copy: true,
            attempts: 0,
        }
    }

    #[test]
    fn the_same_or_an_older_version_is_never_offered() {
        let cur = Version(1, 0, 1);
        assert!(offer(&manifest("1.0.1"), cur, ASSET_WINDOWS).is_none());
        assert!(offer(&manifest("1.0.0"), cur, ASSET_WINDOWS).is_none());
        assert!(offer(&manifest("0.9.9"), cur, ASSET_WINDOWS).is_none());
        let o = offer(&manifest("1.0.2"), cur, ASSET_WINDOWS).unwrap();
        assert_eq!(o.version, "1.0.2");
        assert!(o.url.ends_with("Voxa-Setup-1.0.2.exe"));
        assert!(offer(&manifest("1.0.2"), cur, "linux-x64").is_none(), "no asset for this platform");
    }

    #[test]
    fn a_paused_feed_offers_nothing() {
        let mut m = manifest("2.0.0");
        m.paused = true;
        assert!(offer(&m, Version(1, 0, 0), ASSET_WINDOWS).is_none());
        let mut a = android_manifest(20000);
        a.paused = true;
        assert!(offer_android(&a, 10000, &["arm64-v8a".into()]).is_none());
        let i = Inputs { paused: true, ..ok_inputs() };
        assert_eq!(can_install_now(&i), Err(Blocked::Paused));
    }

    #[test]
    fn below_min_auto_from_is_offered_as_manual_only() {
        let mut m = manifest("2.0.0");
        m.min_auto_from = Some("1.5.0".into());
        assert!(!offer(&m, Version(1, 4, 9), ASSET_WINDOWS).unwrap().auto);
        assert!(offer(&m, Version(1, 5, 0), ASSET_WINDOWS).unwrap().auto);
        m.min_auto_from = None;
        assert!(offer(&m, Version(1, 0, 0), ASSET_WINDOWS).unwrap().auto);
        let i = Inputs { auto_allowed: false, ..ok_inputs() };
        assert_eq!(can_install_now(&i), Err(Blocked::ManualOnly));
    }

    fn android_manifest(code: u64) -> Manifest {
        let h = crate::feed::tests::H;
        serde_json::from_value(serde_json::json!({
            "schema": 1, "channel": "android", "serial": 1, "version": "1.0.1", "version_code": code,
            "notes_url": "https://github.com/blackscythe123/voxa/releases/tag/android-v1.0.1", "signer_sha256": h,
            "assets": {
                "android-arm64-v8a": { "url": "https://github.com/blackscythe123/voxa/releases/download/android-v1.0.1/Voxa-Android-1.0.1-arm64-v8a.apk", "sha256": h, "size": 6000000 },
                "android-universal": { "url": "https://github.com/blackscythe123/voxa/releases/download/android-v1.0.1/Voxa-Android-1.0.1-universal.apk", "sha256": h, "size": 9000000 }
            }
        }))
        .unwrap()
    }

    #[test]
    fn android_offers_compare_version_code_not_name() {
        let mut m = android_manifest(10001);
        // The name says older, the code says newer: the code wins.
        m.version = "0.0.1".into();
        assert_eq!(offer_android(&m, 10000, &["arm64-v8a".into()]).unwrap().version_code, 10001);
        // The name says newer, the code says the same: no offer.
        let mut m = android_manifest(10000);
        m.version = "9.9.9".into();
        assert!(offer_android(&m, 10000, &["arm64-v8a".into()]).is_none());
        assert!(offer_android(&android_manifest(9999), 10000, &["arm64-v8a".into()]).is_none());
        // No version_code at all: no offer.
        let mut m = android_manifest(10001);
        m.version_code = None;
        assert!(offer_android(&m, 10000, &["arm64-v8a".into()]).is_none());
    }

    #[test]
    fn android_picks_the_device_abi_then_the_universal_apk() {
        let m = android_manifest(10001);
        let o = offer_android(&m, 10000, &["arm64-v8a".into(), "armeabi-v7a".into()]).unwrap();
        assert_eq!(o.asset_key, "android-arm64-v8a");
        assert_eq!(o.signer_sha256, crate::feed::tests::H);
        let o = offer_android(&m, 10000, &["x86_64".into(), "arm64-v8a".into()]).unwrap();
        assert_eq!(o.asset_key, "android-arm64-v8a", "falls back through SUPPORTED_ABIS in order");
        let o = offer_android(&m, 10000, &["x86_64".into()]).unwrap();
        assert_eq!(o.asset_key, "android-universal");
        assert!(o.url.ends_with("universal.apk"));
    }

    #[test]
    fn can_install_now_blocks_while_recording_paused_writing_or_uploading() {
        assert_eq!(can_install_now(&ok_inputs()), Ok(()));
        for flow in [Flow::NeedsYou, Flow::Recording { paused: true }, Flow::Recording { paused: false }, Flow::Transcribing, Flow::Delivering, Flow::Delivered] {
            assert_eq!(can_install_now(&Inputs { flow, ..ok_inputs() }), Err(Blocked::Dictation), "{flow:?}");
            assert!(can_install_on_request(&Inputs { flow, ..ok_inputs() }).is_err());
        }
        assert_eq!(can_install_now(&Inputs { uploads_busy: true, ..ok_inputs() }), Err(Blocked::Uploads));
        assert_eq!(can_install_now(&Inputs { mic_testing: true, ..ok_inputs() }), Err(Blocked::MicTest));
        assert_eq!(can_install_now(&Inputs { strip_visible: true, ..ok_inputs() }), Err(Blocked::StripVisible));
        assert_eq!(can_install_now(&Inputs { since_hotkey_ms: 60_000, ..ok_inputs() }), Err(Blocked::HotkeyRecent));
        assert_eq!(can_install_now(&Inputs { since_hotkey_ms: HOTKEY_QUIET_MS, ..ok_inputs() }), Ok(()));
        assert_eq!(can_install_now(&Inputs { auto_update: false, ..ok_inputs() }), Err(Blocked::AutoOff));
        assert_eq!(can_install_on_request(&Inputs { auto_update: false, ..ok_inputs() }), Ok(()), "the user pressed Restart and update");
        assert_eq!(can_install_on_request(&Inputs { uploads_busy: true, auto_update: false, ..ok_inputs() }), Err(Blocked::Uploads));
    }

    #[test]
    fn can_install_now_blocks_while_the_window_or_signin_is_open() {
        assert_eq!(can_install_now(&Inputs { window_open: true, ..ok_inputs() }), Err(Blocked::WindowOpen));
        assert_eq!(can_install_now(&Inputs { signing_in: true, ..ok_inputs() }), Err(Blocked::SigningIn));
        assert_eq!(can_install_now(&Inputs { signin_running: true, ..ok_inputs() }), Err(Blocked::SigninOpen));
    }

    #[test]
    fn can_install_now_blocks_a_test_copy_or_a_dev_build() {
        assert_eq!(can_install_now(&Inputs { test_copy: true, ..ok_inputs() }), Err(Blocked::TestCopy));
        assert_eq!(can_install_now(&Inputs { installed_copy: false, ..ok_inputs() }), Err(Blocked::NotInstalled));
        assert_eq!(can_install_on_request(&Inputs { test_copy: true, ..ok_inputs() }), Err(Blocked::TestCopy));
        assert_eq!(can_install_on_request(&Inputs { installed_copy: false, ..ok_inputs() }), Err(Blocked::NotInstalled));
    }

    #[test]
    fn can_install_now_needs_lock_or_15_minutes_idle() {
        let at = |locked, idle_ms| can_install_now(&Inputs { moment: Moment::Idle { locked, idle_ms }, ..ok_inputs() });
        assert_eq!(at(false, 0), Err(Blocked::NotIdle));
        assert_eq!(at(false, IDLE_INSTALL_MS - 1), Err(Blocked::NotIdle));
        assert_eq!(at(false, IDLE_INSTALL_MS), Ok(()));
        assert_eq!(at(true, 0), Ok(()));
        assert_eq!(can_install_now(&Inputs { moment: Moment::BackgroundStart, since_hotkey_ms: 0, ..ok_inputs() }), Ok(()));
    }

    #[test]
    fn two_failed_attempts_stop_auto_install_until_a_newer_version() {
        let mut a = Attempts::default();
        a.target("1.0.1");
        a.failed();
        assert!(!a.exhausted());
        a.failed();
        assert!(a.exhausted());
        assert_eq!(can_install_now(&Inputs { attempts: a.count, ..ok_inputs() }), Err(Blocked::AttemptsExhausted));
        a.target("1.0.1");
        assert!(a.exhausted(), "the same target keeps its count");
        a.target("1.0.0");
        assert!(a.exhausted(), "an older target never resets it");
        a.target("1.0.2");
        assert!(!a.exhausted());
        assert_eq!(a.count, 0);
    }

    #[test]
    fn three_passing_checks_a_second_apart_are_needed() {
        let mut d = Debounce::default();
        assert!(!d.feed(0, true));
        assert!(!d.feed(500, true), "too soon, not counted");
        assert!(!d.feed(1000, true));
        assert!(d.feed(2000, true));
        let mut d = Debounce::default();
        d.feed(0, true);
        d.feed(1000, true);
        assert!(!d.feed(2000, false), "a block resets it");
        assert!(!d.feed(3000, true));
    }

    #[test]
    fn the_highest_serial_never_goes_down() {
        assert_eq!(next_serial(10, 5), 10);
        assert_eq!(next_serial(10, 12), 12);
    }
}
