//! Voxa for Windows. The strip and tray are drawn by GPUI; every state, word and button comes
//! from voxa-core. This file only turns core effects into Windows actions.

#![windows_subsystem = "windows"]

mod home;
mod import;
mod link;
mod prepare;
mod remote;
mod store;
mod tab;
mod audio;
mod stand_in;
mod win;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{
    AnyElement, App, Application, Bounds, ClickEvent, Context, Hsla, PathBuilder, Render, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, point, prelude::*, px, rgb,
    rgba, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use home::{Home, Page};
use remote::Backend;
use store::{Entry, History, Settings, Status};
use voxa_core::copy::strip_text;
use voxa_core::flow::Delivery;
use voxa_chatgpt::{ChatGpt, Session as GptSession};
use voxa_core::{Effect, Event, Flow, JobId, Mic, Network, Problem, Readiness, Session, State, Target, TranscribeError};

pub(crate) const INK: u32 = 0x8F4C2C;
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

fn fake_mode() -> bool {
    std::env::var("VOXA_FAKE").is_ok_and(|v| !v.is_empty())
}

/// %APPDATA%\voxa-next, or VOXA_DATA (tests use a throwaway folder so real history is untouched).
pub(crate) fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("VOXA_DATA") {
        return d.into();
    }
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("voxa-next")
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
                win::log("profile picture saved");
            }
        });
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Where the current (Electron) Voxa saved its session. Used once, to carry a user's sign-in over.
fn electron_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("voxa")
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
    menu_open: MenuId,
    menu_quit: MenuId,
    rec_started: Instant,
    frame: u64,
    tray: Option<TrayIcon>,
    hotkeys: GlobalHotKeyManager,
    main_key: HotKey,
    esc_key: HotKey,
    esc_bound: bool,
    mic: Option<audio::Mic>,
    mic_on: bool,
    audio_paths: HashMap<JobId, PathBuf>,
    peaks: HashMap<JobId, f32>,
    pub(crate) levels: VecDeque<f32>,
    gpt: Arc<ChatGpt>,
    session: Option<GptSession>,
    last_check: Instant,
}

impl Voxa {
    fn new(cx: &mut Context<Self>) -> Self {
        let session = if fake_mode() {
            if stand_in::starts_signed_out() { Session::Expired } else { Session::Valid }
        } else {
            Session::Unknown
        };
        let built = audio::Mic::new();
        let mic = match &built {
            Ok(m) => {
                win::log(&format!("microphone ready (paused): {}", m.device_name()));
                Mic::Granted
            }
            Err(audio::MicError::NoDevice) => Mic::Missing,
            Err(e) => {
                win::log(&format!("microphone unavailable: {e:?}"));
                Mic::Denied
            }
        };
        let flow = Flow::new(Readiness { session, network: Network::Up, mic });
        let mut settings = Settings::load(&data_dir());
        let mut history = History::load(&data_dir());
        if let Some(n) = import::run_once(&electron_dir(), &data_dir(), &mut history, &mut settings) {
            win::log(&format!("imported {n} dictations from the old Voxa"));
        }
        if history.prune(settings.keep_days, now_ms()) > 0 {
            history.save();
        }
        let apps: std::collections::HashSet<String> = history.entries.iter().map(|e| e.via.clone().unwrap_or_else(|| e.app.clone())).collect();
        for site in history.entries.iter().map(|e| e.app.as_str()).filter(|a| win::is_known_site(a)) {
            win::cache_site_icon(site);
        }
        std::thread::spawn(move || win::cache_open_app_icons(apps));
        let hotkeys = GlobalHotKeyManager::new().expect("hotkey manager");
        let main_key = settings.hotkey.parse::<HotKey>().unwrap_or_else(|_| {
            settings.hotkey = Settings::default().hotkey;
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
        });
        let esc_key = HotKey::new(None, Code::Escape);
        let mut main_key = main_key;
        match hotkeys.register(main_key) {
            Ok(()) => win::log(&format!("hotkey {} registered", settings.hotkey)),
            Err(e) => {
                // Taken by another app (e.g. the old Voxa still running): never end up with no
                // shortcut; fall back to the default and say so in Settings.
                win::log(&format!("hotkey {} unavailable ({e}); using the default", settings.hotkey));
                let fallback = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
                if fallback.id() != main_key.id() && hotkeys.register(fallback).is_ok() {
                    main_key = fallback;
                    settings.hotkey = Settings::default().hotkey;
                    settings.save(&data_dir());
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

        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(40)).await;
            let alive = this
                .update(cx, |v, cx| {
                    while let Ok(ev) = MenuEvent::receiver().try_recv() {
                        if ev.id == v.menu_open {
                            v.open_home(None, cx);
                        } else if ev.id == v.menu_quit {
                            win::log("quit from tray");
                            v.mic_test(false, cx);
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
                    if win::OPEN_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        win::log("another launch asked to open the window");
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
                        if ev.state != HotKeyState::Pressed {
                            continue;
                        }
                        if ev.id == v.main_key.id() {
                            if v.mic_testing {
                                v.mic_test(false, cx);
                            }
                            let fg = win::foreground();
                            let target = Target { app: win::app_name(fg), window: fg as u64 };
                            win::cache_app_icon(fg, win::icon_owner(fg));
                            win::cache_site_icon(&target.app);
                            win::log(&format!("hotkey in {}", win::describe(fg)));
                            let browser = win::browser_name(fg);
                            let (pid, stem) = win::window_process(fg);
                            v.handle(Event::Hotkey { target }, cx);
                            // What exactly is in front (the site in a browser tab, Claude Code in an
                            // editor): read while the user talks, off the UI thread.
                            if let State::Recording { job, .. } = v.flow.state().clone() {
                                let map = v.job_context.clone();
                                let hwnd = fg as usize;
                                std::thread::spawn(move || {
                                    let found = match browser {
                                        Some(b) => tab::domain_of(hwnd).map(|d| {
                                            win::cache_domain_icon_or_site(&d);
                                            (win::site_for_domain(&d), b)
                                        }),
                                        None => tab::coding_agent(&stem, pid).inspect(|(name, _)| win::cache_named_icon(name, "claude.ai")),
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
                    if matches!(v.flow.state(), State::Recording { .. }) {
                        let raw = if fake_mode() { stand_in::level(v.frame as f32 * 0.35) } else { audio::level() };
                        // Rises at once with the voice, settles over ~0.3 s, so the ink doesn't flicker.
                        let last = v.levels.back().copied().unwrap_or(0.0);
                        let lvl = raw.max(last * 0.88);
                        v.levels.push_back(lvl);
                        while v.levels.len() > QUILL_CYCLE {
                            v.levels.pop_front();
                        }
                    }
                    if !fake_mode() && v.last_check.elapsed() > RECHECK && matches!(v.flow.state(), State::Idle) {
                        v.check_session(cx);
                    }
                    if matches!(v.flow.state(), State::Recording { .. } | State::Transcribing { .. } | State::Delivering { .. }) {
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

        let mut me = Self {
            flow,
            history,
            settings,
            mic_name,
            mic_testing: false,
            job_entries: HashMap::new(),
            job_secs: HashMap::new(),
            published: None,
            hotkey_result: None,
            hotkey_paused: false,
            job_context: Arc::new(std::sync::Mutex::new(HashMap::new())),
            signing_in: false,
            menu_open: open_item.id().clone(),
            menu_quit: quit_item.id().clone(),
            rec_started: Instant::now(),
            frame: 0,
            tray,
            hotkeys,
            main_key,
            esc_key,
            esc_bound: false,
            mic: built.ok(),
            mic_on: false,
            audio_paths: HashMap::new(),
            peaks: HashMap::new(),
            levels: VecDeque::new(),
            gpt: Arc::new(ChatGpt::new()),
            session: None,
            last_check: Instant::now(),
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
        cx.spawn(async move |this, cx| {
            let (session, state, network) = cx
                .background_executor()
                .spawn(async move {
                    let dir = data_dir();
                    // Carry an Electron sign-in over once, unless the user signed out here.
                    let saved = GptSession::from_voxa_dir(&dir).or_else(|| if dir.join("signed-out").exists() { None } else { GptSession::from_electron_dir(&electron_dir()) });
                    match saved {
                        None => (None, Session::Expired, Network::Up),
                        Some(s) => match gpt.account(&s) {
                            Ok(a) => {
                                save_account(&a);
                                (Some(s), Session::Valid, Network::Up)
                            }
                            Err(TranscribeError::SignedOut | TranscribeError::SessionExpired) => (Some(s), Session::Expired, Network::Up),
                            Err(TranscribeError::Offline) => (Some(s), Session::Unknown, Network::Down),
                            Err(_) => (Some(s), Session::Unknown, Network::Up),
                        },
                    }
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                win::log(&format!("session check: {state:?}, network {network:?}"));
                v.session = session;
                let r = Readiness { session: state, network, ..v.flow.readiness() };
                if r != v.flow.readiness() {
                    v.handle(Event::Readiness(r), cx);
                }
            });
        })
        .detach();
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        win::log(&format!("event {event:?}"));
        let effects = self.flow.handle(event);
        win::log(&format!("  -> {:?} | effects {effects:?}", self.flow.state()));
        for e in effects {
            self.run(e, cx);
        }
        self.sync_history();
        self.publish();
        self.sync_esc();
        self.sync_tray();
        cx.notify();
    }

    fn run(&mut self, effect: Effect, cx: &mut Context<Self>) {
        match effect {
            Effect::ShowStrip => win::strip_visible(true),
            Effect::HideStrip { after_ms: 0 } => win::strip_visible(false),
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
                        win::log(&format!("  recording to {}", path.display()));
                        self.audio_paths.insert(job, path);
                        self.mic_on = true;
                    }
                    Err(e) => {
                        // The mic would not open: undo the recording and show the blocker instead.
                        win::log(&format!("  mic failed: {e:?}"));
                        let target = self.flow.job(job).map(|j| j.target.clone());
                        let mic = if matches!(e, audio::MicError::NoDevice) { Mic::Missing } else { Mic::Denied };
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
                self.job_secs.insert(job, self.rec_started.elapsed().as_secs_f32());
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    self.mic_on = false;
                    let peak = m.finish();
                    win::log(&format!("  recording stopped, peak rms {peak:.4}"));
                    self.peaks.insert(job, peak);
                }
            }
            Effect::DiscardCapture { .. } => {
                if let Some(m) = self.mic.as_mut().filter(|_| self.mic_on) {
                    self.mic_on = false;
                    m.discard();
                }
            }
            Effect::Transcribe { job } if !fake_mode() => {
                if self.peaks.get(&job).is_some_and(|p| *p < SILENCE) {
                    win::log("  silence: not sending");
                    self.handle(Event::Transcribed { job, result: Ok(String::new()) }, cx);
                    return;
                }
                let path = self.audio_paths.get(&job).cloned();
                let peak = self.peaks.get(&job).copied().unwrap_or(1.0);
                let session = self.session.clone();
                let gpt = self.gpt.clone();
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let Some(session) = session else { return Err(TranscribeError::SignedOut) };
                            let Some(audio) = path.and_then(|p| prepare::audio_for_upload(&p)) else { return Err(TranscribeError::Server(0)) };
                            gpt.transcribe(&session, audio, "recording.wav", "audio/wav").map(|t| prepare::finish_text(t, &data_dir()))
                        })
                        .await;
                    let result = match result {
                        Ok(t) if invented_for_silence(&t, peak) => {
                            win::log(&format!("  dropped {t:?}: near-silent take (peak {peak:.4})"));
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
                let fg = win::foreground();
                let same_tab = !win::browser_name(fg).is_some() || win::app_name(fg) == target.app;
                let result = if fg as u64 == target.window && same_tab {
                    if win::paste_here(&text) { Delivery::Pasted } else { Delivery::Failed }
                } else {
                    let app = win::app_name(fg);
                    win::cache_app_icon(fg, win::icon_owner(fg));
                    win::cache_site_icon(&app);
                    Delivery::TargetChanged(Target { app, window: fg as u64 })
                };
                self.handle(Event::Delivered { job, result }, cx);
            }
            Effect::PasteHere { job, text } => {
                let result = if win::paste_here(&text) { Delivery::Pasted } else { Delivery::Failed };
                self.handle(Event::Delivered { job, result }, cx);
            }
            Effect::OpenSignIn if !fake_mode() => {
                let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("voxa-signin.exe")));
                let Some(exe) = exe.filter(|p| p.exists()) else {
                    win::log("FAIL sign-in window not found next to voxa-desktop.exe");
                    return;
                };
                if self.signing_in {
                    return;
                }
                win::log("sign-in: opening the sign-in window");
                self.signing_in = true;
                cx.spawn(async move |this, cx| {
                    let code = cx
                        .background_executor()
                        .spawn(async move { std::process::Command::new(exe).status().ok().and_then(|s| s.code()) })
                        .await;
                    let _ = this.update(cx, |v, cx| {
                        win::log(&format!("sign-in window closed (exit {code:?})"));
                        v.signing_in = false;
                        cx.notify();
                        // Signed in or not, re-check: the core resends a waiting dictation when the session is valid.
                        v.check_session(cx);
                    });
                })
                .detach();
            }
            Effect::OpenSignIn => {
                win::log("STAND-IN sign-in: pretending the user signs in");
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
                let _ = std::process::Command::new("explorer").arg("ms-settings:privacy-microphone").spawn();
            }
            Effect::SaveJob { job } => {
                if let Some(j) = self.flow.job(job) {
                    win::log(&format!("  job {:?} for {} -> {:?}", j.id, j.target.app, j.state));
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
                via: host
                    .clone()
                    .or_else(|| win::browser_name(job.target.window as _))
                    .filter(|b| *b != app)
                    .or_else(|| self.history.get(id).and_then(|e| e.via.clone())),
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
        }
        if changed {
            self.history.save();
        }
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
        if page == Some(Page::Setup(1)) {
            c.arg("--tour");
        }
        match c.spawn() {
            Ok(_) => win::log("window process started"),
            Err(e) => win::log(&format!("FAIL window process: {e}")),
        }
    }

    /// Tell the window process what it can't see in the files: sign-in, network, mic, live jobs.
    fn publish(&mut self) {
        let (session, online, mic) = link::Live::from_readiness(self.flow.readiness());
        let live_ids = self.job_entries.values().copied().filter(|id| self.is_live(*id)).collect();
        let live = link::Live { session, online, mic, mic_name: self.mic_name.clone(), signing_in: self.signing_in, live_ids, hotkey_result: self.hotkey_result.clone() };
        if self.published.as_ref() != Some(&live) {
            link::write_live(&live);
            self.published = Some(live);
        }
    }

    fn on_command(&mut self, cmd: link::Command, cx: &mut Context<Self>) {
        use link::Command::*;
        win::log(&format!("window asks: {}", match &cmd { SetHotkey(_) => "SetHotkey".into(), c => format!("{c:?}") }));
        match cmd {
            SignIn => self.sign_in(cx),
            SignOut => self.sign_out(cx),
            SetHotkey(s) => {
                let r = self.set_hotkey(&s, cx);
                self.hotkey_result = Some((s, r.err()));
            }
            PauseHotkey(p) => self.pause_hotkey(p),
            ReloadSettings => {
                self.settings = Settings::load(&data_dir());
                if self.history.prune(self.settings.keep_days, now_ms()) > 0 {
                    self.history.save();
                }
            }
            Resend(id) => self.resend(id, cx),
            ResendAll => self.resend_all(cx),
            DeleteEntry(id) => self.delete_entry(id, cx),
            DeleteAll => self.delete_history(cx),
        }
        self.publish();
    }

    pub(crate) fn sign_in(&mut self, cx: &mut Context<Self>) {
        let _ = std::fs::remove_file(data_dir().join("signed-out"));
        self.run(Effect::OpenSignIn, cx);
    }

    pub(crate) fn sign_out(&mut self, cx: &mut Context<Self>) {
        let dir = data_dir();
        let _ = std::fs::remove_file(dir.join("session.json"));
        // The sign-in window's cookies too, so the next sign-in really asks.
        let _ = std::fs::remove_dir_all(dir.join("webview"));
        let _ = std::fs::write(dir.join("signed-out"), b"");
        let _ = std::fs::remove_file(dir.join("account.json"));
        let _ = std::fs::remove_file(dir.join("avatar.png"));
        self.session = None;
        win::log("signed out");
        let r = Readiness { session: Session::Expired, ..self.flow.readiness() };
        self.handle(Event::Readiness(r), cx);
    }

    /// Stop listening for the shortcut while the user picks a new one.
    pub(crate) fn pause_hotkey(&mut self, pause: bool) {
        if pause == self.hotkey_paused {
            return;
        }
        let r = if pause { self.hotkeys.unregister(self.main_key) } else { self.hotkeys.register(self.main_key) };
        win::log(&format!("shortcut {} ({r:?})", if pause { "paused" } else { "resumed" }));
        self.hotkey_paused = pause;
    }

    pub(crate) fn set_hotkey(&mut self, s: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let new = s.parse::<HotKey>().map_err(|_| format!("{s} can't be used as a shortcut"))?;
        if new.id() == self.main_key.id() {
            return Ok(());
        }
        let _ = self.hotkeys.unregister(self.main_key);
        if let Err(e) = self.hotkeys.register(new) {
            win::log(&format!("shortcut {s} refused: {e}"));
            let _ = self.hotkeys.register(self.main_key);
            return Err(format!("{s} is already used by another app. Try a different one."));
        }
        win::log(&format!("shortcut changed to {s}"));
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
        self.history.delete_all();
        win::log("history deleted");
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
        self.history.upsert(Entry { status: Status::Waiting("Writing it out".into()), ..entry.clone() });
        cx.notify();
        let gpt = self.gpt.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let bytes = prepare::audio_for_upload(&audio).ok_or(TranscribeError::Server(0))?;
                    let (name, mime) = if audio.extension().is_some_and(|e| e == "webm") { ("recording.webm", "audio/webm") } else { ("recording.wav", "audio/wav") };
                    gpt.transcribe(&session, bytes, name, mime).map(|t| prepare::finish_text(t, &data_dir()))
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                win::log(&format!("resend {id}: {}", if result.is_ok() { "ok" } else { "failed" }));
                let next = match result {
                    Ok(t) if !t.trim().is_empty() => Entry { text: t, status: Status::Ready, ..entry },
                    Ok(_) => Entry { status: Status::Failed("No speech in the recording".into()), ..entry },
                    Err(e) if e.needs_sign_in() => Entry { status: Status::Waiting("Sign in to send".into()), ..entry },
                    Err(_) => Entry { status: Status::Failed("Couldn't reach ChatGPT".into()), ..entry },
                };
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
            win::log("history entry deleted");
        }
        cx.notify();
    }

    pub(crate) fn resend_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .history
            .entries
            .iter()
            .filter(|e| matches!(e.status, Status::Waiting(_) | Status::Failed(_)) && e.audio.as_ref().is_some_and(|a| a.exists()) && !self.is_live(e.id))
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
                    win::log("mic test on");
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
            win::log("mic test off");
        }
        cx.notify();
    }

    /// Esc is bound only while recording, so Voxa never steals Esc from other apps.
    fn sync_esc(&mut self) {
        let want = matches!(self.flow.state(), State::Recording { .. });
        if want != self.esc_bound {
            let r = if want { self.hotkeys.register(self.esc_key) } else { self.hotkeys.unregister(self.esc_key) };
            self.esc_bound = want && r.is_ok();
            win::log(&format!("Esc {} ({r:?})", if want { "bound" } else { "released" }));
        }
    }

    fn sync_tray(&self) {
        let dot = match self.flow.state() {
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
    let (bg, fg) = if primary { (INK, 0xFFFFFF) } else { (0xFFFFFF, 0x1A1A1A) };
    div()
        .id(id)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .border_1()
        .border_color(rgba(0x00000022))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .cursor_pointer()
        .child(label.into())
}

/// Leading glyph: a different shape per kind of state, never colour alone.
fn glyph(state: &State, frame: u64) -> AnyElement {
    let ink: Hsla = rgb(INK).into();
    let (kind, color) = match state {
        State::Recording { .. } => ("dot", 0xE5483D),
        State::Transcribing { .. } | State::Delivering { .. } => ("spin", INK),
        State::Delivered { .. } => ("check", 0x0F7B0F),
        State::NeedsYou(Problem::NoSpeech { .. }) => ("info", 0x5F5F5F),
        State::NeedsYou(Problem::Unreachable { .. }) | State::Blocked(_) => ("alert", 0xC42B1C),
        _ => ("alert", 0x9D5D00),
    };
    let color: Hsla = rgb(color).into();
    let _ = ink;
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
    let ink: Hsla = rgb(INK).into();
    let pen: Hsla = rgb(0x2D2418).into();
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
                window.paint_path(path, gpui::hsla(0.08, 0.3, 0.75, 0.55 * alpha));
            }
        },
    )
    .w(px(200.))
    .h(px(36.))
    .into_any_element()
}

impl Render for Voxa {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.flow.state().clone();
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
        let text = strip_text(&self.flow);

        match &state {
            State::Recording { job, .. } => {
                let app = self.flow.job(*job).map(|j| j.target.app.clone()).unwrap_or_default();
                let secs = self.rec_started.elapsed().as_secs();
                row.push(div().text_size(px(12.)).text_color(rgb(0x5F5F5F)).child(app).into_any_element());
                // This pass's levels: the last (tick + 1) ticks.
                let tick = self.frame as usize % QUILL_CYCLE;
                let pass: Vec<f32> = self.levels.iter().skip(self.levels.len().saturating_sub(tick + 1)).copied().collect();
                row.push(ink_line(pass, tick));
                row.push(div().w(px(36.)).child(format!("{}:{:02}", secs / 60, secs % 60)).into_any_element());
                if let Some(t) = &text {
                    if t.title != "Listening" {
                        row.push(div().text_size(px(12.)).text_color(rgb(0x7A5100)).child(t.title.clone()).into_any_element());
                    }
                }
                row.push(
                    button("cancel", "Cancel", false)
                        .on_click(cx.listener(|v, _: &ClickEvent, _, cx| v.handle(Event::Cancel, cx)))
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
                        col = col.child(div().text_size(px(12.)).text_color(rgb(0x5F5F5F)).child(d));
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
                .bg(rgba(0xF9F7F4F2))
                .border_1()
                .border_color(rgba(0x00000020))
                .shadow_lg()
                .font_family("Segoe UI")
                .text_size(px(14.))
                .text_color(rgb(0x1A1A1A))
                .children(row),
        )
    }
}

/// `voxa-desktop --window`: just the Voxa window. Exits when the window closes, returning its
/// memory; the tray process keeps running.
fn run_window(tour: bool) {
    if !win::single_window_instance() {
        return; // already open: it was asked to come to the front
    }
    Application::new().run(move |cx: &mut App| {
        let _ = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!("../assets/Fraunces.ttf").as_slice())]);
        let backend = cx.new(Backend::new);
        let first = if tour || !backend.read(cx).settings.onboarded { Page::Setup(1) } else { Page::Home };
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
            if let Ok(RawWindowHandle::Win32(w)) = window.window_handle().map(|h| h.as_raw()) {
                win::light_caption(w.hwnd.get() as _);
                win::bring_to_front(w.hwnd.get() as _);
            }
            cx.new(|cx| Home::new(backend, first, window, cx))
        });
        match opened {
            Ok(h) => {
                win::log(&format!("window opened on {first:?}"));
                // Another "Open Voxa" while open: come to the front.
                cx.spawn(async move |cx| loop {
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    if win::OPEN_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        let r = h.update(cx, |_, window, _| {
                            window.activate_window();
                            if let Ok(RawWindowHandle::Win32(w)) = window.window_handle().map(|h| h.as_raw()) {
                                win::bring_to_front(w.hwnd.get() as _);
                            }
                        });
                        if r.is_err() {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(e) => win::log(&format!("FAIL open window: {e}")),
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--window") {
        return run_window(args.iter().any(|a| a == "--tour"));
    }
    if !win::single_instance() {
        // Voxa is already running: it opens its window.
        return;
    }
    let background = std::env::args().any(|a| a == "--background");
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::remove_file(win::log_path());
    win::log("voxa-desktop starting");
    Application::new().run(move |cx: &mut App| {
        let fonts = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!("../assets/Fraunces.ttf").as_slice())]);
        win::log(&format!("fonts: {:?}; families with Fraunces: {:?}", fonts.err(), cx.text_system().all_font_names().into_iter().filter(|n| n.contains("Fraunces")).collect::<Vec<_>>()));
        let (w, h) = (px(760.), px(84.));
        let db = cx.primary_display().expect("display").bounds();
        let origin = point(db.origin.x + (db.size.width - w) / 2., db.origin.y + db.size.height - h - px(64.));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: size(w, h) })),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let prev = win::foreground() as usize;
        let opened = cx.open_window(options, move |window, cx| {
            if let Ok(RawWindowHandle::Win32(h)) = window.window_handle().map(|h| h.as_raw()) {
                win::make_overlay(h.hwnd.get() as _);
                win::STRIP.store(h.hwnd.get() as usize, std::sync::atomic::Ordering::Relaxed);
            }
            cx.new(Voxa::new)
        });
        if let Err(e) = &opened {
            win::log(&format!("FAIL open_window: {e}"));
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
        win::give_focus_back(prev as _);
        // Hide only after the first frame is painted, or the strip comes back blank later.
        let demo = std::env::var_os("VOXA_DEMO_RECORD").is_some() && fake_mode();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            win::strip_visible(false);
            // Dev only (with VOXA_FAKE): start a stand-in recording without touching the keyboard,
            // so the strip can be screen-recorded while someone is using the PC.
            if let (true, Ok(handle)) = (demo, opened) {
                cx.background_executor().timer(Duration::from_millis(400)).await;
                let fg = win::foreground();
                let target = Target { app: win::app_name(fg), window: fg as u64 };
                let _ = handle.update(cx, |v, _, cx| v.handle(Event::Hotkey { target }, cx));
            }
        })
        .detach();
    });
}
