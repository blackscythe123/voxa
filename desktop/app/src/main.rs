//! Voxa for Windows. The strip and tray are drawn by GPUI; every state, word and button comes
//! from voxa-core. This file only turns core effects into Windows actions.

#![windows_subsystem = "windows"]

mod home;
mod input;
mod import;
mod link;
mod notify;
mod prepare;
mod redact;
mod remote;
mod talk;
mod store;
mod audio;
mod stand_in;
mod common;
mod theme;
#[cfg(windows)]
mod tab;
/// The per-OS layer: win.rs on Windows, mac.rs on macOS. Same functions on both.
#[cfg_attr(windows, path = "win.rs")]
#[cfg_attr(target_os = "macos", path = "mac.rs")]
mod os;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{
    AnyElement, App, Application, Bounds, ClickEvent, Context, Hsla, PathBuilder, Render, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, point, prelude::*, px, rgb,
    rgba, size,
};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use home::{Home, Page};
use remote::Backend;
use store::{Entry, History, Settings, Status};
use voxa_core::copy::strip_text;
use voxa_core::flow::Delivery;
use voxa_chatgpt::{ChatGpt, Session as GptSession};
use voxa_core::{Effect, Event, Flow, JobId, Mic, Network, Problem, Readiness, Session, State, Target, TranscribeError};

/// The 1.0 sepia: 8.2:1 with white, 7.7:1 on the strip.
pub(crate) const INK: u32 = 0x7A3F22;
/// Below this peak RMS a recording is treated as silence and never sent. Measured: soft speech
/// peaked at 0.0156, a silent take at 0.0079, so the cutoff sits well under soft speech.
const SILENCE: f32 = 0.003;
/// Whisper invents these for near-silent audio. Only trusted when the recording was also quiet.
const QUIET: f32 = 0.02;
const SILENCE_PHRASES: &[&str] = &["thank you.", "thank you", "thanks for watching!", "thanks for watching.", "you", "bye."];

fn invented_for_silence(text: &str, peak: f32) -> bool {
    peak < QUIET && SILENCE_PHRASES.contains(&text.trim().to_lowercase().as_str())
}
const RECHECK: Duration = Duration::from_secs(600);
/// A check that finds the session ended is repeated this much later before Voxa says so: a
/// false "signed out" is worse than a late one.
const CONFIRM_EXPIRY: Duration = Duration::from_secs(120);

/// Test only (with a separate VOXA_DATA folder): VOXA_TEST_SESSION=valid,expired,expired makes
/// the next session checks answer that, in order, without asking ChatGPT, and
/// VOXA_TEST_CONFIRM_SECS shortens the confirmation wait. Used to test the sign-in toast.
fn test_session_answer() -> Option<Session> {
    static SEQ: std::sync::Mutex<Option<VecDeque<String>>> = std::sync::Mutex::new(None);
    std::env::var_os("VOXA_DATA")?;
    let mut g = SEQ.lock().unwrap_or_else(|e| e.into_inner());
    let q = g.get_or_insert_with(|| std::env::var("VOXA_TEST_SESSION").unwrap_or_default().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
    match q.pop_front()?.as_str() {
        "valid" => Some(Session::Valid),
        "expired" => Some(Session::Expired),
        _ => None,
    }
}

/// Test only (VOXA_TEST_RECHECK_SECS, with VOXA_DATA): check the session more often than every 10 minutes.
fn recheck_every() -> Duration {
    std::env::var("VOXA_TEST_RECHECK_SECS").ok().filter(|_| std::env::var_os("VOXA_DATA").is_some()).and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(RECHECK)
}

fn confirm_after() -> Duration {
    std::env::var("VOXA_TEST_CONFIRM_SECS").ok().filter(|_| std::env::var_os("VOXA_DATA").is_some()).and_then(|s| s.parse().ok()).map(Duration::from_secs).unwrap_or(CONFIRM_EXPIRY)
}

fn fake_mode() -> bool {
    std::env::var("VOXA_FAKE").is_ok_and(|v| !v.is_empty())
}

/// Where the OS keeps per-user app data: %APPDATA% on Windows, ~/Library/Application Support on Mac.
fn app_data_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join("Library").join("Application Support")
    } else {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into()))
    }
}

/// Voxa's data folder (voxa-next), or VOXA_DATA (tests use a throwaway folder so real history is untouched).
pub(crate) fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return d.into();
    }
    app_data_root().join("voxa-next")
}

/// Keep who is signed in for the Settings page, and their picture (fetched only when it changes).
fn save_account(a: &voxa_chatgpt::Account) {
    let dir = data_dir();
    let path = dir.join("account.json");
    let before: voxa_chatgpt::Account = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if before == *a && (a.image.is_empty() || dir.join("avatar.png").exists()) {
        return;
    }
    if let Ok(b) = serde_json::to_vec_pretty(a) {
        let _ = std::fs::write(&path, b);
    }
    if a.image.starts_with("https://") && (before.image != a.image || !dir.join("avatar.png").exists()) {
        let url = a.image.clone();
        let out = dir.join("avatar.png");
        std::thread::spawn(move || {
            let img = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .ok()
                .and_then(|c| c.get(url).send().ok())
                .filter(|r| r.status().is_success())
                .and_then(|r| r.bytes().ok())
                .and_then(|b| image::load_from_memory(&b).ok());
            if let Some(img) = img {
                let _ = img.resize_to_fill(96, 96, image::imageops::FilterType::Lanczos3).save_with_format(&out, image::ImageFormat::Png);
                os::log("profile picture saved");
            }
        });
    }
}

/// Goes up on every sign-in and sign-out. Work that started under an older value (a session
/// check, an upload that can take minutes) never writes the session back, in memory or on disk.
static SESSION_GEN: AtomicU64 = AtomicU64::new(0);

fn session_gen() -> u64 {
    SESSION_GEN.load(Ordering::SeqCst)
}

fn lock_session_file() -> std::sync::MutexGuard<'static, ()> {
    voxa_chatgpt::SESSION_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ask ChatGPT whether the saved session is good: (session, readiness, network, definite).
/// `definite`: ChatGPT's own session call answered with no token (not a 401 or 403, which may be
/// a rate limit; not "nothing saved", not Cloudflare, not offline), the only answer that may
/// lead to a "signed out" toast. `generation`: [`session_gen`] when the check was asked for.
fn ask_session(gpt: &ChatGpt, generation: u64) -> (Option<GptSession>, Session, Network, bool) {
    let dir = data_dir();
    // Carry an Electron sign-in over once, unless the user signed out here. Never for a test copy
    // (VOXA_DATA): it must not pick up the real account and send a test take with it.
    let carry_over = !dir.join("signed-out").exists() && std::env::var_os("VOXA_DATA").is_none();
    let saved = GptSession::from_voxa_dir(&dir).or_else(|| if carry_over { GptSession::from_electron_dir(&electron_dir()) } else { None });
    if let (Some(s), Some(answer)) = (saved.clone(), test_session_answer()) {
        os::log(&format!("test: session check answers {answer:?}"));
        return (Some(s), answer, Network::Up, answer == Session::Expired);
    }
    use voxa_chatgpt::SessionAnswer;
    match saved {
        None => (None, Session::Expired, Network::Up, false),
        Some(s) => match gpt.check(&s) {
            SessionAnswer::Valid(a) => {
                if session_gen() == generation {
                    save_account(&a);
                }
                let s = keep_refreshed_cookies(gpt, s, generation);
                (Some(s), Session::Valid, Network::Up, false)
            }
            SessionAnswer::NoToken => (Some(s), Session::Expired, Network::Up, true),
            SessionAnswer::Failed(TranscribeError::SessionExpired | TranscribeError::SignedOut) => (Some(s), Session::Expired, Network::Up, false),
            SessionAnswer::Failed(TranscribeError::Offline) => (Some(s), Session::Unknown, Network::Down, false),
            SessionAnswer::Failed(_) => (Some(s), Session::Unknown, Network::Up, false),
        },
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Where the current (Electron) Voxa saved its session. Used once, to carry a user's sign-in over.
fn electron_dir() -> PathBuf {
    // A test copy (VOXA_DATA) must never see the real old app: no sign-in, no History carried over.
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return PathBuf::from(d).join("no-old-app");
    }
    app_data_root().join(if cfg!(target_os = "macos") { "Voxa" } else { "voxa" })
}

pub(crate) struct Voxa {
    pub(crate) flow: Flow,
    pub(crate) history: History,
    pub(crate) settings: Settings,
    pub(crate) mic_name: String,
    /// The setup's "Say something" step: mic open, nothing kept.
    mic_testing: bool,
    /// History entry id for each job of this run.
    job_entries: HashMap<JobId, u64>,
    /// The job state last written to History, so an entry is only rewritten when its dictation
    /// moves on (otherwise the user's edits and deletes would be undone by the next event).
    job_synced: HashMap<JobId, voxa_core::JobState>,
    /// What went wrong with a take's audio (mic stopped, disk), decided when it stops.
    trouble: HashMap<JobId, audio::Trouble>,
    /// When the shortcut was paused for picking a new one (resumed if the window goes quiet).
    hotkey_paused_at: Option<Instant>,
    /// Draw nothing in the strip (while a rebuilt strip paints its first frame unseen).
    strip_blank: bool,
    /// The OS is in dark mode (for Appearance "Use system setting").
    os_dark: bool,
    /// Test only (VOXA_TEST_STRIP, with VOXA_DATA): a stand-alone flow the strip draws instead of
    /// the real one, so its look can be checked without recording. See `strip_preview`.
    strip_preview: Option<Flow>,
    /// The strip window now in use (replaced when it is rebuilt).
    strip_window: Option<gpui::AnyWindowHandle>,
    job_secs: HashMap<JobId, f32>,
    /// Last state published to the window process (live.json), to write only on change.
    published: Option<link::Live>,
    hotkey_result: Option<(String, Option<String>)>,
    hotkey_paused: bool,
    /// What each job was really dictated into, read off the UI thread while the user talks:
    /// (name, host), e.g. ("mangafire.to", "Brave") or ("Claude Code", "VS Code").
    job_context: Arc<std::sync::Mutex<HashMap<JobId, (String, String)>>>,
    /// The ChatGPT sign-in window is open.
    pub(crate) signing_in: bool,
    /// When the shortcut went down, while it is held.
    holding: Option<Instant>,
    menu_open: MenuId,
    menu_quit: MenuId,
    /// When the mic last went live in this take (start or resume), for the stalled-mic watchdog.
    rec_started: Instant,
    /// Talking time in this take; paused time is left out, also when stopped while paused.
    talk: talk::TalkClock,
    /// History entries being resent now, so one recording is never uploaded twice at once.
    sending: std::collections::HashSet<u64>,
    frame: u64,
    tray: Option<TrayIcon>,
    hotkeys: GlobalHotKeyManager,
    main_key: HotKey,
    esc_key: HotKey,
    esc_bound: bool,
    mic: Option<audio::Mic>,
    mic_on: bool,
    /// The stalled-mic watchdog already restarted the stream for this take.
    mic_revived: bool,
    /// Settings were read from disk without error (else never prune, import or overwrite them).
    settings_ok: bool,
    audio_paths: HashMap<JobId, PathBuf>,
    peaks: HashMap<JobId, f32>,
    pub(crate) levels: VecDeque<f32>,
    gpt: Arc<ChatGpt>,
    session: Option<GptSession>,
    last_check: Instant,
    /// Toasts (docs/notifications.md).
    notifier: notify::Notifier,
    /// What the last session check said for certain (never from a failed transcription).
    session_known: Option<Session>,
    /// A second check is due to confirm an expiry.
    confirming: bool,
}

impl Voxa {
    fn new(cx: &mut Context<Self>) -> Self {
        let session = if fake_mode() {
            if stand_in::starts_signed_out() { Session::Expired } else { Session::Valid }
        } else {
            Session::Unknown
        };
        let built = audio::Mic::with_preferred(Settings::load(&data_dir()).mic_device);
        let mic = match &built {
            Ok(m) => {
                os::log(&format!("microphone ready (paused): {}", m.device_name()));
                Mic::Granted
            }
            Err(audio::MicError::NoDevice) => Mic::Missing,
            Err(e) => {
                os::log(&format!("microphone unavailable: {e:?}"));
                Mic::Denied
            }
        };
        let flow = Flow::new(Readiness { session, network: Network::Up, mic });
        // A settings file that exists but can't be read must not turn into defaults that then
        // prune history, re-run the import or overwrite the user's file.
        let (mut settings, settings_ok) = match Settings::try_load(&data_dir()) {
            Ok(s) => (s.unwrap_or_default(), true),
            Err(e) => {
                os::log(&format!("FAIL reading settings ({e}): using defaults for now, not saving or pruning"));
                (Settings::default(), false)
            }
        };
        let mut history = History::load(&data_dir());
        if settings_ok {
            if let Some(n) = import::run_once(&electron_dir(), &data_dir(), &mut history, &mut settings) {
                os::log(&format!("imported {n} dictations from the old Voxa"));
            }
            if history.prune(settings.keep_days, now_ms()) > 0 {
                history.save();
            }
            if history.prune_audio(settings.keep_audio_days, now_ms()) > 0 {
                history.save();
            }
        }
        let apps: std::collections::HashSet<String> = history.entries.iter().map(|e| e.via.clone().unwrap_or_else(|| e.app.clone())).collect();
        os::refresh_site_icons_once();
        for site in history.entries.iter().map(|e| e.app.as_str()).filter(|a| os::is_known_site(a)) {
            os::cache_site_icon(site);
        }
        std::thread::spawn(move || os::cache_open_app_icons(apps));
        let hotkeys = GlobalHotKeyManager::new().expect("hotkey manager");
        let main_key = settings.hotkey.parse::<HotKey>().unwrap_or_else(|_| {
            settings.hotkey = Settings::default().hotkey;
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
        });
        let esc_key = HotKey::new(None, Code::Escape);
        let mut main_key = main_key;
        match hotkeys.register(main_key) {
            Ok(()) => os::log(&format!("hotkey {} registered", settings.hotkey)),
            Err(e) => {
                // Taken by another app (e.g. the old Voxa still running): never end up with no
                // shortcut; fall back to the default and say so in Settings.
                os::log(&format!("hotkey {} unavailable ({e}); using the default", settings.hotkey));
                let fallback = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
                if fallback.id() != main_key.id() && hotkeys.register(fallback).is_ok() {
                    main_key = fallback;
                    settings.hotkey = Settings::default().hotkey;
                    if settings_ok {
                        settings.save(&data_dir());
                    }
                }
            }
        }
        let menu = Menu::new();
        let open_item = MenuItem::new("Open Voxa", true, None);
        let quit_item = MenuItem::new("Quit Voxa", true, None);
        let _ = menu.append_items(&[&open_item, &PredefinedMenuItem::separator(), &quit_item]);
        let tray = TrayIconBuilder::new()
            .with_tooltip("Voxa")
            .with_icon(tray_icon(None))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok();
        let mic_name = built.as_ref().map(|m| m.device_name().to_string()).unwrap_or_else(|_| "No microphone".into());
        // macOS: pasting into other apps needs the Accessibility permission (asks once).
        if !fake_mode() && !os::can_paste() {
            os::log("paste needs the Accessibility permission (System Settings > Privacy & Security)");
        }

        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(40)).await;
            let alive = this
                .update(cx, |v, cx| {
                    // A take whose mic delivers nothing 0.7 s in: restart the stream once (no UI).
                    // Never while paused; after a resume it judges the time since the resume.
                    if v.mic_on && !v.paused() && !v.mic_revived && v.rec_started.elapsed() > Duration::from_millis(700) {
                        if let Some(m) = v.mic.as_mut() {
                            if m.stalled() {
                                v.mic_revived = true;
                                os::log("  mic delivered no sound: restarting the stream");
                                if let Err(e) = m.revive() {
                                    os::log(&format!("  mic restart failed: {e:?}"));
                                }
                            }
                        }
                    }
                    if v.hotkey_paused && v.hotkey_paused_at.is_some_and(|t| t.elapsed() > Duration::from_secs(30)) {
                        os::log("shortcut picking stopped answering: resuming the shortcut");
                        v.pause_hotkey(false);
                    }
                    while let Ok(ev) = MenuEvent::receiver().try_recv() {
                        if ev.id == v.menu_open {
                            v.open_home(None, cx);
                        } else if ev.id == v.menu_quit {
                            os::log("quit from tray");
                            v.mic_test(false, cx);
                            v.keep_unfinished_takes(cx);
                            cx.quit();
                        }
                    }
                    while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                            v.open_home(None, cx);
                        }
                    }
                    for cmd in link::take_commands() {
                        v.on_command(cmd, cx);
                    }
                    for click in notify::take_clicks() {
                        v.on_notification(click, cx);
                    }
                    if os::OPEN_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        os::log("another launch asked to open the window");
                        v.open_home(None, cx);
                    }
                    if v.mic_testing {
                        let lvl = audio::level().max(v.levels.back().copied().unwrap_or(0.0) * 0.88);
                        v.levels.push_back(lvl);
                        while v.levels.len() > QUILL_CYCLE {
                            v.levels.pop_front();
                        }
                        cx.notify();
                    }
                    while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                        // Hold to talk: letting go after holding finishes the dictation. A quick
                        // tap leaves it on, and the next press stops it, as in toggle mode.
                        if ev.id == v.main_key.id() && ev.state == HotKeyState::Released {
                            let held = std::mem::replace(&mut v.holding, None);
                            if v.settings.hold_to_talk && held.is_some_and(|t| t.elapsed() > Duration::from_millis(400)) && matches!(v.flow.state(), State::Recording { .. }) {
                                os::log("shortcut released: finishing");
                                v.handle(Event::Stop, cx);
                            }
                            continue;
                        }
                        if ev.state != HotKeyState::Pressed {
                            continue;
                        }
                        if ev.id == v.main_key.id() {
                            // Key repeat while held must not toggle again.
                            if v.holding.is_some() {
                                continue;
                            }
                            v.holding = Some(Instant::now());
                            if v.mic_testing {
                                v.mic_test(false, cx);
                            }
                            let fg = os::foreground();
                            let target = Target { app: os::app_name(fg), window: fg as u64 };
                            os::cache_app_icon(fg, os::icon_owner(fg));
                            os::cache_site_icon(&target.app);
                            os::log(&format!("hotkey in {}", os::describe(fg)));
                            // The strip's graphics may be gone (sleep, charger, display change):
                            // build a fresh one now, hidden and without taking focus; it replaces
                            // the old one as this dictation's strip.
                            v.rebuild_strip_if_woke(cx);
                            // The mic was missing or refused (at startup, or one failed start):
                            // look again now, so plugging it in or allowing it works without a restart.
                            if !matches!(v.flow.readiness().mic, Mic::Granted) && !fake_mode() {
                                match audio::Mic::with_preferred(v.settings.mic_device.clone()) {
                                    Ok(m) => {
                                        os::log(&format!("microphone found again: {}", m.device_name()));
                                        v.mic_name = m.device_name().to_string();
                                        v.mic = Some(m);
                                        let r = Readiness { mic: Mic::Granted, ..v.flow.readiness() };
                                        v.handle(Event::Readiness(r), cx);
                                    }
                                    Err(e) => os::log(&format!("microphone still unavailable: {e:?}")),
                                }
                            }
                            let browser = os::browser_name(fg);
                            let (pid, stem) = os::window_process(fg);
                            v.handle(Event::Hotkey { target }, cx);
                            // What exactly is in front (the site in a browser tab, Claude Code in an
                            // editor): read while the user talks, off the UI thread.
                            if let State::Recording { job, .. } = v.flow.state().clone() {
                                let map = v.job_context.clone();
                                let hwnd = fg as usize;
                                std::thread::spawn(move || {
                                    let found = match browser {
                                        Some(b) => os::domain_of(hwnd).map(|d| {
                                            os::cache_domain_icon_or_site(&d);
                                            (os::site_for_domain(&d), b)
                                        }),
                                        None => os::coding_agent(&stem, pid).inspect(|(name, _)| os::cache_named_icon(name, "claude.ai")),
                                    };
                                    if let Some(f) = found {
                                        map.lock().unwrap().insert(job, f);
                                    }
                                });
                            }
                        } else if ev.id == v.esc_key.id() {
                            v.handle(Event::Cancel, cx);
                        }
                    }
                    if matches!(v.flow.state(), State::Recording { paused: false, .. }) {
                        let raw = if fake_mode() { stand_in::level(v.frame as f32 * 0.35) } else { audio::level() };
                        // Rises at once with the voice, settles over ~0.3 s, so the ink doesn't flicker.
                        let last = v.levels.back().copied().unwrap_or(0.0);
                        let lvl = raw.max(last * 0.88);
                        v.levels.push_back(lvl);
                        while v.levels.len() > QUILL_CYCLE {
                            v.levels.pop_front();
                        }
                    }
                    // Asking "you moved to X, paste there?": follow the app in front, so going back
                    // to where it started pastes there and the question always names the real app.
                    if let State::NeedsYou(Problem::TargetChanged { now_in, .. }) = v.flow.state().clone() {
                        let fg = os::foreground();
                        if !fg.is_null() && fg as u64 != now_in.window && !os::is_own_window(fg) {
                            let app = os::app_name(fg);
                            os::cache_app_icon(fg, os::icon_owner(fg));
                            v.handle(Event::Focus { target: Target { app, window: fg as u64 } }, cx);
                        }
                    }
                    let offline = v.flow.readiness().network == Network::Down;
                    let every = if offline { Duration::from_secs(5) } else { recheck_every() };
                    let quiet = matches!(v.flow.state(), State::Idle | State::NeedsYou(_));
                    if !fake_mode() && v.last_check.elapsed() > every && quiet {
                        v.check_session(cx);
                    }
                    // Paused: the ink line and the timer hold still.
                    if matches!(v.flow.state(), State::Recording { paused: false, .. } | State::Transcribing { .. } | State::Delivering { .. }) {
                        v.frame += 1;
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        })
        .detach();

        let os_dark = theme::os_is_dark(cx.window_appearance());
        theme::apply(settings.appearance, os_dark);
        let mut me = Self {
            flow,
            history,
            settings,
            mic_name,
            mic_testing: false,
            job_entries: HashMap::new(),
            job_synced: HashMap::new(),
            trouble: HashMap::new(),
            hotkey_paused_at: None,
            strip_blank: false,
            os_dark,
            strip_preview: None,
            strip_window: None,
            job_secs: HashMap::new(),
            published: None,
            hotkey_result: None,
            hotkey_paused: false,
            job_context: Arc::new(std::sync::Mutex::new(HashMap::new())),
            signing_in: false,
            holding: None,
            menu_open: open_item.id().clone(),
            menu_quit: quit_item.id().clone(),
            rec_started: Instant::now(),
            talk: talk::TalkClock::default(),
            sending: std::collections::HashSet::new(),
            frame: 0,
            tray,
            hotkeys,
            main_key,
            esc_key,
            esc_bound: false,
            mic: built.ok(),
            mic_on: false,
            mic_revived: false,
            settings_ok,
            audio_paths: HashMap::new(),
            peaks: HashMap::new(),
            levels: VecDeque::new(),
            gpt: Arc::new(ChatGpt::new()),
            session: None,
            last_check: Instant::now(),
            notifier: notify::Notifier::new(),
            session_known: None,
            confirming: false,
        };
        if !fake_mode() {
            me.check_session(cx);
        }
        me
    }

    /// Check the ChatGPT session in the background and report readiness to the core.
    fn check_session(&mut self, cx: &mut Context<Self>) {
        self.last_check = Instant::now();
        let gpt = self.gpt.clone();
        let generation = session_gen();
        cx.spawn(async move |this, cx| {
            let (session, state, network, definite) = cx.background_executor().spawn(async move { ask_session(&gpt, generation) }).await;
            let _ = this.update(cx, |v, cx| {
                if session_gen() != generation {
                    os::log("session check outlived a sign-in or sign-out: ignored");
                    return;
                }
                os::log(&format!("session check: {state:?}, network {network:?}"));
                v.session = session;
                v.watch_expiry(state, definite, cx);
                let r = Readiness { session: state, network, ..v.flow.readiness() };
                if r != v.flow.readiness() {
                    v.handle(Event::Readiness(r), cx);
                }
            });
        })
        .detach();
    }

    /// D1: say "You're signed out" only for a definite answer (ChatGPT's session call returned no
    /// token, not Cloudflare, not a failed transcription), after the session was known good, and
    /// once a second check 2 minutes later says the same. Once per expiry episode.
    fn watch_expiry(&mut self, state: Session, definite: bool, cx: &mut Context<Self>) {
        match state {
            Session::Valid => {
                if self.session_known == Some(Session::Expired) {
                    // Signed back in after an expiry: the moment macOS may ask to notify.
                    self.notifier.ask_permission();
                }
                self.session_known = Some(Session::Valid);
                self.notifier.clear_signin();
            }
            Session::Expired if definite => {
                let was_valid = self.session_known == Some(Session::Valid);
                self.session_known = Some(Session::Expired);
                if was_valid && !self.confirming {
                    self.confirming = true;
                    let wait = confirm_after();
                    os::log(&format!("session ended: checking again in {} s before saying so", wait.as_secs()));
                    let gpt = self.gpt.clone();
                    let generation = session_gen();
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(wait).await;
                        let (_, again, _, definite) = cx.background_executor().spawn(async move { ask_session(&gpt, generation) }).await;
                        let _ = this.update(cx, |v, cx| {
                            v.confirming = false;
                            if session_gen() != generation {
                                return;
                            }
                            os::log(&format!("session check (confirming): {again:?}, definite {definite}"));
                            if again == Session::Expired && definite && v.session_known == Some(Session::Expired) {
                                let waiting = v.history.entries.iter().filter(|e| matches!(&e.status, Status::Waiting(r) if r == "Sign in to send")).count() as u32;
                                let looking = v.user_is_looking();
                                v.notifier.signed_out(waiting, looking);
                            } else if again == Session::Valid {
                                v.watch_expiry(again, definite, cx);
                            }
                        });
                    })
                    .detach();
                }
            }
            _ => {}
        }
    }

    /// The strip shows a live take, or Voxa's window is in front: a toast waits until then. A
    /// paused take, or a needs-you strip the user left up, does not count: they are elsewhere.
    fn user_is_looking(&self) -> bool {
        let strip = match self.flow.state() {
            State::Idle | State::NeedsYou(_) => false,
            State::Recording { paused, .. } => !paused,
            _ => true,
        };
        strip || os::is_own_window(os::foreground())
    }

    fn on_notification(&mut self, click: notify::Click, cx: &mut Context<Self>) {
        match click {
            notify::Click::SignIn => self.sign_in(cx),
            notify::Click::Open => self.open_home(Some(Page::History), cx),
            notify::Click::Copy(id) => {
                if let Some(e) = self.history.get(id).filter(|e| !e.text.is_empty()) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(e.text.clone()));
                    os::log("notification: dictation copied");
                }
                self.notifier.clear_dictations();
            }
            notify::Click::SendNow => {
                self.notifier.clear_dictations();
                // A dictation of this run that failed is retried by the flow; older ones are resent.
                let failed = self.job_entries.keys().copied().find(|j| self.flow.job(*j).is_some_and(|j| matches!(j.state, voxa_core::JobState::Failed(_))));
                if let Some(job) = failed {
                    self.handle(Event::Retry { job }, cx);
                }
                self.resend_all(cx);
            }
        }
    }

    /// D2 / D3: a dictation that wasn't on screen finished. Ready: it waits in History, ready to
    /// copy. Failed (not for sign-in, not "no speech"): it couldn't be sent.
    fn notify_background(&mut self, job: JobId) {
        use voxa_core::JobState;
        let Some(j) = self.flow.job(job) else { return };
        let Some(&id) = self.job_entries.get(&job) else { return };
        let looking = self.user_is_looking();
        match &j.state {
            JobState::Ready(t) if !t.trim().is_empty() => {
                let app = self.history.get(id).map(|e| e.app.clone()).unwrap_or_else(|| j.target.app.clone());
                self.notifier.ready(id, &app, looking);
            }
            // Never for Cloudflare blocking: that stays on the strip and the Home bar.
            JobState::Failed(e) if !e.needs_sign_in() && *e != TranscribeError::Server(204) && *e != TranscribeError::Blocked => self.notifier.not_sent(looking),
            _ => {}
        }
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        os::log(&format!("event {}", redact::event(&event)));
        // A dictation finishing while another (or none) is on the strip: written in the background.
        let background = match &event {
            Event::Transcribed { job, .. } => Some(*job).filter(|j| !matches!(self.flow.state(), State::Transcribing { job: on } if on == j)),
            _ => None,
        };
        if matches!(event, Event::Delivered { result: Delivery::Pasted, .. }) {
            // The first dictation worked: the other moment macOS may ask to notify (once).
            self.notifier.ask_permission();
        }
        let effects = self.flow.handle(event);
        os::log(&format!("  -> {:?} | effects {}", self.flow.state(), redact::effects(&effects)));
        for e in effects {
            self.run(e, cx);
        }
        self.sync_history();
        if let Some(job) = background {
            self.notify_background(job);
        }
        // A toast held back while the user was looking goes out once they aren't.
        let looking = self.user_is_looking();
        self.notifier.flush(looking);
        self.publish();
        self.sync_esc();
        self.sync_tray();
        cx.notify();
    }

    fn run(&mut self, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::ShowStrip => os::strip_visible(true),
            Effect::HideStrip { after_ms: 0 } => os::strip_visible(false),
            Effect::HideStrip { after_ms } => {
                let shown = self.flow.state().clone();
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(after_ms as u64)).await;
                    let _ = this.update(cx, |v, cx| {
                        if *v.flow.state() == shown {
                            v.handle(Event::Dismiss, cx);
                        }
                    });
                })
                .detach();
            }
            Effect::StartCapture { job } => {
                if self.mic_testing {
                    self.mic_test(false, cx);
                }
                self.job_entries.insert(job, now_ms());
                self.rec_started = Instant::now();
                self.talk.start(self.rec_started);
                self.frame = 0;
                self.levels.clear();
                if fake_mode() {
                    return;
                }
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
                let path = data_dir().join("audio").join(format!("{ts}-{}.wav", job.0));
                let started = match self.mic.as_mut() {
                    Some(m) => m.start(&path),
                    None => audio::Mic::new().and_then(|mut m| {
                        let r = m.start(&path);
                        self.mic = Some(m);
                        r
                    }),
                };
                match started {
                    Ok(()) => {
                        os::log(&format!("  recording to {}", path.display()));
                        self.audio_paths.insert(job, path);
                        self.mic_on = true;
                        self.mic_revived = false;
                    }
                    Err(e) => {
                        // The mic would not open: undo the recording and show the blocker instead.
                        os::log(&format!("  mic failed: {e:?}"));
                        let target = self.flow.job(job).map(|j| j.target.clone());
                        let mic = match e {
                            audio::MicError::NoDevice => Mic::Missing,
                            audio::MicError::Disk(_) => Mic::CantSave,
                            audio::MicError::Blocked(_) => Mic::Denied,
                        };
                        self.handle(Event::Cancel, cx);
                        let r = Readiness { mic, ..self.flow.readiness() };
                        self.handle(Event::Readiness(r), cx);
                        if let Some(target) = target {
                            self.handle(Event::Hotkey { target }, cx);
                        }
                    }
                }
            }
            Effect::StopCapture { job } => {
                self.job_secs.insert(job, self.talk.stop(Instant::now()).as_secs_f32());
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    self.mic_on = false;
                    let peak = m.finish();
                    os::log(&format!("  recording stopped, peak rms {peak:.4}"));
                    self.peaks.insert(job, peak);
                    if let Some(t) = m.trouble() {
                        os::log(&format!("  recording trouble: {t:?}"));
                        self.trouble.insert(job, t);
                    }
                }
            }
            Effect::PauseCapture { .. } => {
                self.talk.pause(Instant::now());
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    m.pause();
                    os::log(&format!("  mic paused after {:.1} s of talking", self.talk.total(Instant::now()).as_secs_f32()));
                }
            }
            Effect::ResumeCapture { .. } => {
                self.mic_revived = false;
                // The PC may have slept during the pause: the strip's graphics can be gone.
                self.rebuild_strip_if_woke(cx);
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    match m.resume() {
                        Ok(()) => {
                            // Counted from when the mic is really back (reopening can take
                            // half a second), for the talking time and the watchdog.
                            self.rec_started = Instant::now();
                            self.talk.resume(self.rec_started);
                            os::log("  mic resumed");
                        }
                        Err(e) => {
                            // Like an interruption: finish and send what was recorded.
                            os::log(&format!("  mic resume failed: {e:?}; sending what was recorded"));
                            self.handle(Event::Stop, cx);
                        }
                    }
                }
            }
            Effect::DiscardCapture { .. } => {
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    self.mic_on = false;
                    m.discard();
                }
            }
            Effect::Transcribe { job } if !fake_mode() => {
                // The mic gave nothing or the file couldn't be written: say so (never "no speech").
                if let Some(t) = self.trouble.remove(&job).filter(|_| self.peaks.get(&job).is_none_or(|p| *p < SILENCE)) {
                    let mic = match t {
                        audio::Trouble::Disk => Mic::CantSave,
                        audio::Trouble::MicStopped => Mic::Stopped,
                    };
                    let target = self.flow.job(job).map(|j| j.target.clone());
                    self.handle(Event::Cancel, cx);
                    let r = Readiness { mic, ..self.flow.readiness() };
                    self.handle(Event::Readiness(r), cx);
                    if let Some(target) = target {
                        self.handle(Event::Hotkey { target }, cx);
                    }
                    return;
                }
                if self.peaks.get(&job).is_some_and(|p| *p < SILENCE) {
                    os::log("  silence: not sending");
                    self.handle(Event::Transcribed { job, result: Ok(String::new()) }, cx);
                    return;
                }
                let path = self.audio_paths.get(&job).cloned();
                let peak = self.peaks.get(&job).copied().unwrap_or(1.0);
                let app = self.flow.job(job).map(|j| j.target.app.clone()).unwrap_or_default();
                let session = self.session.clone();
                let gpt = self.gpt.clone();
                let generation = session_gen();
                cx.spawn(async move |this, cx| {
                    let (result, refreshed) = cx
                        .background_executor()
                        .spawn(async move {
                            let Some(session) = session else { return (Err(TranscribeError::SignedOut), None) };
                            let Some(audio) = path.and_then(|p| prepare::audio_for_upload(&p)) else { return (Err(TranscribeError::Server(0)), None) };
                            // A dropped connection is common on flaky Wi-Fi: try twice more before
                            // calling it offline.
                            let mut result = gpt.transcribe(&session, audio.clone(), "recording.wav", "audio/wav");
                            // A big recording that timed out would only time out again (and block new
                            // dictations meanwhile): keep it for Retry instead.
                            let small = audio.len() < 2_000_000;
                            for wait in [1500u64, 4000] {
                                let again = match &result {
                                    Err(TranscribeError::Offline) => true,
                                    Err(TranscribeError::Timeout) => small,
                                    _ => false,
                                };
                                if !again {
                                    break;
                                }
                                os::log(&format!("  upload failed ({:?}); retrying in {wait} ms", result.as_ref().err()));
                                std::thread::sleep(Duration::from_millis(wait));
                                result = gpt.transcribe(&session, audio.clone(), "recording.wav", "audio/wav");
                            }
                            let sent = session.cookie_header.clone();
                            let kept = keep_refreshed_cookies(&gpt, session, generation);
                            let refreshed = (kept.cookie_header != sent).then_some((sent, kept));
                            (result.map(|t| prepare::finish_text(t, &data_dir(), &app)), refreshed)
                        })
                        .await;
                    if let Some((sent, kept)) = refreshed {
                        let _ = this.update(cx, |v, _| v.adopt_refreshed(generation, &sent, kept));
                    }
                    let result = match result {
                        Ok(t) if invented_for_silence(&t, peak) => {
                            os::log(&format!("  dropped {}: near-silent take (peak {peak:.4})", redact::text(&t)));
                            Ok(String::new())
                        }
                        other => other,
                    };
                    let _ = this.update(cx, |v, cx| v.handle(Event::Transcribed { job, result }, cx));
                })
                .detach();
            }
            Effect::Transcribe { job } => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(900)).await;
                    let result = stand_in::next_transcription();
                    let _ = this.update(cx, |v, cx| v.handle(Event::Transcribed { job, result }, cx));
                })
                .detach();
            }
            Effect::Paste { job, target, text } => {
                let mut fg = os::foreground();
                // Voxa's own window is never where text goes: finishing while looking at Voxa
                // means "put it where I started". Switch back and paste there.
                if os::is_own_window(fg) && fg as u64 != target.window {
                    os::log(&format!("  finished in Voxa's window; going back to {}", target.app));
                    os::switch_to(target.window as _);
                    fg = os::foreground();
                }
                let same_tab = !os::browser_name(fg).is_some() || os::app_name(fg) == target.app;
                let result = if fg as u64 == target.window && same_tab {
                    if os::paste_here(&text, self.settings.keep_on_clipboard) { Delivery::Pasted } else { Delivery::Failed }
                } else {
                    let app = os::app_name(fg);
                    os::cache_app_icon(fg, os::icon_owner(fg));
                    os::cache_site_icon(&app);
                    Delivery::TargetChanged(Target { app, window: fg as u64 })
                };
                self.handle(Event::Delivered { job, result }, cx);
            }
            Effect::PasteHere { job, text } => {
                let result = if os::paste_here(&text, self.settings.keep_on_clipboard) { Delivery::Pasted } else { Delivery::Failed };
                self.handle(Event::Delivered { job, result }, cx);
            }
            Effect::OpenSignIn if !fake_mode() => {
                let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("voxa-signin.exe")));
                let Some(exe) = exe.filter(|p| p.exists()) else {
                    os::log("FAIL sign-in window not found next to voxa-desktop.exe");
                    return;
                };
                if self.signing_in {
                    return;
                }
                os::log("sign-in: opening the sign-in window");
                self.signing_in = true;
                cx.spawn(async move |this, cx| {
                    let code = cx
                        .background_executor()
                        .spawn(async move { std::process::Command::new(exe).status().ok().and_then(|s| s.code()) })
                        .await;
                    let _ = this.update(cx, |v, cx| {
                        os::log(&format!("sign-in window closed (exit {code:?})"));
                        v.signing_in = false;
                        cx.notify();
                        // Signed in or not, re-check: the core resends a waiting dictation when the session is valid.
                        v.check_session(cx);
                    });
                })
                .detach();
            }
            Effect::OpenSignIn => {
                os::log("STAND-IN sign-in: pretending the user signs in");
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(1500)).await;
                    let _ = this.update(cx, |v, cx| {
                        let r = Readiness { session: Session::Valid, ..v.flow.readiness() };
                        v.handle(Event::Readiness(r), cx);
                    });
                })
                .detach();
            }
            Effect::OpenMicSettings => {
                os::open_mic_privacy();
            }
            Effect::OpenStorage => {
                self.handle(Event::Dismiss, cx);
                self.open_home(Some(Page::Settings), cx);
            }
            Effect::SaveJob { job } => {
                if let Some(j) = self.flow.job(job) {
                    os::log(&format!("  job {:?} for {} -> {}", j.id, j.target.app, redact::job_state(&j.state)));
                }
            }
        }
    }

    /// Mirror this run's jobs into the saved history.
    fn sync_history(&mut self) {
        use voxa_core::JobState;
        let mut changed = false;
        let jobs: Vec<voxa_core::Job> = self.flow.jobs().cloned().collect();
        for job in jobs {
            let Some(&id) = self.job_entries.get(&job.id) else { continue };
            if self.job_synced.get(&job.id) == Some(&job.state) {
                continue;
            }
            let secs = self.job_secs.get(&job.id).copied().unwrap_or(0.0);
            // Name what was really in front (site, Claude Code) unless the user pasted elsewhere.
            let context = if job.started_in.is_none() { self.job_context.lock().unwrap().get(&job.id).cloned() } else { None };
            let (app, host) = match context {
                Some((name, host)) => (name, Some(host)),
                None => (job.target.app.clone(), None),
            };
            let entry = |text: String, status: Status| Entry {
                id,
                app: app.clone(),
                text,
                secs,
                status,
                audio: self.audio_paths.get(&job.id).cloned(),
                started_in: job.started_in.as_ref().map(|t| t.app.clone()),
                // The browser, when the app is a site; kept once known (the window may close later).
                // After a move, only where it was actually pasted counts (no earlier guess).
                via: if job.started_in.is_some() {
                    os::browser_name(job.target.window as _).filter(|b| *b != app)
                } else {
                    host.clone()
                        .or_else(|| os::browser_name(job.target.window as _))
                        .filter(|b| *b != app)
                        .or_else(|| self.history.get(id).and_then(|e| e.via.clone()))
                },
            };
            let next = match &job.state {
                JobState::Recording | JobState::Transcribing => continue,
                JobState::WaitingForNetwork => Some(entry(String::new(), Status::Waiting("Sends when you're back online".into()))),
                JobState::Ready(t) | JobState::Delivered(t) if t.trim().is_empty() => None,
                JobState::Ready(t) => Some(entry(t.clone(), Status::Ready)),
                JobState::Delivered(t) => Some(entry(t.clone(), Status::Pasted)),
                JobState::Failed(e) if e.needs_sign_in() => Some(entry(String::new(), Status::Waiting("Sign in to send".into()))),
                JobState::Failed(_) => Some(entry(String::new(), Status::Failed("Couldn't reach ChatGPT".into()))),
                JobState::Discarded => None,
            };
            changed |= match next {
                Some(e) => self.history.upsert(e),
                None => self.history.remove(id),
            };
            self.job_synced.insert(job.id, job.state.clone());
        }
        if changed {
            self.history.save();
        }
    }

    /// Quitting mid-take (a paused take can sit for hours): finish the recording file and keep
    /// the take in History as "Not sent", ready for Send now, instead of losing it.
    fn keep_unfinished_takes(&mut self, cx: &mut Context<Self>) {
        use voxa_core::JobState;
        if matches!(self.flow.state(), State::Recording { .. }) {
            if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                self.mic_on = false;
                m.finish();
            }
        }
        let jobs: Vec<voxa_core::Job> = self.flow.jobs().cloned().collect();
        let mut kept = 0;
        for job in jobs {
            if !matches!(job.state, JobState::Recording | JobState::Transcribing) {
                continue;
            }
            let (Some(&id), Some(audio)) = (self.job_entries.get(&job.id), self.audio_paths.get(&job.id).cloned()) else { continue };
            let secs = self.job_secs.get(&job.id).copied().unwrap_or_else(|| self.talk.total(Instant::now()).as_secs_f32());
            self.history.upsert(Entry {
                id,
                app: job.target.app.clone(),
                text: String::new(),
                secs,
                status: Status::Waiting("Not sent".into()),
                audio: Some(audio),
                started_in: job.started_in.as_ref().map(|t| t.app.clone()),
                via: None,
            });
            kept += 1;
        }
        if kept > 0 {
            self.history.save();
            os::log(&format!("quit mid-take: {kept} kept in History as not sent"));
        }
        cx.notify();
    }

    /// Whether a history entry belongs to a dictation the core is still handling.
    pub(crate) fn is_live(&self, id: u64) -> bool {
        use voxa_core::JobState;
        self.job_entries.iter().any(|(job, e)| *e == id && self.flow.job(*job).is_some_and(|j| !matches!(j.state, JobState::Delivered(_) | JobState::Discarded)))
    }

    /// Open the Voxa window (or bring it forward), optionally on a page.
    /// Open the Voxa window: a separate process, so its memory is returned when it closes.
    /// If it is already open, it comes to the front.
    pub(crate) fn open_home(&mut self, page: Option<Page>, _cx: &mut Context<Self>) {
        self.publish();
        let Ok(exe) = std::env::current_exe() else { return };
        let mut c = std::process::Command::new(exe);
        c.arg("--window");
        if page == Some(Page::Settings) {
            c.arg("--page=settings");
        }
        if page == Some(Page::History) {
            c.arg("--page=history");
        }
        // The user is looking at Voxa now: its dictations notification has done its job.
        self.notifier.clear_dictations();
        if page.is_none() && !self.settings.onboarded {
            c.arg("--tour");
        }
        match c.spawn() {
            Ok(_) => os::log("window process started"),
            Err(e) => os::log(&format!("FAIL window process: {e}")),
        }
    }

    /// Tell the window process what it can't see in the files: sign-in, network, mic, live jobs.
    fn publish(&mut self) {
        let (session, online, mic) = link::Live::from_readiness(self.flow.readiness());
        let live_ids = self.job_entries.values().copied().filter(|id| self.is_live(*id)).collect();
        let state = match self.flow.state() {
            State::Recording { paused: true, .. } => "paused",
            State::Recording { .. } => "recording",
            State::Transcribing { .. } | State::Delivering { .. } => "writing",
            State::Delivered { .. } => "pasted",
            State::NeedsYou(_) => "needs",
            State::Blocked(_) => "blocked",
            State::Idle => "idle",
        }
        .to_string();
        let live = link::Live { session, online, mic, mic_name: self.mic_name.clone(), signing_in: self.signing_in, live_ids, hotkey_result: self.hotkey_result.clone(), state };
        if self.published.as_ref() != Some(&live) {
            link::write_live(&live);
            self.published = Some(live);
        }
    }

    fn on_command(&mut self, cmd: link::Command, cx: &mut Context<Self>) {
        use link::Command::*;
        os::log(&format!("window asks: {}", match &cmd {
            SetHotkey(_) => "SetHotkey".into(),
            EditEntry(id, text) => format!("EditEntry({id}, {})", redact::text(text)),
            c => format!("{c:?}"),
        }));
        match cmd {
            SignIn => self.sign_in(cx),
            SignOut => self.sign_out(cx),
            SetHotkey(s) => {
                let r = self.set_hotkey(&s, cx);
                self.hotkey_result = Some((s, r.err()));
            }
            PauseHotkey(p) => self.pause_hotkey(p),
            ReloadSettings => {
                match Settings::try_load(&data_dir()) {
                    Ok(s) => {
                        self.settings = s.unwrap_or_default();
                        self.settings_ok = true;
                    }
                    Err(e) => {
                        os::log(&format!("FAIL reading settings ({e}): keeping the current ones"));
                        self.publish();
                        return;
                    }
                }
                if let Some(m) = self.mic.as_mut() {
                    m.set_preferred(self.settings.mic_device.clone());
                }
                // Appearance may have changed in the window: the strip follows it straight away.
                self.os_dark = theme::os_is_dark(cx.window_appearance());
                if theme::apply(self.settings.appearance, self.os_dark) {
                    cx.notify();
                }
                self.mic_name = self.settings.mic_device.clone().unwrap_or_else(|| audio::default_device_name().unwrap_or_else(|| "No microphone".into()));
                if self.history.prune(self.settings.keep_days, now_ms()) > 0 {
                    self.history.save();
                }
                if self.history.prune_audio(self.settings.keep_audio_days, now_ms()) > 0 {
                    self.history.save();
                }
            }
            Resend(id) => match self.job_entries.iter().find(|(_, e)| **e == id).map(|(j, _)| *j) {
                Some(job) => self.handle(Event::Retry { job }, cx),
                None => self.resend(id, cx),
            },
            ResendAll => self.resend_all(cx),
            DeleteEntry(id) => self.delete_entry(id, cx),
            EditEntry(id, text) => {
                if let Some(e) = self.history.get(id).cloned() {
                    self.history.upsert(Entry { text, ..e });
                    self.history.save();
                }
            }
            DeleteAll => self.delete_history(cx),
            DeleteOldAudio(days) => {
                let freed = self.history.prune_audio(days.max(1), now_ms());
                self.history.save();
                os::log(&format!("deleted recordings older than {days} days ({:.1} MB)", freed as f64 / 1e6));
                cx.notify();
            }
        }
        self.publish();
    }

    pub(crate) fn sign_in(&mut self, cx: &mut Context<Self>) {
        {
            let _g = lock_session_file();
            // Nothing refreshed for the session before this sign-in may be saved after it.
            SESSION_GEN.fetch_add(1, Ordering::SeqCst);
            self.gpt.clear_refreshed();
            let _ = std::fs::remove_file(data_dir().join("signed-out"));
        }
        self.run(Effect::OpenSignIn, cx);
    }

    /// An upload or check refreshed the sign-in: use it in memory too, unless the user signed
    /// out or in since it started, or the session changed meanwhile.
    fn adopt_refreshed(&mut self, generation: u64, sent: &str, refreshed: GptSession) {
        if session_gen() == generation && self.session.as_ref().is_some_and(|s| s.cookie_header == sent) {
            self.session = Some(refreshed);
        }
    }

    pub(crate) fn sign_out(&mut self, cx: &mut Context<Self>) {
        let dir = data_dir();
        let g = lock_session_file();
        // Checks and uploads still running must never write this session back.
        SESSION_GEN.fetch_add(1, Ordering::SeqCst);
        self.gpt.clear_refreshed();
        let _ = std::fs::remove_file(dir.join("session.json"));
        // The sign-in window's cookies too, so the next sign-in really asks.
        let _ = std::fs::remove_dir_all(dir.join("webview"));
        let _ = std::fs::write(dir.join("signed-out"), b"");
        drop(g);
        let _ = std::fs::remove_file(dir.join("account.json"));
        let _ = std::fs::remove_file(dir.join("avatar.png"));
        self.session = None;
        // On purpose: never a "signed out" toast, and any showing one goes.
        self.session_known = None;
        self.notifier.clear_signin();
        os::log("signed out");
        let r = Readiness { session: Session::Expired, ..self.flow.readiness() };
        self.handle(Event::Readiness(r), cx);
    }

    /// Stop listening for the shortcut while the user picks a new one.
    pub(crate) fn pause_hotkey(&mut self, pause: bool) {
        // The window repeats "pause" while it waits for the new keys; if it stops (closed or
        // crashed mid-pick), the shortcut comes back by itself.
        self.hotkey_paused_at = if pause { Some(Instant::now()) } else { None };
        if pause == self.hotkey_paused {
            return;
        }
        let r = if pause { self.hotkeys.unregister(self.main_key) } else { self.hotkeys.register(self.main_key) };
        os::log(&format!("shortcut {} ({r:?})", if pause { "paused" } else { "resumed" }));
        self.hotkey_paused = pause;
    }

    pub(crate) fn set_hotkey(&mut self, s: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let new = s.parse::<HotKey>().map_err(|_| format!("{s} can't be used as a shortcut"))?;
        if new.id() == self.main_key.id() {
            return Ok(());
        }
        let _ = self.hotkeys.unregister(self.main_key);
        if let Err(e) = self.hotkeys.register(new) {
            os::log(&format!("shortcut {s} refused: {e}"));
            let _ = self.hotkeys.register(self.main_key);
            return Err(format!("{s} is already used by another app. Try a different one."));
        }
        os::log(&format!("shortcut changed to {s}"));
        self.main_key = new;
        self.update_settings(|st| st.hotkey = s.to_string(), cx);
        Ok(())
    }

    pub(crate) fn update_settings(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        f(&mut self.settings);
        self.settings.save(&data_dir());
        if self.history.prune(self.settings.keep_days, now_ms()) > 0 {
            self.history.save();
        }
        cx.notify();
    }

    pub(crate) fn delete_history(&mut self, cx: &mut Context<Self>) {
        // Dictations still in progress (recording, writing, waiting to send) are kept: deleting
        // their audio would break them. Everything else goes.
        let live: Vec<u64> = self.history.entries.iter().map(|e| e.id).filter(|id| self.is_live(*id)).collect();
        self.history.delete_all_except(&live);
        self.job_entries.retain(|_, id| live.contains(id));
        os::log(&format!("history deleted (kept {} in progress)", live.len()));
        cx.notify();
    }

    /// Send a kept recording that wasn't written out (from an earlier run). The text is kept in
    /// History, ready to copy; it is never pasted into whatever app happens to be open now.
    pub(crate) fn resend(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.history.get(id).cloned() else { return };
        let Some(audio) = entry.audio.clone().filter(|a| a.exists()) else { return };
        let Some(session) = self.session.clone() else {
            self.sign_in(cx);
            return;
        };
        if !self.sending.insert(id) {
            os::log(&format!("resend {id}: already being sent"));
            return;
        }
        self.history.upsert(Entry { status: Status::Waiting("Writing it out".into()), ..entry.clone() });
        cx.notify();
        let gpt = self.gpt.clone();
        let app = entry.app.clone();
        let generation = session_gen();
        cx.spawn(async move |this, cx| {
            let (result, refreshed) = cx
                .background_executor()
                .spawn(async move {
                    let Some(bytes) = prepare::audio_for_upload(&audio) else { return (Err(TranscribeError::Server(0)), None) };
                    let (name, mime) = if audio.extension().is_some_and(|e| e == "webm") { ("recording.webm", "audio/webm") } else { ("recording.wav", "audio/wav") };
                    let r = gpt.transcribe(&session, bytes, name, mime);
                    let sent = session.cookie_header.clone();
                    let kept = keep_refreshed_cookies(&gpt, session, generation);
                    let refreshed = (kept.cookie_header != sent).then_some((sent, kept));
                    (r.map(|t| prepare::finish_text(t, &data_dir(), &app)), refreshed)
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.sending.remove(&id);
                if let Some((sent, kept)) = refreshed {
                    v.adopt_refreshed(generation, &sent, kept);
                }
                os::log(&format!("resend {id}: {}", if result.is_ok() { "ok" } else { "failed" }));
                let next = match result {
                    Ok(t) if !t.trim().is_empty() => Entry { text: t, status: Status::Ready, ..entry },
                    Ok(_) => Entry { status: Status::Failed("No speech in the recording".into()), ..entry },
                    Err(e) if e.needs_sign_in() => Entry { status: Status::Waiting("Sign in to send".into()), ..entry },
                    Err(_) => Entry { status: Status::Failed("Couldn't reach ChatGPT".into()), ..entry },
                };
                // Deleted while it was being sent: leave it deleted.
                if v.history.get(id).is_none() {
                    return;
                }
                v.history.upsert(next);
                v.history.save();
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn delete_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(a) = self.history.get(id).and_then(|e| e.audio.clone()) {
            let _ = std::fs::remove_file(a);
        }
        if self.history.remove(id) {
            self.history.save();
            os::log("history entry deleted");
        }
        // Forget it for good: nothing from this run may write it back.
        self.job_entries.retain(|_, e| *e != id);
        cx.notify();
    }

    pub(crate) fn resend_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .history
            .entries
            .iter()
            .filter(|e| matches!(e.status, Status::Waiting(_) | Status::Failed(_)) && e.audio.as_ref().is_some_and(|a| a.exists()) && !self.is_live(e.id) && !self.sending.contains(&e.id))
            .map(|e| e.id)
            .collect();
        for id in ids {
            self.resend(id, cx);
        }
    }

    /// Open the mic just to show the level (setup step 2). Nothing is kept.
    pub(crate) fn mic_test(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.mic_testing || fake_mode() {
            return;
        }
        if on {
            if !matches!(self.flow.state(), State::Idle) {
                return;
            }
            let path = std::env::temp_dir().join("voxa-mic-test.wav");
            if let Some(m) = self.mic.as_mut() {
                if m.start(&path).is_ok() {
                    self.mic_testing = true;
                    self.mic_on = true;
                    self.levels.clear();
                    os::log("mic test on");
                }
            }
        } else {
            if let Some(m) = self.mic.as_mut() {
                m.discard();
            }
            self.mic_on = false;
            self.mic_testing = false;
            self.levels.clear();
            let _ = std::fs::remove_file(std::env::temp_dir().join("voxa-mic-test.wav"));
            os::log("mic test off");
        }
        cx.notify();
    }

    fn paused(&self) -> bool {
        matches!(self.flow.state(), State::Recording { paused: true, .. })
    }

    /// Talking time in this take: paused time is left out.
    fn talk_time(&self) -> Duration {
        self.talk.total(Instant::now())
    }

    /// After sleep, a charger or a display change the strip's graphics may be gone: build a fresh
    /// one now, hidden and without taking focus; it replaces the old one as this dictation's strip.
    fn rebuild_strip_if_woke(&mut self, cx: &mut Context<Self>) {
        if os::take_woke() {
            let me = cx.entity();
            let old = self.strip_window;
            cx.spawn(async move |_, cx| {
                match rebuild_strip(&me, old, cx).await {
                    Some(h) => {
                        let _ = me.update(cx, |v, _| v.strip_window = Some(h));
                    }
                    None => os::log("FAIL rebuilding the strip"),
                }
            })
            .detach();
        }
    }

    /// Esc is bound only while recording, so Voxa never steals Esc from other apps. Not while
    /// paused: the user is working elsewhere then, and Esc would throw the whole take away.
    fn sync_esc(&mut self) {
        let want = matches!(self.flow.state(), State::Recording { paused: false, .. });
        if want != self.esc_bound {
            let r = if want { self.hotkeys.register(self.esc_key) } else { self.hotkeys.unregister(self.esc_key) };
            self.esc_bound = want && r.is_ok();
            os::log(&format!("Esc {} ({r:?})", if want { "bound" } else { "released" }));
        }
    }

    fn sync_tray(&self) {
        let dot = match self.flow.state() {
            State::Recording { paused: true, .. } => Some([0x8A, 0x8A, 0x8A]),
            State::Recording { .. } => Some([0xE5, 0x48, 0x3D]),
            State::Blocked(_) | State::NeedsYou(_) => Some([0xC9, 0x8A, 0x16]),
            _ if self.flow.readiness().blocker().is_some() => Some([0xC9, 0x8A, 0x16]),
            _ => None,
        };
        if let Some(t) = &self.tray {
            let _ = t.set_icon(Some(tray_icon(dot)));
        }
    }
}

fn tray_icon(dot: Option<[u8; 3]>) -> Icon {
    // The app icon (32x32 RGBA, from assets/make_icon.py), with a status dot when Voxa needs you.
    let mut px_ = include_bytes!("../assets/tray.rgba").to_vec();
    if let Some(c) = dot {
        for y in 0..32i32 {
            for x in 0..32i32 {
                let (dx, dy) = (x - 25, y - 25);
                let d2 = dx * dx + dy * dy;
                let i = ((y * 32 + x) * 4) as usize;
                if d2 <= 36 {
                    px_[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                } else if d2 <= 64 {
                    px_[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }
    Icon::from_rgba(px_, 32, 32).expect("icon")
}

// ── view ──────────────────────────────────────────────────────────────────

fn button(id: &'static str, label: impl Into<gpui::SharedString>, primary: bool) -> gpui::Stateful<gpui::Div> {
    let t = theme::p();
    let (bg, fg) = if primary { (t.accent, t.on_accent) } else { (t.card, t.text) };
    div()
        .id(id)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .border_color(rgba(t.line_input_a))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .cursor_pointer()
        .child(label.into())
}

/// Leading glyph: a different shape per kind of state, never colour alone.
fn glyph(state: &State, frame: u64) -> AnyElement {
    let t = theme::p();
    let (kind, color) = match state {
        State::Recording { paused: true, .. } => ("pause", t.strip_paused),
        State::Recording { .. } => ("dot", t.strip_rec),
        State::Transcribing { .. } | State::Delivering { .. } => ("spin", t.accent),
        State::Delivered { .. } => ("check", t.good),
        State::NeedsYou(Problem::NoSpeech { .. }) => ("info", t.strip_muted),
        State::NeedsYou(Problem::Unreachable { .. }) | State::Blocked(_) => ("alert", t.error),
        _ => ("alert", t.warn_text),
    };
    let color: Hsla = rgb(color).into();
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            match kind {
                "dot" => {
                    let mut p = PathBuilder::fill();
                    for i in 0..=24 {
                        let a = i as f32 / 24. * std::f32::consts::TAU;
                        let q = point(c.x + px(5. * a.cos()), c.y + px(5. * a.sin()));
                        if i == 0 { p.move_to(q) } else { p.line_to(q) }
                    }
                    if let Ok(p) = p.build() { window.paint_path(p, color) }
                }
                "pause" => {
                    for dx in [-3.5f32, 1.5] {
                        let mut p = PathBuilder::fill();
                        let (x, y) = (c.x + px(dx), c.y - px(5.));
                        p.move_to(point(x, y));
                        p.line_to(point(x + px(2.5), y));
                        p.line_to(point(x + px(2.5), y + px(10.)));
                        p.line_to(point(x, y + px(10.)));
                        p.close();
                        if let Ok(p) = p.build() { window.paint_path(p, color) }
                    }
                }
                "spin" => {
                    let mut p = PathBuilder::stroke(px(2.));
                    let start = frame as f32 * 0.25;
                    for i in 0..=18 {
                        let a = start + i as f32 / 18. * 4.2;
                        let q = point(c.x + px(7. * a.cos()), c.y + px(7. * a.sin()));
                        if i == 0 { p.move_to(q) } else { p.line_to(q) }
                    }
                    if let Ok(p) = p.build() { window.paint_path(p, color) }
                }
                "check" => {
                    let mut p = PathBuilder::stroke(px(2.));
                    p.move_to(point(c.x - px(6.), c.y));
                    p.line_to(point(c.x - px(2.), c.y + px(4.)));
                    p.line_to(point(c.x + px(6.), c.y - px(5.)));
                    if let Ok(p) = p.build() { window.paint_path(p, color) }
                }
                _ => {
                    // circle outline with a bar (alert) or a dot (info)
                    let mut p = PathBuilder::stroke(px(1.6));
                    for i in 0..=28 {
                        let a = i as f32 / 28. * std::f32::consts::TAU;
                        let q = point(c.x + px(8. * a.cos()), c.y + px(8. * a.sin()));
                        if i == 0 { p.move_to(q) } else { p.line_to(q) }
                    }
                    if let Ok(p) = p.build() { window.paint_path(p, color) }
                    let mut m = PathBuilder::stroke(px(1.8));
                    if kind == "info" {
                        m.move_to(point(c.x, c.y - px(1.)));
                        m.line_to(point(c.x, c.y + px(4.)));
                    } else {
                        m.move_to(point(c.x, c.y - px(4.)));
                        m.line_to(point(c.x, c.y + px(1.)));
                    }
                    if let Ok(m) = m.build() { window.paint_path(m, color) }
                }
            }
        },
    )
    .size(px(24.))
    .into_any_element()
}

/// Ticks per quill pass: 100 x 40 ms = 4 s, the old overlay's rhythm.
pub(crate) const QUILL_CYCLE: usize = 100;

/// The old overlay's quill (viewBox 0 0 16 16), as cubic/line segments. Nib tip is at (2, 15.6).
const QUILL_NIB: (f32, f32) = (3.0, 15.2);

fn quill_point(x: f32, y: f32, scale: f32, angle: f32, at: gpui::Point<gpui::Pixels>) -> gpui::Point<gpui::Pixels> {
    let (dx, dy) = ((x - QUILL_NIB.0) * scale, (y - QUILL_NIB.1) * scale);
    let (s, c) = angle.sin_cos();
    point(at.x + px(dx * c - dy * s), at.y + px(dx * s + dy * c))
}

/// The writing quill: glides across every 4 s, leaving an ink line whose waves follow the voice.
/// `levels` are this pass's smoothed mic levels (0..1), one per 40 ms tick; `tick` is the position
/// in the pass.
pub(crate) fn ink_line(levels: Vec<f32>, tick: usize) -> AnyElement {
    ink_line_faded(levels, tick, true)
}

/// `fade`: fade in and out with each 4 s pass (the strip); off for still uses in the window.
pub(crate) fn ink_line_faded(levels: Vec<f32>, tick: usize, fade: bool) -> AnyElement {
    let t = theme::p();
    let ink: Hsla = rgb(t.accent).into();
    let pen: Hsla = rgb(t.pen).into();
    // The feather's spine: a light line on the dark quill, a dark one on the light quill.
    let spine_hsla = if t.dark { gpui::hsla(0.08, 0.25, 0.45, 0.6) } else { gpui::hsla(0.08, 0.3, 0.75, 0.55) };
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let w = f32::from(b.size.width);
            let base = b.origin.y + px(f32::from(b.size.height) * 0.62);
            let n = QUILL_CYCLE as f32;
            // Seismograph: no voice, flat line; a reading only when there is sound. Room hum below
            // the gate stays flat; above it, quiet speech is lifted so it still shows.
            const GATE: f32 = 0.14;
            let amp = |lvl: f32| if lvl < GATE { 0.0 } else { ((lvl - GATE) / (1.0 - GATE)).min(1.0).powf(0.6) * 11.0 };
            let at = |k: usize| {
                let l = levels.get(k).copied().unwrap_or(0.0);
                // Alternate up and down with a fixed per-tick jitter, like a needle trace.
                let jitter = 0.55 + 0.45 * (((k * 7919) % 97) as f32 / 96.0);
                let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
                (b.origin.x + px(k as f32 / n * w), base - px(dir * jitter * amp(l)))
            };

            let p_now = tick as f32 / n;
            // Fade in at the start of a pass and out at its end, like the old overlay.
            let alpha = if fade { (p_now / 0.06).min(1.0).min((1.0 - p_now) / 0.08).clamp(0.0, 1.0) } else { 1.0 };

            // Ink trail, from the left edge up to the nib.
            if tick >= 1 {
                let mut path = PathBuilder::stroke(px(1.6));
                let (x0, y0) = at(0);
                path.move_to(point(x0, y0));
                for k in 1..=tick.min(levels.len().saturating_sub(1)) {
                    let (x, y) = at(k);
                    path.line_to(point(x, y));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, ink.opacity(0.9 * alpha));
                }
            }

            // The quill, nib on the head of the line; it tilts with the stroke (-15 deg up, -9 down)
            // and leans a little further when you speak up.
            let (hx, hy) = at(tick.min(levels.len().saturating_sub(1)));
            let lvl = levels.get(tick).copied().unwrap_or(0.0);
            let deg = -12.0 - 5.0 * lvl;
            let (sc, ang, head) = (1.45, deg.to_radians(), point(hx, hy));
            let q = |x: f32, y: f32| quill_point(x, y, sc, ang, head);
            let mut body = PathBuilder::fill();
            body.move_to(q(12.5, 1.5));
            body.cubic_bezier_to(q(5.5, 13.0), q(9.0, 4.0), q(6.5, 8.0));
            body.line_to(q(4.5, 14.5));
            body.line_to(q(4.0, 15.0));
            body.cubic_bezier_to(q(9.5, 11.0), q(6.0, 14.5), q(8.0, 13.0));
            body.cubic_bezier_to(q(12.5, 1.5), q(11.5, 8.5), q(13.0, 5.0));
            body.close();
            if let Ok(path) = body.build() {
                window.paint_path(path, pen.opacity(alpha));
            }
            let mut nib = PathBuilder::fill();
            nib.move_to(q(4.0, 15.0));
            nib.line_to(q(2.0, 15.6));
            nib.line_to(q(3.0, 14.0));
            nib.close();
            if let Ok(path) = nib.build() {
                window.paint_path(path, pen.opacity(alpha));
            }
            // Spine highlight down the feather.
            let mut spine = PathBuilder::stroke(px(0.7));
            spine.move_to(q(12.2, 2.0));
            spine.cubic_bezier_to(q(5.5, 13.0), q(9.5, 4.5), q(7.0, 8.5));
            if let Ok(path) = spine.build() {
                window.paint_path(path, spine_hsla.opacity(alpha));
            }
        },
    )
    .w(px(200.))
    .h(px(36.))
    .into_any_element()
}

impl Render for Voxa {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.strip_blank {
            return div().into_any_element();
        }
        self.strip_view(w, cx).into_any_element()
    }
}

impl Voxa {
    fn strip_view(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let flow = self.strip_preview.as_ref().unwrap_or(&self.flow);
        let state = flow.state().clone();
        let th = theme::p();
        // The brand mark keeps the 1.0 sepia with white in both modes, like the app icon.
        let mark = div()
            .size(px(24.))
            .rounded(px(6.))
            .bg(rgb(INK))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0xFFFFFF))
            .text_size(px(13.))
            .child("v");

        let mut row: Vec<AnyElement> = vec![mark.into_any_element(), glyph(&state, self.frame)];
        let text = strip_text(flow);

        match &state {
            State::Recording { job, paused, .. } => {
                let paused = *paused;
                let app = flow.job(*job).map(|j| j.target.app.clone()).unwrap_or_default();
                let secs = self.talk_time().as_secs();
                row.push(div().text_size(px(12.)).text_color(rgb(th.strip_muted)).child(app).into_any_element());
                // This pass's levels: the last (tick + 1) ticks.
                let tick = self.frame as usize % QUILL_CYCLE;
                let pass: Vec<f32> = self.levels.iter().skip(self.levels.len().saturating_sub(tick + 1)).copied().collect();
                if paused {
                    // Frozen where it stopped, and faded: nothing is being heard.
                    row.push(div().opacity(0.35).child(ink_line_faded(pass, tick, false)).into_any_element());
                } else {
                    row.push(ink_line(pass, tick));
                }
                row.push(div().w(px(36.)).child(format!("{}:{:02}", secs / 60, secs % 60)).into_any_element());
                if let Some(t) = &text {
                    if paused {
                        let mut col = div().flex().flex_col().text_size(px(12.)).child(div().child(t.title.clone()));
                        if let Some(d) = t.detail.clone() {
                            col = col.child(div().text_color(rgb(th.strip_muted)).child(d));
                        }
                        row.push(col.into_any_element());
                    } else if t.title != "Listening" {
                        row.push(div().text_size(px(12.)).text_color(rgb(th.warn_text)).child(t.title.clone()).into_any_element());
                    }
                }
                row.push(
                    button("cancel", "Cancel", false)
                        .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Cancel, cx)))
                        .into_any_element(),
                );
                let (id, label, ev) = if paused {
                    ("resume", voxa_core::copy::RESUME_LABEL, Event::Resume)
                } else {
                    ("pause", voxa_core::copy::PAUSE_LABEL, Event::Pause)
                };
                row.push(
                    button(id, label, false)
                        .on_click(cx.listener(move |v, _: &ClickEvent, _, cx| v.handle(ev.clone(), cx)))
                        .into_any_element(),
                );
                row.push(
                    button("stop", "Stop", true)
                        .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Stop, cx)))
                        .into_any_element(),
                );
            }
            _ => {
                if let Some(t) = text {
                    let mut col = div().flex().flex_col().child(div().child(t.title.clone()));
                    if let Some(d) = t.detail.clone() {
                        col = col.child(div().text_size(px(12.)).text_color(rgb(th.strip_muted)).child(d));
                    }
                    row.push(col.into_any_element());
                    if matches!(state, State::Transcribing { .. }) {
                        row.push(
                            button("cancel", "Cancel", false)
                                .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Cancel, cx)))
                                .into_any_element(),
                        );
                    }
                    if let Some(a) = t.action.clone() {
                        row.push(
                            button("action", a, true)
                                .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Action, cx)))
                                .into_any_element(),
                        );
                    }
                    if t.dismissable {
                        row.push(
                            button("close", "✕", false)
                                .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Dismiss, cx)))
                                .into_any_element(),
                        );
                    }
                }
            }
        }

        div().size_full().flex().items_center().justify_center().child(
            div()
                .id("strip")
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(52.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(rgba(th.strip_a))
                .border_1()
                .border_color(rgba(th.line_input_a))
                .shadow_lg()
                .font_family("Segoe UI")
                .text_size(px(14.))
                .text_color(rgb(th.text))
                .children(row),
        )
    }
}

/// `voxa-desktop --window`: just the Voxa window. Exits when the window closes, returning its
/// memory; the tray process keeps running.
/// The display face, embedded once for both processes.
static FRAUNCES: &[u8] = include_bytes!("../assets/Fraunces.ttf");

fn run_window(tour: bool) {
    if !os::single_window_instance() {
        return; // already open: it was asked to come to the front
    }
    Application::new().run(move |cx: &mut App| {
        os::show_in_dock();
        input::bind_keys(cx);
        let _ = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(FRAUNCES)]);
        let backend = cx.new(Backend::new);
        let start_tour = tour || !backend.read(cx).settings.onboarded;
        let mut first = Page::Home;
        // Developer switch: VOXA_PAGE=words | history | insights | settings | detail (newest dictation).
        // The tray asks for a page with --page=<name> (e.g. "Free up space" opens Settings).
        let asked = std::env::args().find_map(|a| a.strip_prefix("--page=").map(str::to_string));
        if let Some(p) = asked.or_else(|| std::env::var("VOXA_PAGE").ok()) {
            first = match p.as_str() {
                "words" => Page::Words,
                "history" => Page::History,
                "insights" => Page::Insights,
                "settings" => Page::Settings,
                "detail" => backend.read(cx).history.entries.last().map(|e| Page::Detail(e.id)).unwrap_or(Page::History),
                _ => first,
            };
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some("Voxa".into()), ..Default::default() }),
            window_bounds: Some(WindowBounds::centered(size(px(1100.), px(760.)), cx)),
            window_min_size: Some(size(px(760.), px(520.))),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            ..Default::default()
        };
        let opened = cx.open_window(options, move |window, cx| {
            if let Some(w) = os::window_ptr(window) {
                os::bring_to_front(w);
            }
            cx.new(|cx| Home::new(backend, first, window, cx))
        });
        match opened {
            Ok(h) => {
                os::log(&format!("window opened on {first:?}"));
                if let Some(n) = std::env::var("VOXA_TOUR").ok().and_then(|n| n.parse().ok()) {
                    let _ = h.update(cx, |home, _, cx| home.start_tour_at(n, cx));
                } else if start_tour && std::env::var_os("VOXA_PAGE").is_none() {
                    let _ = h.update(cx, |home, _, cx| home.start_tour(cx));
                }
                // Another "Open Voxa" while open: come to the front.
                cx.spawn(async move |cx| loop {
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    if os::OPEN_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        let r = h.update(cx, |_, window, _| {
                            window.activate_window();
                            if let Some(w) = os::window_ptr(window) {
                                os::bring_to_front(w);
                            }
                        });
                        if r.is_err() {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(e) => os::log(&format!("FAIL open window: {e}")),
        }
    });
}

/// Test only (VOXA_TEST_STRIP=recording|paused|signed-out, and only with a separate VOXA_DATA
/// folder): a flow of its own, moved into that state with its effects thrown away, so no
/// microphone opens and nothing is sent or pasted. The strip only draws it.
fn strip_preview() -> Option<Flow> {
    std::env::var_os("VOXA_DATA")?;
    let kind = std::env::var("VOXA_TEST_STRIP").ok()?;
    let session = if kind == "signed-out" { Session::Expired } else { Session::Valid };
    let mut f = Flow::new(Readiness { session, network: Network::Up, mic: Mic::Granted });
    let _ = f.handle(Event::Hotkey { target: Target { app: "Notepad".into(), window: 0 } });
    if kind == "paused" {
        let _ = f.handle(Event::Pause);
    }
    Some(f)
}

/// The strip follows the system's light/dark switch while Appearance is "Use system setting".
fn follow_os_appearance(window: &mut Window, cx: &mut Context<Voxa>) {
    cx.observe_window_appearance(window, |v, _, cx| {
        v.os_dark = theme::os_is_dark(cx.window_appearance());
        if theme::apply(v.settings.appearance, v.os_dark) {
            cx.notify();
        }
    })
    .detach();
}

/// Where the strip sits: bottom centre of the main screen.
fn strip_origin(cx: &App) -> gpui::Point<gpui::Pixels> {
    let (w, h) = (px(760.), px(84.));
    let db = cx.primary_display().expect("display").bounds();
    point(db.origin.x + (db.size.width - w) / 2., db.origin.y + db.size.height - h - px(64.))
}

fn strip_options(origin: gpui::Point<gpui::Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: size(px(760.), px(84.)) })),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    }
}

/// A fresh strip window for the same Voxa, replacing one whose graphics were lost (sleep, a
/// charger change on a two-GPU laptop, a display change). Never takes focus: the window is
/// created hidden, given the old strip's exact place, and shown without activation. Runs at a
/// shortcut press, so it normally shows straight away with the new dictation; if nothing needs the
/// strip by then, it paints one blank frame unseen and hides. Returns the new window.
async fn rebuild_strip(voxa: &gpui::Entity<Voxa>, old: Option<gpui::AnyWindowHandle>, cx: &mut gpui::AsyncApp) -> Option<gpui::AnyWindowHandle> {
    let rect = os::strip_rect();
    let target = cx.update(|cx| strip_origin(cx)).ok()?;
    let v = voxa.clone();
    let old_strip = os::STRIP.load(std::sync::atomic::Ordering::Relaxed);
    let opened = cx.update(|cx| {
        let mut o = strip_options(target);
        o.show = false;
        cx.open_window(o, move |window, _| {
            if let Some(h) = os::window_ptr(window) {
                os::make_overlay(h);
                os::STRIP.store(h as usize, std::sync::atomic::Ordering::Relaxed);
            }
            v
        })
    });
    let Ok(Ok(new)) = opened else {
        // Keep using the old strip.
        os::STRIP.store(old_strip, std::sync::atomic::Ordering::Relaxed);
        return None;
    };
    let _ = new.update(cx, |_, window, cx| follow_os_appearance(window, cx));
    if let Some((x, y, w, h)) = rect {
        os::move_strip(x, y, w, h);
    }
    os::place_strip();
    let wanted = voxa.update(cx, |v, _| !matches!(v.flow.state(), State::Idle)).unwrap_or(false);
    if wanted {
        os::strip_visible(true);
    } else {
        // Paint once, unseen, so it isn't blank when it's shown later.
        let _ = voxa.update(cx, |v, cx| {
            v.strip_blank = true;
            cx.notify();
        });
        os::strip_visible(true);
        cx.background_executor().timer(Duration::from_millis(500)).await;
        let busy = voxa
            .update(cx, |v, cx| {
                v.strip_blank = false;
                cx.notify();
                !matches!(v.flow.state(), State::Idle)
            })
            .unwrap_or(false);
        if !busy {
            os::strip_visible(false);
        }
    }
    if let Some(old) = old {
        let _ = old.update(cx, |_, window, _| window.remove_window());
    }
    os::log("strip rebuilt");
    Some(new.into())
}

/// GPUI's own warnings and errors (e.g. "DirectX device lost") go into Voxa's log, so graphics
/// problems leave a trace instead of failing silently.
struct GpuiLog;

impl log::Log for GpuiLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            let msg = r.args().to_string();
            // GPUI's Windows backend logs these three every time a window is closed: it looks the
            // closed window up once more ("window not found"), then RevokeDragDrop and
            // DestroyWindow find the handle already gone (0x80040102, 0x80070578). Harmless.
            if is_window_close_noise(&msg) {
                return;
            }
            os::log(&format!("[{} {}] {msg}", r.level(), r.target()));
        }
    }
    fn flush(&self) {}
}

static GPUI_LOG: GpuiLog = GpuiLog;

fn is_window_close_noise(msg: &str) -> bool {
    msg == "window not found" || msg.contains("0x80040102") || msg.contains("0x80070578")
}

/// ChatGPT re-issues the sign-in (90 days from now) as it's used: save it, or the saved sign-in
/// expires 90 days after the original sign-in however much Voxa is used. One critical section
/// (take, check, save) under the session file lock: nothing is saved after a sign-out or a new
/// sign-in (`generation` changed, or the signed-out marker exists), and a header never replaces
/// a newer one saved meanwhile. Returns the session to use. Logs no values.
fn keep_refreshed_cookies(gpt: &voxa_chatgpt::ChatGpt, s: GptSession, generation: u64) -> GptSession {
    let _g = lock_session_file();
    let Some(header) = gpt.take_refreshed(&s.cookie_header) else { return s };
    let dir = data_dir();
    if session_gen() != generation || dir.join("signed-out").exists() {
        os::log("sign-in refreshed after a sign-out or a new sign-in: not saved");
        return s;
    }
    if GptSession::from_voxa_dir(&dir).is_some_and(|saved| saved.cookie_header != s.cookie_header) {
        os::log("sign-in refreshed, but a newer one is saved already: kept that one");
        return s;
    }
    let s = GptSession { cookie_header: header, ..s };
    match s.save_to_voxa_dir(&dir) {
        Ok(()) => os::log("sign-in refreshed by ChatGPT: saved"),
        Err(e) => os::log(&format!("FAIL saving the refreshed sign-in: {e}")),
    }
    s
}

fn main() {
    if log::set_logger(&GPUI_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--window") {
        return run_window(args.iter().any(|a| a == "--tour"));
    }
    if !os::single_instance() {
        // Voxa is already running: it opens its window.
        return;
    }
    let background = std::env::args().any(|a| a == "--background");
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::remove_file(os::log_path());
    os::log("voxa-desktop starting");
    Application::new().run(move |cx: &mut App| {
        let fonts = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(FRAUNCES)]);
        os::log(&format!("fonts: {:?}; families with Fraunces: {:?}", fonts.err(), cx.text_system().all_font_names().into_iter().filter(|n| n.contains("Fraunces")).collect::<Vec<_>>()));
        let options = strip_options(strip_origin(cx));
        let prev = os::foreground() as usize;
        let opened = cx.open_window(options, move |window, cx| {
            if let Some(h) = os::window_ptr(window) {
                os::make_overlay(h);
                os::STRIP.store(h as usize, std::sync::atomic::Ordering::Relaxed);
            }
            cx.new(Voxa::new)
        });
        if let Err(e) = &opened {
            os::log(&format!("FAIL open_window: {e}"));
        }
        if let Ok(h) = &opened {
            let _ = h.update(cx, |_, window, cx| follow_os_appearance(window, cx));
        }
        // Started by the user (not at Windows sign-in): show the window; the tour on first run.
        if let Ok(h) = &opened {
            if let Ok(voxa) = h.entity(cx) {
                let onboarded = voxa.read(cx).settings.onboarded;
                if !background || !onboarded {
                    voxa.update(cx, |v, cx| v.open_home(None, cx));
                }
            }
        }
        // GPUI shows (and activates) the window once; give focus back and hide until the hotkey.
        os::give_focus_back(prev as _);
        // Remember the strip window: it is rebuilt at a shortcut press after sleep, a power change
        // or a display change (see os::take_woke).
        if let Ok(first) = &opened {
            let handle: gpui::AnyWindowHandle = (*first).into();
            let _ = first.update(cx, |v, _, _| v.strip_window = Some(handle));
        }
        // Hide only after the first frame is painted, or the strip comes back blank later.
        let demo = std::env::var_os("VOXA_DEMO_RECORD").is_some() && fake_mode();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            os::strip_visible(false);
            if let (Some(preview), Ok(handle)) = (strip_preview(), opened.as_ref()) {
                os::log("test: showing the strip preview (nothing records)");
                let _ = handle.update(cx, |v, _, cx| {
                    v.levels = (0..QUILL_CYCLE).map(|i| stand_in::level(i as f32 * 0.4)).collect();
                    v.frame = QUILL_CYCLE as u64 * 4 + 70;
                    v.strip_preview = Some(preview);
                    cx.notify();
                });
                os::strip_visible(true);
            }
            // Dev only (with VOXA_FAKE): start a stand-in recording without touching the keyboard,
            // so the strip can be screen-recorded while someone is using the PC.
            if let (true, Ok(handle)) = (demo, opened.as_ref()) {
                cx.background_executor().timer(Duration::from_millis(400)).await;
                let fg = os::foreground();
                let target = Target { app: os::app_name(fg), window: fg as u64 };
                let _ = handle.update(cx, |v, _, cx| v.handle(Event::Hotkey { target }, cx));
            }
            // Test only (VOXA_TEST_DICTATE=<seconds>, and only with a separate VOXA_DATA folder):
            // start a real take without a key press, then CANCEL it, so nothing is ever sent or
            // pasted. Used to test the recording path (full disk, mic trouble) unattended.
            let test_secs = std::env::var("VOXA_TEST_DICTATE").ok().and_then(|s| s.parse::<u64>().ok()).filter(|_| std::env::var_os("VOXA_DATA").is_some());
            if let (Some(secs), Ok(handle)) = (test_secs, opened.as_ref()) {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let fg = os::foreground();
                let target = Target { app: os::app_name(fg), window: fg as u64 };
                os::log("test: starting a take (will be cancelled)");
                let _ = handle.update(cx, |v, _, cx| {
                    let r = Readiness { session: Session::Valid, ..v.flow.readiness() };
                    v.handle(Event::Readiness(r), cx);
                    v.handle(Event::Hotkey { target }, cx);
                });
                cx.background_executor().timer(Duration::from_secs(secs)).await;
                let _ = handle.update(cx, |v, _, cx| {
                    os::log(&format!("test: state before cancel {:?}", v.flow.state()));
                    v.handle(Event::Cancel, cx);
                });
            }
            // Test only (VOXA_TEST_PAUSE=<s> or <talk>,<paused>,<resumed> in seconds, and only with a
            // separate VOXA_DATA folder): a real take that talks, pauses and resumes, then is
            // CANCELLED, so nothing is ever sent or pasted. Logs Esc, the talking time and the file.
            let pause_secs: Option<Vec<f32>> = std::env::var("VOXA_TEST_PAUSE")
                .ok()
                .and_then(|s| s.split(',').map(|x| x.trim().parse::<f32>().ok()).collect::<Option<Vec<f32>>>())
                .map(|v| if v.len() == 1 { vec![v[0]; 3] } else { v })
                .filter(|v| v.len() == 3 && std::env::var_os("VOXA_DATA").is_some());
            if let (Some(secs), Ok(handle)) = (pause_secs, opened.as_ref()) {
                let wait = |i: usize| Duration::from_secs_f32(secs[i].max(0.0));
                let report = |v: &Voxa, at: &str| {
                    let written = v.mic.as_ref().filter(|_| v.mic_on).map(|m| m.written_secs()).unwrap_or(0.0);
                    os::log(&format!(
                        "test: {at}: state {:?}, Esc bound {}, talking {:.2} s, file {:.2} s",
                        v.flow.state(),
                        v.esc_bound,
                        v.talk_time().as_secs_f32(),
                        written
                    ));
                };
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let fg = os::foreground();
                let target = Target { app: os::app_name(fg), window: fg as u64 };
                os::log("test: starting a take to pause and resume (will be cancelled)");
                let _ = handle.update(cx, |v, _, cx| {
                    let r = Readiness { session: Session::Valid, ..v.flow.readiness() };
                    v.handle(Event::Readiness(r), cx);
                    v.handle(Event::Hotkey { target }, cx);
                });
                cx.background_executor().timer(wait(0)).await;
                let _ = handle.update(cx, |v, _, cx| {
                    report(v, "before pause");
                    v.handle(Event::Pause, cx);
                    report(v, "paused");
                });
                cx.background_executor().timer(wait(1)).await;
                // VOXA_TEST_PAUSE_END=stop: Stop while still paused (no session in a test folder,
                // so nothing is sent) and log the take's seconds, which must leave the pause out.
                if std::env::var("VOXA_TEST_PAUSE_END").is_ok_and(|e| e == "stop") {
                    let _ = handle.update(cx, |v, _, cx| {
                        report(v, "end of pause");
                        let job = match v.flow.state() {
                            State::Recording { job, .. } => Some(*job),
                            _ => None,
                        };
                        v.handle(Event::Stop, cx);
                        let secs = job.and_then(|j| v.job_secs.get(&j).copied()).unwrap_or(-1.0);
                        os::log(&format!("test: stopped while paused: take {secs:.2} s"));
                    });
                    return;
                }
                let _ = handle.update(cx, |v, _, cx| {
                    report(v, "end of pause");
                    v.handle(Event::Resume, cx);
                    report(v, "resumed");
                });
                cx.background_executor().timer(wait(2)).await;
                let _ = handle.update(cx, |v, _, cx| {
                    report(v, "before cancel");
                    v.handle(Event::Cancel, cx);
                });
            }
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_gpuis_window_close_messages_are_dropped() {
        assert!(super::is_window_close_noise("window not found"));
        assert!(super::is_window_close_noise(r#"Error { code: HRESULT(0x80040102), message: "Invalid window handle" }"#));
        assert!(super::is_window_close_noise(r#"Error { code: HRESULT(0x80070578), message: "Invalid window handle." }"#));
        assert!(!super::is_window_close_noise("DirectX device lost"));
    }
}
