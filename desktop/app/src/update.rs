//! Automatic updates on the desktop. Only the tray process runs this; the window process shows
//! the state (link::UpdateView) and sends commands.
//!
//! Windows: check the signed feed 2 minutes after start and then once a day, download the
//! versioned installer into %LOCALAPPDATA%\Voxa\updates (verified while streaming), and install it
//! silently at a moment nobody is using Voxa: at a sign-in start before the shortcut exists, when
//! the PC is locked or idle for 15 minutes, or at tray Quit. Never during a take, never while the
//! window or sign-in is open, never from a test copy or a dev build, and never by stealing focus.
//! macOS: the same check, then a Home bar and a Download button; the user replaces the app.
//!
//! Everything about the decision is pure and tested in voxa_update::policy; this file wires it to
//! Voxa's live state. update.log records checks, results and gate reasons, never personal data.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::Context;
use serde::{Deserialize, Serialize};
use voxa_update::policy::{self, Blocked, Debounce, Inputs, Moment};
use voxa_update::{Channel, CheckState, Offer, Origin, UpdateError, Version, fetch::Client, keys, stage};

use crate::link::UpdateView;
use crate::{Voxa, os};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const FIRST_CHECK: Duration = Duration::from_secs(120);
const LOG_CAP: u64 = 256 * 1024;
/// Windows installs by itself; macOS only tells the user.
pub const INSTALLS: bool = cfg!(windows);
const ASSET: &str = if cfg!(windows) { voxa_update::ASSET_WINDOWS } else { voxa_update::ASSET_MAC };
const PLATFORM: &str = if cfg!(windows) { "Windows" } else { "macOS" };
const RELEASES_TAG: &str = "https://github.com/blackscythe123/voxa/releases/tag/v";

pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version(0, 0, 0))
}

/// Where updates are staged: %LOCALAPPDATA%\Voxa\updates (not the roaming profile). A test copy
/// (VOXA_DATA) keeps them in its own folder.
pub fn updates_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return PathBuf::from(d).join("updates");
    }
    if cfg!(windows) {
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("Voxa").join("updates")
    } else {
        crate::data_dir().join("updates")
    }
}

/// The exe is the installed copy in {localappdata}\Programs\Voxa (never a build in target\).
fn installed_copy() -> bool {
    if std::env::var_os("VOXA_DATA").is_some() {
        return false;
    }
    let (Ok(exe), Ok(local)) = (std::env::current_exe(), std::env::var("LOCALAPPDATA")) else { return false };
    is_installed_path(&exe, Path::new(&local))
}

fn is_installed_path(exe: &Path, local: &Path) -> bool {
    let want = local.join("Programs").join("Voxa").to_string_lossy().to_lowercase();
    let have = exe.parent().map(|p| p.to_string_lossy().to_lowercase()).unwrap_or_default();
    have == want
}

/// The feed and keys: the published GitHub feed, or in a test copy only (test_hooks()) a LOCAL
/// test feed signed with a TEST key (VOXA_TEST_UPDATE_FEED, VOXA_TEST_UPDATE_KEY as 64 hex).
fn origin_and_keys() -> (Origin, Vec<(u8, [u8; 32])>) {
    if crate::test_hooks() {
        let feed = std::env::var("VOXA_TEST_UPDATE_FEED").ok();
        let key = std::env::var("VOXA_TEST_UPDATE_KEY").ok().and_then(|h| voxa_update::hex_decode(h.trim())).and_then(|v| <[u8; 32]>::try_from(v).ok());
        if let (Some(o), Some(k)) = (feed.as_deref().and_then(Origin::test), key) {
            return (o, vec![(1, k), (2, k)]);
        }
    }
    (Origin::github(Channel::Desktop), keys::KEYS.to_vec())
}

// ── state (update.json) ──────────────────────────────────────────────────

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    pub last_check_ms: u64,
    /// up_to_date, ready, available (macOS), manual_only, offline, failed, refused, disk_full.
    pub last_result: String,
    /// The check schedule, cache validators and the highest verified serial (voxa_update::schedule).
    pub check: CheckState,
    pub offer: Option<Offer>,
    pub paused: bool,
    pub staged_path: Option<PathBuf>,
    /// The version an install was last started for, and how many times.
    pub target: String,
    pub attempts: u32,
    pub last_error: String,
    pub from_version: String,
    pub last_idle_install_ms: u64,
    /// Windows: the Home bar "Voxa was updated to X", until closed.
    pub updated_to: Option<String>,
    /// macOS: the "is out" bar closed for this version.
    pub dismissed: String,
    /// macOS: the designated requirement recorded at the last start, and whether the
    /// Accessibility bar is owed until the grant is back.
    pub mac_requirement: String,
    pub mac_access_pending: bool,
    /// Size of the download when the disk was full (for "about N MB").
    pub needed_mb: u64,
}

impl UpdateState {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("update.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            if let Err(e) = crate::store::write_atomic(&dir.join("update.json"), &b) {
                os::log(&format!("FAIL saving update state: {}", e.kind()));
            }
        }
    }
}

/// Move `from` over `to`, retrying while Windows reports the target busy.
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    voxa_core::fsx::replace_file(from, to)
}

/// update.log: append only, never deleted at start; at 256 KB it moves to update.old.log.
pub fn ulog(dir: &Path, msg: &str) {
    use std::io::Write;
    os::log(&format!("update: {msg}"));
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("update.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_CAP) {
        let _ = replace_file(&path, &dir.join("update.old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let t = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{t} {msg}");
    }
}

// ── the installer handoff ────────────────────────────────────────────────

/// The installer's command line, exactly: silent, no restart prompt, no cancel, never the UAC
/// "remove the old Voxa" task, logged next to the staged file. `relaunch: false` (tray Quit)
/// tells Setup not to start Voxa again.
pub fn installer_args(from: &str, version: &str, log: &Path, relaunch: bool) -> Vec<String> {
    let mut a: Vec<String> = ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/NOCANCEL", "/SP-"].iter().map(|s| s.to_string()).collect();
    a.push(format!("/VOXAUPDATE={from}"));
    a.push("/MERGETASKS=\"!removeold\"".into());
    a.push(format!("/LOG=\"{}\"", log.display()));
    let _ = version;
    if !relaunch {
        a.push("/VOXANORELAUNCH=1".into());
    }
    a
}

/// The one place the installer is started. The gate must hold and the staged file must match
/// the verified offer again right now (the folder is user-writable); then attempts go up before
/// Setup starts, so a crash mid-install still counts.
pub fn launch_installer(st: &mut UpdateState, dir: &Path, inputs: &Inputs, on_request: bool, relaunch: bool) -> Result<(), String> {
    let gate = if on_request { policy::can_install_on_request(inputs) } else { policy::can_install_now(inputs) };
    gate.map_err(|b| format!("waiting: {}", b.code()))?;
    let offer = st.offer.clone().ok_or("no offer")?;
    let path = st.staged_path.clone().ok_or("nothing staged")?;
    if let Err(e) = stage::verify_file(&path, &offer) {
        let _ = std::fs::remove_file(&path);
        st.staged_path = None;
        st.save(dir);
        return Err(format!("the staged installer failed its check ({}), downloading again later", e.code()));
    }
    let from = env!("CARGO_PKG_VERSION").to_string();
    let args = installer_args(&from, &offer.version, &dir.join(format!("install-{}.log", offer.version)), relaunch);
    if st.target != offer.version {
        st.target = offer.version.clone();
        st.attempts = 0;
    }
    st.attempts += 1;
    st.from_version = from.clone();
    st.save(dir);
    ulog(dir, &format!("installing {} over {from} (attempt {}, relaunch {relaunch})", offer.version, st.attempts));
    os::spawn_detached(&path, &args).map_err(|e| format!("couldn't start the installer: {}", e.kind()))
}

/// After a start: an install that was started either landed (this is the target version) or
/// rolled back. Returns what happened, and tidies the folder.
#[derive(Debug, PartialEq, Eq)]
pub enum AfterRestart {
    Nothing,
    Updated { from: String, to: String },
    Failed { to: String, attempts: u32 },
}

pub fn after_restart(st: &mut UpdateState, dir: &Path, running: &str) -> AfterRestart {
    if st.target.is_empty() {
        return AfterRestart::Nothing;
    }
    let (Some(run), Some(target)) = (Version::parse(running), Version::parse(&st.target)) else { return AfterRestart::Nothing };
    if run >= target {
        let out = AfterRestart::Updated { from: st.from_version.clone(), to: st.target.clone() };
        if let Some(p) = st.staged_path.take() {
            let _ = std::fs::remove_file(p);
        }
        stage::clean_except(dir, None);
        st.updated_to = Some(running.to_string());
        st.offer = None;
        st.target.clear();
        st.attempts = 0;
        st.last_error.clear();
        st.last_result = "up_to_date".into();
        return out;
    }
    if st.attempts > 0 && st.from_version == running {
        st.last_error = format!("the installer for {} did not finish (see install-{}.log)", st.target, st.target);
        return AfterRestart::Failed { to: st.target.clone(), attempts: st.attempts };
    }
    AfterRestart::Nothing
}

/// At a `--background` start (Windows sign-in), before the shortcut is registered and before
/// anything shows: install a verified staged update, if the gate allows. True: Setup was started
/// and this process must exit now.
pub fn apply_at_background_start() -> bool {
    if !INSTALLS {
        return false;
    }
    let dir = updates_dir();
    let mut st = UpdateState::load(&dir);
    let Some(offer) = st.offer.clone() else { return false };
    if Version::parse(&offer.version).is_none_or(|v| v <= current()) || st.staged_path.is_none() {
        return false;
    }
    let settings = crate::store::Settings::load(&crate::data_dir());
    let inputs = Inputs {
        moment: Moment::BackgroundStart,
        flow: policy::Flow::Idle,
        uploads_busy: false,
        signing_in: false,
        mic_testing: false,
        signin_running: os::process_running("voxa-signin.exe"),
        window_open: os::window_process_running(),
        strip_visible: false,
        since_hotkey_ms: u64::MAX,
        auto_update: settings.auto_update,
        paused: st.paused,
        auto_allowed: offer.auto,
        test_copy: std::env::var_os("VOXA_DATA").is_some(),
        installed_copy: installed_copy(),
        attempts: if st.target == offer.version { st.attempts } else { 0 },
    };
    match launch_installer(&mut st, &dir, &inputs, false, true) {
        Ok(()) => true,
        Err(why) => {
            ulog(&dir, &format!("not installing at sign-in: {why}"));
            false
        }
    }
}

/// Started at Windows sign-in (`--background`, also after an update with `--updated`): never
/// open or activate a window once setup is done. Only a start the user made shows Home.
pub fn opens_window_at_start(background: bool, onboarded: bool) -> bool {
    !background || !onboarded
}

/// macOS: whether to show "Voxa needs Accessibility access again". Only when the app's
/// designated requirement differs from the one recorded at the last start (the grant was tied to
/// the old one), and only until the grant is back. Returns (show, still owed).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn accessibility_bar(prev: Option<&str>, cur: &str, trusted: bool, pending: bool) -> (bool, bool) {
    let changed = prev.is_some_and(|p| !p.is_empty() && p != cur);
    let owed = (pending || changed) && !trusted;
    (owed, owed)
}

pub const ACCESS_TITLE: &str = "Voxa needs Accessibility access again after the update.";
pub const ACCESS_BODY: &str = "macOS treats the new version as a new app, so the switch can look on while it doesn't work. In System Settings, Privacy and Security, Accessibility, select Voxa, remove it with the minus button, then add it again.";

// ── status line (copy deck, docs/updates.md) ─────────────────────────────

fn when(ms: u64, now_ms: u64) -> String {
    use chrono::TimeZone;
    let (Some(t), Some(n)) = (chrono::Local.timestamp_millis_opt(ms as i64).single(), chrono::Local.timestamp_millis_opt(now_ms as i64).single()) else {
        return "recently".into();
    };
    let hm = t.format("%H:%M");
    let days = n.date_naive().signed_duration_since(t.date_naive()).num_days();
    match days {
        0 => format!("today at {hm}"),
        1 => format!("yesterday at {hm}"),
        _ => format!("on {} at {hm}", t.format("%-d %B")),
    }
}

/// The Settings status line.
pub fn status_line(st: &UpdateState, auto: bool, checking: bool, downloading: Option<&str>, manual: bool, now_ms: u64) -> String {
    let cur = env!("CARGO_PKG_VERSION");
    if checking {
        return "Checking for updates...".into();
    }
    if let Some(v) = downloading {
        return format!("Downloading Voxa {v}...");
    }
    let v = st.offer.as_ref().map(|o| o.version.clone()).unwrap_or_default();
    if !st.target.is_empty() && policy::attempts_exhausted(st.attempts) {
        return "Voxa hasn't been able to update for a while. You can download the latest version yourself.".into();
    }
    if !st.target.is_empty() && st.attempts > 0 && st.from_version == cur {
        return format!("The update to {} didn't finish. You're still on {cur}, and Voxa will try again later.", st.target);
    }
    match st.last_result.as_str() {
        "ready" if INSTALLS && auto => format!("Voxa {v} is ready. It installs by itself the next time you're not using Voxa."),
        "ready" if INSTALLS => format!("Voxa {v} is ready to install. Voxa starts again in a few seconds."),
        "ready" | "available" => format!("Voxa {v} is out."),
        "manual_only" => format!("Voxa {v} is out. Download it and install it yourself to update."),
        "up_to_date" => format!("You have the latest version. Last checked {}.", when(st.last_check_ms, now_ms)),
        "offline" => "Couldn't check for updates because you're offline. Voxa will try again later.".into(),
        "refused" if manual => {
            let what = if v.is_empty() { "the update".to_string() } else { v.clone() };
            format!("Voxa didn't install {what} because it couldn't confirm it came from Voxa. Your current version keeps working.")
        }
        "disk_full" => format!("Not enough space to update. Voxa needs about {} MB free.", st.needed_mb.max(1)),
        "failed" | "refused" => "Couldn't check for updates right now. Voxa will try again later.".into(),
        _ => String::new(),
    }
}

// ── the tray's updater ───────────────────────────────────────────────────

pub struct Updater {
    pub state: UpdateState,
    dir: PathBuf,
    checking: bool,
    manual: bool,
    downloading: Option<String>,
    install_after_download: bool,
    debounce: Debounce,
    last_block: Option<Blocked>,
    last_hotkey: Option<Instant>,
    started: Instant,
    started_ms: u64,
    pub mac_access_bar: bool,
}

/// An automatic check goes to the network only when this install's daily slot (or a backoff
/// after a failure) is due, and never in the first 2 minutes after the tray starts. A restart
/// therefore never adds a check.
pub fn should_check(check: &CheckState, now_ms: u64, started_ms: u64) -> bool {
    now_ms >= started_ms + FIRST_CHECK.as_millis() as u64 && check.due(now_ms)
}

impl Updater {
    pub fn new() -> Self {
        let dir = updates_dir();
        let mut state = UpdateState::load(&dir);
        if state.check.slot_ms.is_none() {
            state.check.init_random(crate::now_ms());
        }
        match after_restart(&mut state, &dir, env!("CARGO_PKG_VERSION")) {
            AfterRestart::Updated { from, to } => ulog(&dir, &format!("updated from {from} to {to}")),
            AfterRestart::Failed { to, attempts } => ulog(&dir, &format!("the update to {to} didn't finish (attempt {attempts}); still on {}", env!("CARGO_PKG_VERSION"))),
            AfterRestart::Nothing => {}
        }
        let keep = state.staged_path.clone();
        stage::clean_except(&dir, keep.as_deref());
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut mac_access_bar = false;
        #[cfg(target_os = "macos")]
        {
            let cur = os::designated_requirement().unwrap_or_default();
            let prev = Some(state.mac_requirement.clone()).filter(|p| !p.is_empty());
            let (show, owed) = accessibility_bar(prev.as_deref(), &cur, os::accessibility_trusted(), state.mac_access_pending);
            mac_access_bar = show;
            state.mac_access_pending = owed;
            if !cur.is_empty() {
                state.mac_requirement = cur;
            }
        }
        state.save(&dir);
        let now = Instant::now();
        Self {
            state,
            dir,
            checking: false,
            manual: false,
            downloading: None,
            install_after_download: false,
            debounce: Debounce::default(),
            last_block: None,
            last_hotkey: None,
            started: now,
            started_ms: crate::now_ms(),
            mac_access_bar,
        }
    }

    pub fn hotkey_used(&mut self) {
        self.last_hotkey = Some(Instant::now());
        self.debounce = Debounce::default();
    }

    fn staged_ready(&self) -> bool {
        self.state.staged_path.as_ref().is_some_and(|p| p.exists())
            && self.state.offer.as_ref().and_then(|o| Version::parse(&o.version)).is_some_and(|v| v > current())
    }

    pub fn view(&self, auto: bool) -> UpdateView {
        let offer = self.state.offer.as_ref();
        let gave_up = !self.state.target.is_empty() && policy::attempts_exhausted(self.state.attempts);
        let ready = self.state.last_result == "ready";
        UpdateView {
            status: status_line(&self.state, auto, self.checking, self.downloading.as_deref(), self.manual, crate::now_ms()),
            checking: self.checking,
            manual: self.manual,
            can_restart: INSTALLS && ready && !auto && !gave_up,
            gave_up: gave_up || self.state.last_result == "manual_only",
            version: offer.map(|o| o.version.clone()),
            download_url: offer.map(|o| o.url.clone()),
            notes_url: Some(offer.map(|o| o.notes_url.clone()).unwrap_or_else(|| format!("{RELEASES_TAG}{}", env!("CARGO_PKG_VERSION")))),
            updated_to: if INSTALLS { self.state.updated_to.clone() } else { None },
            mac_available: offer.filter(|o| !INSTALLS && matches!(self.state.last_result.as_str(), "available" | "manual_only") && self.state.dismissed != o.version).map(|o| o.version.clone()),
            mac_accessibility_again: self.mac_access_bar,
        }
    }
}

/// One check through the cache (If-None-Match / If-Modified-Since); `check` comes back with the
/// schedule, backoff and serial updated.
fn check_feed(mut check: CheckState, manual: bool) -> (Result<voxa_update::Manifest, UpdateError>, CheckState) {
    let (origin, keys) = origin_and_keys();
    let result = match Client::new(origin, env!("CARGO_PKG_VERSION"), PLATFORM) {
        Ok(c) => c.check_with_state(&keys, Channel::Desktop, &mut check, crate::now_ms(), manual),
        Err(e) => {
            // Never a tight loop: a client that can't be built backs off like any failure.
            check.record(crate::now_ms(), false, manual);
            Err(e)
        }
    };
    (result, check)
}

fn download_offer(offer: &Offer, dir: &Path) -> Result<PathBuf, UpdateError> {
    let (origin, _) = origin_and_keys();
    let client = Client::new(origin, env!("CARGO_PKG_VERSION"), PLATFORM)?;
    client.download(offer, dir)
}

fn result_code(e: &UpdateError) -> &'static str {
    match e {
        UpdateError::Offline => "offline",
        UpdateError::DiskFull => "disk_full",
        e if e.is_refusal() => "refused",
        _ => "failed",
    }
}

impl Voxa {
    /// From the tray's tick loop, about once a second.
    pub(crate) fn update_tick(&mut self, cx: &mut Context<Self>) {
        if !self.updater.checking && self.updater.downloading.is_none() && should_check(&self.updater.state.check, crate::now_ms(), self.updater.started_ms) {
            self.check_for_updates(false, cx);
        }
        #[cfg(target_os = "macos")]
        if self.updater.mac_access_bar && os::accessibility_trusted() {
            self.updater.mac_access_bar = false;
            self.updater.state.mac_access_pending = false;
            self.updater.state.save(&self.updater.dir);
            self.publish();
        }
        if !INSTALLS || !self.updater.staged_ready() || self.updater.started.elapsed() < FIRST_CHECK {
            return;
        }
        // The idle install, at most once a day.
        if crate::now_ms().saturating_sub(self.updater.state.last_idle_install_ms) < DAY_MS {
            return;
        }
        let inputs = self.gate_inputs(Moment::Idle { locked: os::session_locked(), idle_ms: os::idle_ms() });
        let gate = policy::can_install_now(&inputs);
        if let Err(b) = gate {
            if self.updater.last_block != Some(b) {
                self.updater.last_block = Some(b);
                ulog(&self.updater.dir, &format!("install waits: {}", b.code()));
            }
        }
        if self.updater.debounce.feed(crate::now_ms(), gate.is_ok()) {
            self.updater.state.last_idle_install_ms = crate::now_ms();
            self.install_update(inputs, false, true, cx);
        }
    }

    /// What the gate looks at, from live state.
    fn gate_inputs(&self, moment: Moment) -> Inputs {
        use voxa_core::State;
        let flow = match self.flow.state() {
            State::Idle | State::Blocked(_) => policy::Flow::Idle,
            State::NeedsYou(_) => policy::Flow::NeedsYou,
            State::Recording { paused, .. } => policy::Flow::Recording { paused: *paused },
            State::Transcribing { .. } => policy::Flow::Transcribing,
            State::Delivering { .. } => policy::Flow::Delivering,
            State::Delivered { .. } => policy::Flow::Delivered,
        };
        let offer = self.updater.state.offer.as_ref();
        let same_target = offer.is_some_and(|o| o.version == self.updater.state.target);
        Inputs {
            moment,
            flow,
            uploads_busy: !self.uploads.is_empty() || self.job_entries.values().any(|id| self.is_live(*id)),
            signing_in: self.signing_in,
            mic_testing: self.mic_testing,
            signin_running: os::process_running("voxa-signin.exe"),
            window_open: os::window_process_running(),
            strip_visible: os::strip_shown(),
            since_hotkey_ms: self.updater.last_hotkey.map_or(u64::MAX, |t| t.elapsed().as_millis() as u64),
            auto_update: self.settings.auto_update,
            paused: self.updater.state.paused,
            auto_allowed: offer.is_some_and(|o| o.auto),
            test_copy: std::env::var_os("VOXA_DATA").is_some(),
            installed_copy: installed_copy(),
            attempts: if same_target { self.updater.state.attempts } else { 0 },
        }
    }

    /// Hand over to Setup: flush History, start the installer, quit. Only when the gate holds.
    fn install_update(&mut self, inputs: Inputs, on_request: bool, relaunch: bool, cx: &mut Context<Self>) -> bool {
        let gate = if on_request { policy::can_install_on_request(&inputs) } else { policy::can_install_now(&inputs) };
        if let Err(b) = gate {
            ulog(&self.updater.dir, &format!("install waits: {}", b.code()));
            return false;
        }
        self.keep_unfinished_takes(cx);
        self.history.save();
        let dir = self.updater.dir.clone();
        match launch_installer(&mut self.updater.state, &dir, &inputs, on_request, relaunch) {
            Ok(()) => {
                cx.quit();
                true
            }
            Err(why) => {
                ulog(&dir, &why);
                self.publish();
                false
            }
        }
    }

    /// Tray "Quit Voxa": install a ready update without starting Voxa again (the user asked it to close).
    pub(crate) fn update_on_quit(&mut self, cx: &mut Context<Self>) {
        if INSTALLS && self.updater.staged_ready() {
            let inputs = self.gate_inputs(Moment::Quit);
            self.install_update(inputs, false, false, cx);
        }
    }

    pub(crate) fn check_for_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.updater.checking {
            return;
        }
        // Pressed again within a minute: show the last answer again, no network.
        if manual && !self.updater.state.check.manual_allowed(crate::now_ms()) {
            self.updater.manual = true;
            self.publish();
            return;
        }
        self.updater.checking = true;
        self.updater.manual = manual;
        self.publish();
        let check = self.updater.state.check.clone();
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let (result, check) = cx.background_executor().spawn(async move { check_feed(check, manual) }).await;
            let _ = this.update(cx, |v, cx| v.check_done(result, check, started.elapsed(), cx));
        })
        .detach();
    }

    fn check_done(&mut self, result: Result<voxa_update::Manifest, UpdateError>, check: CheckState, took: Duration, cx: &mut Context<Self>) {
        let u = &mut self.updater;
        u.checking = false;
        u.state.check = check;
        let dir = u.dir.clone();
        let mut download = None;
        match result {
            Ok(m) => {
                u.state.last_check_ms = crate::now_ms();
                {
                    u.state.paused = m.paused;
                    let offer = policy::offer(&m, current(), ASSET);
                    ulog(&dir, &format!("checked: feed {} serial {} paused {} in {} ms: {}", m.version, m.serial, m.paused, took.as_millis(), offer.as_ref().map_or("nothing newer", |_| "offer")));
                    match offer {
                        None => {
                            u.state.last_result = "up_to_date".into();
                            u.state.offer = None;
                            if let Some(p) = u.state.staged_path.take() {
                                let _ = std::fs::remove_file(p);
                            }
                        }
                        Some(o) => {
                            let staged_ok = u.state.offer.as_ref() == Some(&o) && u.state.staged_path.as_ref().is_some_and(|p| stage::verify_file(p, &o).is_ok());
                            if !INSTALLS {
                                u.state.last_result = if o.auto { "available" } else { "manual_only" }.into();
                            } else if !o.auto {
                                u.state.last_result = "manual_only".into();
                            } else if staged_ok {
                                u.state.last_result = "ready".into();
                            } else if self.settings.auto_update || u.install_after_download {
                                u.state.staged_path = None;
                                download = Some(o.clone());
                            } else {
                                // Auto off: the result only; "Restart and update" downloads first.
                                u.state.last_result = "ready".into();
                                u.state.staged_path = None;
                            }
                            u.state.offer = Some(o);
                        }
                    }
                }
            }
            Err(e) => {
                u.state.last_result = result_code(&e).into();
                ulog(&dir, &format!("check failed: {} in {} ms", e.code(), took.as_millis()));
            }
        }
        u.state.save(&dir);
        if let Some(o) = download {
            self.start_download(o, cx);
        }
        self.publish();
    }

    fn start_download(&mut self, offer: Offer, cx: &mut Context<Self>) {
        self.updater.downloading = Some(offer.version.clone());
        let dir = self.updater.dir.clone();
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let o = offer.clone();
            let result = cx.background_executor().spawn(async move { download_offer(&o, &dir) }).await;
            let _ = this.update(cx, |v, cx| v.download_done(offer, result, started.elapsed(), cx));
        })
        .detach();
        self.publish();
    }

    fn download_done(&mut self, offer: Offer, result: Result<PathBuf, UpdateError>, took: Duration, cx: &mut Context<Self>) {
        let u = &mut self.updater;
        u.downloading = None;
        let dir = u.dir.clone();
        match result {
            Ok(path) => {
                ulog(&dir, &format!("downloaded {} ({} bytes in {} ms), verified", offer.version, offer.size, took.as_millis()));
                stage::clean_except(&dir, Some(&path));
                u.state.staged_path = Some(path);
                u.state.last_result = "ready".into();
            }
            Err(e) => {
                ulog(&dir, &format!("download of {} failed: {}", offer.version, e.code()));
                u.state.last_result = result_code(&e).into();
                u.state.needed_mb = offer.size.div_ceil(1024 * 1024);
                u.install_after_download = false;
            }
        }
        u.state.save(&dir);
        if self.updater.install_after_download && self.updater.staged_ready() {
            self.updater.install_after_download = false;
            let inputs = self.gate_inputs(Moment::Quit);
            self.install_update(inputs, true, true, cx);
        }
        self.publish();
    }

    /// The window's update commands.
    pub(crate) fn on_update_command(&mut self, cmd: &crate::link::Command, cx: &mut Context<Self>) {
        use crate::link::Command::*;
        match cmd {
            CheckForUpdates => self.check_for_updates(true, cx),
            InstallUpdateNow if INSTALLS => {
                if self.updater.staged_ready() {
                    let inputs = self.gate_inputs(Moment::Quit);
                    self.install_update(inputs, true, true, cx);
                } else if let Some(o) = self.updater.state.offer.clone() {
                    self.updater.install_after_download = true;
                    self.start_download(o, cx);
                }
            }
            OpenReleaseNotes => {
                let url = self.updater.view(self.settings.auto_update).notes_url.unwrap_or_default();
                if url.starts_with(voxa_update::REPO_URL) || crate::test_hooks() {
                    os::open(std::ffi::OsStr::new(&url));
                }
            }
            DismissUpdateNote => {
                self.updater.state.updated_to = None;
                if let Some(o) = &self.updater.state.offer {
                    self.updater.state.dismissed = o.version.clone();
                }
                self.updater.state.save(&self.updater.dir);
            }
            _ => {}
        }
        self.publish();
    }

    pub(crate) fn update_view(&self) -> UpdateView {
        self.updater.view(self.settings.auto_update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voxa-desktop-update-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn offer_for(data: &[u8], version: &str) -> Offer {
        Offer {
            version: version.into(),
            asset_key: voxa_update::ASSET_WINDOWS.into(),
            url: format!("https://github.com/blackscythe123/voxa/releases/download/v{version}/Voxa-Setup-{version}.exe"),
            sha256: stage::sha256_hex(data),
            size: data.len() as u64,
            auto: true,
            ..Offer::default()
        }
    }

    fn ok_inputs() -> Inputs {
        Inputs {
            moment: Moment::Idle { locked: true, idle_ms: 0 },
            flow: policy::Flow::Idle,
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
    fn installer_arguments_are_silent_and_skip_removeold() {
        let log = Path::new(r"C:\Users\A B\AppData\Local\Voxa\updates\install-1.0.1.log");
        let a = installer_args("1.0.0", "1.0.1", log, true);
        assert_eq!(
            a,
            vec![
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/NOCANCEL",
                "/SP-",
                "/VOXAUPDATE=1.0.0",
                "/MERGETASKS=\"!removeold\"",
                r#"/LOG="C:\Users\A B\AppData\Local\Voxa\updates\install-1.0.1.log""#,
            ]
        );
        let quit = installer_args("1.0.0", "1.0.1", log, false);
        assert_eq!(quit.last().map(String::as_str), Some("/VOXANORELAUNCH=1"), "Quit Voxa: no relaunch");
        // The script itself honours these: the silent removeold guard and the relaunch.
        let iss = include_str!("../../windows/voxa.iss");
        assert!(iss.contains("Check: OldVoxaInstalled and not WizardSilent"));
        assert!(iss.contains("(not WizardSilent) and WizardIsTaskSelected('removeold')"));
        assert!(iss.contains("'--background --updated'"));
        assert!(iss.contains("{param:VOXAUPDATE|}"));
        assert!(iss.contains("{param:VOXANORELAUNCH|}"));
        assert!(iss.contains("CheckForMutexes('Local\\VoxaNextSingleInstance,Local\\VoxaNextWindow')"));
        assert!(iss.contains("CloseApplications=no") && !iss.contains("AppMutex="), "never AppMutex: it aborts on the tray that launched Setup");
        assert!(!iss.contains("skipifsilent") || iss.contains("DeinitializeSetup"), "a silent run relaunches from DeinitializeSetup");
    }

    #[test]
    fn the_staged_installer_is_verified_again_before_launch() {
        let dir = tmp("verify");
        let data = vec![7u8; 4096];
        let o = offer_for(&data, "9.0.0");
        let path = dir.join("Voxa-Setup-9.0.0.exe");
        let mut bytes = data.clone();
        bytes[10] ^= 1; // tampered after the download
        std::fs::write(&path, &bytes).unwrap();
        let mut st = UpdateState { offer: Some(o), staged_path: Some(path.clone()), ..Default::default() };
        let r = launch_installer(&mut st, &dir, &ok_inputs(), false, true);
        assert!(r.unwrap_err().contains("failed its check"));
        assert!(!path.exists(), "a tampered file is deleted, never launched");
        assert!(st.staged_path.is_none());
        assert_eq!(st.attempts, 0, "nothing was started, so no attempt is counted");
        // And a blocked gate never even looks at the file.
        let mut st = UpdateState { offer: Some(offer_for(&data, "9.0.0")), staged_path: Some(path.clone()), ..Default::default() };
        let busy = Inputs { flow: policy::Flow::Recording { paused: true }, ..ok_inputs() };
        assert_eq!(launch_installer(&mut st, &dir, &busy, false, true), Err("waiting: dictation".into()));
        let test_copy = Inputs { test_copy: true, ..ok_inputs() };
        assert_eq!(launch_installer(&mut st, &dir, &test_copy, true, true), Err("waiting: test_copy".into()));
    }

    #[test]
    fn a_background_relaunch_never_activates_a_window() {
        // `--background --updated` after an update: onboarded users get no window at all.
        assert!(!opens_window_at_start(true, true));
        assert!(opens_window_at_start(false, true), "a start the user made shows Home");
        assert!(opens_window_at_start(true, false), "setup not finished yet: the tour");
        let main = include_str!("main.rs");
        assert!(main.contains("update::opens_window_at_start(background, onboarded)"), "main() decides with this rule");
    }

    #[test]
    fn updated_flag_with_matching_version_cleans_up_and_shows_the_note() {
        let dir = tmp("after");
        let staged = dir.join("Voxa-Setup-1.0.1.exe");
        std::fs::write(&staged, b"x").unwrap();
        std::fs::write(dir.join(".part-1-0"), b"x").unwrap();
        let mut st = UpdateState { target: "1.0.1".into(), attempts: 1, from_version: "1.0.0".into(), staged_path: Some(staged.clone()), offer: Some(offer_for(b"x", "1.0.1")), ..Default::default() };
        assert_eq!(after_restart(&mut st, &dir, "1.0.1"), AfterRestart::Updated { from: "1.0.0".into(), to: "1.0.1".into() });
        assert!(!staged.exists() && !dir.join(".part-1-0").exists());
        assert_eq!(st.updated_to.as_deref(), Some("1.0.1"));
        assert_eq!((st.attempts, st.target.as_str(), st.offer.is_none()), (0, "", true));
        assert_eq!(after_restart(&mut st, &dir, "1.0.1"), AfterRestart::Nothing, "only once");
        // A rollback: still the old version.
        let mut st = UpdateState { target: "1.0.1".into(), attempts: 1, from_version: "1.0.0".into(), ..Default::default() };
        assert_eq!(after_restart(&mut st, &dir, "1.0.0"), AfterRestart::Failed { to: "1.0.1".into(), attempts: 1 });
        assert!(st.updated_to.is_none());
        // Settings then says so (from_version is the running build).
        st.from_version = env!("CARGO_PKG_VERSION").into();
        assert!(status_line(&st, true, false, None, false, 0).starts_with("The update to 1.0.1 didn't finish."));
    }

    #[test]
    fn two_failed_installs_stop_and_offer_a_download() {
        let st = UpdateState { target: "1.0.1".into(), attempts: 2, from_version: env!("CARGO_PKG_VERSION").into(), offer: Some(offer_for(b"x", "1.0.1")), ..Default::default() };
        assert_eq!(status_line(&st, true, false, None, false, 0), "Voxa hasn't been able to update for a while. You can download the latest version yourself.");
        let u = Updater { state: st, ..test_updater() };
        let v = u.view(true);
        assert!(v.gave_up && !v.can_restart);
        assert!(v.download_url.unwrap().ends_with("Voxa-Setup-1.0.1.exe"));
        let exhausted = Inputs { attempts: 2, ..ok_inputs() };
        assert_eq!(policy::can_install_now(&exhausted), Err(Blocked::AttemptsExhausted));
    }

    fn test_updater() -> Updater {
        let now = Instant::now();
        Updater {
            state: UpdateState::default(),
            dir: std::env::temp_dir(),
            checking: false,
            manual: false,
            downloading: None,
            install_after_download: false,
            debounce: Debounce::default(),
            last_block: None,
            last_hotkey: None,
            started: now,
            started_ms: 0,
            mac_access_bar: false,
        }
    }

    #[test]
    fn the_status_line_follows_the_copy_deck() {
        let mut st = UpdateState { offer: Some(offer_for(b"x", "1.1.0")), last_result: "ready".into(), ..Default::default() };
        if INSTALLS {
            assert_eq!(status_line(&st, true, false, None, false, 0), "Voxa 1.1.0 is ready. It installs by itself the next time you're not using Voxa.");
            assert_eq!(status_line(&st, false, false, None, false, 0), "Voxa 1.1.0 is ready to install. Voxa starts again in a few seconds.");
        }
        assert_eq!(status_line(&st, true, true, None, true, 0), "Checking for updates...");
        assert_eq!(status_line(&st, true, false, Some("1.1.0"), false, 0), "Downloading Voxa 1.1.0...");
        st.last_result = "offline".into();
        assert_eq!(status_line(&st, true, false, None, false, 0), "Couldn't check for updates because you're offline. Voxa will try again later.");
        st.last_result = "refused".into();
        assert_eq!(status_line(&st, true, false, None, false, 0), "Couldn't check for updates right now. Voxa will try again later.", "a background refusal is quiet");
        assert_eq!(status_line(&st, true, false, None, true, 0), "Voxa didn't install 1.1.0 because it couldn't confirm it came from Voxa. Your current version keeps working.");
        st.last_result = "up_to_date".into();
        let now = crate::now_ms();
        st.last_check_ms = now;
        assert!(status_line(&st, true, false, None, false, now).starts_with("You have the latest version. Last checked today at "));
        // Copy rules: no dashes, no emojis.
        for r in ["ready", "offline", "refused", "up_to_date", "disk_full", "manual_only", "failed"] {
            st.last_result = r.into();
            for manual in [false, true] {
                let s = status_line(&st, true, false, None, manual, now);
                assert!(s.is_ascii() && !s.contains(" - "), "{s}");
            }
        }
    }

    #[test]
    fn a_start_does_not_check_when_not_due() {
        let day = voxa_update::schedule::DAY_MS;
        let mut c = CheckState::default();
        c.init(10 * day, 3 * 60 * 60 * 1000); // this install's slot: 03:00 UTC
        let started = 10 * day + 4 * 60 * 60 * 1000; // the tray started at 04:00, after today's slot
        // Checked fine yesterday at the slot: the next one is tomorrow's slot, a restart adds none.
        c.record(10 * day + 3 * 60 * 60 * 1000, true, false);
        assert!(!should_check(&c, started + 10 * 60 * 1000, started), "not due: no network at start");
        // Due, but never in the first 2 minutes after the tray starts.
        let due = c.next_due_ms;
        assert!(!should_check(&c, due, due - 1000));
        assert!(should_check(&c, due + 120_000, due - 1000));
        // A failed automatic check backs off an hour, never a tight loop.
        c.record(due + 120_000, false, false);
        assert!(!should_check(&c, due + 120_000 + 59 * 60 * 1000, 0));
        assert!(should_check(&c, due + 120_000 + 60 * 60 * 1000, 0));
    }

    #[test]
    fn the_manual_button_within_a_minute_does_no_network() {
        let mut c = CheckState::default();
        c.init(1_000_000, 0);
        assert!(c.manual_allowed(2_000_000));
        c.record(2_000_000, true, true);
        assert!(!c.manual_allowed(2_000_000 + 30_000), "pressed again: the last answer, no request");
        assert!(c.manual_allowed(2_000_000 + 60_000));
        // check_for_updates honours it before anything goes to the network.
        let src = include_str!("update.rs");
        let f = &src[src.find("pub(crate) fn check_for_updates").unwrap()..];
        let guard = f.find("manual_allowed(").unwrap();
        assert!(guard < f.find("check_feed(").unwrap());
    }

    #[test]
    fn update_log_is_appended_and_rotated_never_deleted() {
        let dir = tmp("log");
        ulog(&dir, "first");
        ulog(&dir, "second");
        let text = std::fs::read_to_string(dir.join("update.log")).unwrap();
        assert!(text.contains("first") && text.contains("second"));
        std::fs::write(dir.join("update.log"), vec![b'x'; (LOG_CAP + 1) as usize]).unwrap();
        ulog(&dir, "third");
        assert!(dir.join("update.old.log").exists(), "the full log moves aside");
        assert!(std::fs::read_to_string(dir.join("update.log")).unwrap().contains("third"));
    }

    #[test]
    fn update_state_survives_the_file_trip() {
        let dir = tmp("state");
        let st = UpdateState { last_check_ms: 5, check: CheckState { highest_serial: 9, ..Default::default() }, offer: Some(offer_for(b"x", "1.0.1")), target: "1.0.1".into(), attempts: 1, ..Default::default() };
        st.save(&dir);
        assert_eq!(UpdateState::load(&dir), st);
        assert_eq!(UpdateState::load(&dir.join("missing")), UpdateState::default());
    }

    #[test]
    fn only_the_installed_copy_counts_as_installed() {
        let local = Path::new(r"C:\Users\a\AppData\Local");
        assert!(is_installed_path(Path::new(r"C:\Users\a\AppData\Local\Programs\Voxa\Voxa.exe"), local));
        assert!(is_installed_path(Path::new(r"c:\users\a\appdata\local\programs\voxa\Voxa.exe"), local));
        assert!(!is_installed_path(Path::new(r"C:\projects\voxa\target\release\voxa-desktop.exe"), local));
        assert!(!is_installed_path(Path::new(r"C:\Users\a\AppData\Local\Programs\Voxa\sub\Voxa.exe"), local));
    }

    #[test]
    fn the_accessibility_bar_shows_only_when_the_signature_changed() {
        let a = "designated => identifier \"app.voxa.desktop\" and certificate leaf = H\"aa\"";
        let b = "designated => cdhash H\"bb\"";
        assert_eq!(accessibility_bar(Some(a), a, false, false), (false, false), "same requirement: no bar");
        assert_eq!(accessibility_bar(None, a, false, false), (false, false), "first start: setup asks, no bar");
        assert_eq!(accessibility_bar(Some(b), a, false, false), (true, true), "changed and not trusted: bar");
        assert_eq!(accessibility_bar(Some(b), a, true, false), (false, false), "changed but still trusted: no bar");
        assert_eq!(accessibility_bar(Some(a), a, false, true), (true, true), "owed from the last start until the grant is back");
        assert_eq!(accessibility_bar(Some(a), a, true, true), (false, false));
        for s in [ACCESS_TITLE, ACCESS_BODY] {
            assert!(s.is_ascii() && !s.contains(" - "), "{s}");
        }
        assert!(ACCESS_BODY.contains("remove it with the minus button, then add it again"));
    }
}
