//! Voxa for Windows. The strip and tray are drawn by GPUI; every state, word and button comes
//! from voxa-core. This file only turns core effects into Windows actions.

#![windows_subsystem = "windows"]

mod prepare;
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
    AnyElement, App, Application, Bounds, ClickEvent, Context, Hsla, PathBuilder, Render, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div, point, prelude::*, px, rgb,
    rgba, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use voxa_core::copy::strip_text;
use voxa_core::flow::Delivery;
use voxa_chatgpt::{ChatGpt, Session as GptSession};
use voxa_core::{Effect, Event, Flow, JobId, Mic, Network, Problem, Readiness, Session, State, Target, TranscribeError};

const INK: u32 = 0x8F4C2C;
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

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("voxa-next")
}

/// Where the current (Electron) Voxa saved its session. Used once, to carry a user's sign-in over.
fn electron_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into())).join("voxa")
}

struct Voxa {
    flow: Flow,
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
    levels: VecDeque<f32>,
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
        let hotkeys = GlobalHotKeyManager::new().expect("hotkey manager");
        let main_key = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
        let esc_key = HotKey::new(None, Code::Escape);
        match hotkeys.register(main_key) {
            Ok(()) => win::log("hotkey Ctrl+Alt+Space registered"),
            Err(e) => win::log(&format!("FAIL hotkey: {e}")),
        }
        let tray = TrayIconBuilder::new().with_tooltip("Voxa").with_icon(tray_icon(None)).build().ok();

        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(40)).await;
            let alive = this
                .update(cx, |v, cx| {
                    while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                        if ev.state != HotKeyState::Pressed {
                            continue;
                        }
                        if ev.id == v.main_key.id() {
                            let fg = win::foreground();
                            let target = Target { app: win::app_name(fg), window: fg as u64 };
                            win::log(&format!("hotkey in {}", win::describe(fg)));
                            v.handle(Event::Hotkey { target }, cx);
                        } else if ev.id == v.esc_key.id() {
                            v.handle(Event::Cancel, cx);
                        }
                    }
                    if matches!(v.flow.state(), State::Recording { .. }) {
                        let lvl = if fake_mode() { stand_in::level(v.frame as f32 * 0.35) } else { audio::level() };
                        v.levels.push_back(lvl);
                        while v.levels.len() > 90 {
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
                    let saved = GptSession::from_voxa_dir(&data_dir()).or_else(|| GptSession::from_electron_dir(&electron_dir()));
                    match saved {
                        None => (None, Session::Expired, Network::Up),
                        Some(s) => match gpt.access_token(&s) {
                            Ok(_) => (Some(s), Session::Valid, Network::Up),
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
                let result = if fg as u64 == target.window {
                    if win::paste_here(&text) { Delivery::Pasted } else { Delivery::Failed }
                } else {
                    Delivery::TargetChanged(Target { app: win::app_name(fg), window: fg as u64 })
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
                win::log("sign-in: opening the sign-in window");
                cx.spawn(async move |this, cx| {
                    let code = cx
                        .background_executor()
                        .spawn(async move { std::process::Command::new(exe).status().ok().and_then(|s| s.code()) })
                        .await;
                    let _ = this.update(cx, |v, cx| {
                        win::log(&format!("sign-in window closed (exit {code:?})"));
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
    let (w, h) = (32u32, 32u32);
    let mut px_ = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let (dx, dy) = (x as i32 - 24, y as i32 - 24);
            if let Some(c) = dot.filter(|_| dx * dx + dy * dy <= 49) {
                px_[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            } else if (2..26).contains(&x) && (2..26).contains(&y) {
                px_[i..i + 4].copy_from_slice(&[0x8F, 0x4C, 0x2C, 255]);
            }
        }
    }
    Icon::from_rgba(px_, w, h).expect("icon")
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

fn ink_line(levels: Vec<f32>) -> AnyElement {
    let ink: Hsla = rgb(INK).into();
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let mid = b.origin.y + b.size.height / 2.;
            let w = f32::from(b.size.width);
            let mut p = PathBuilder::stroke(px(2.));
            // Newest level on the right; older speech scrolls left. Alternate up and down like a pen stroke.
            let n = 90usize;
            let pad = n.saturating_sub(levels.len());
            p.move_to(point(b.origin.x, mid));
            for i in 1..=n {
                let x = w * i as f32 / n as f32;
                let lvl = if i < pad { 0.0 } else { levels.get(i - pad).copied().unwrap_or(0.0) };
                let y = if i % 2 == 0 { lvl } else { -lvl } * 12.;
                p.line_to(point(b.origin.x + px(x), mid + px(y)));
            }
            if let Ok(p) = p.build() {
                window.paint_path(p, ink);
            }
        },
    )
    .w(px(180.))
    .h(px(28.))
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
                row.push(ink_line(self.levels.iter().copied().collect()));
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

fn main() {
    let _ = std::fs::remove_file("voxa-desktop.log");
    win::log("voxa-desktop starting");
    Application::new().run(|cx: &mut App| {
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
        if let Err(e) = opened {
            win::log(&format!("FAIL open_window: {e}"));
        }
        // GPUI shows (and activates) the window once; give focus back and hide until the hotkey.
        win::give_focus_back(prev as _);
        // Hide only after the first frame is painted, or the strip comes back blank later.
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            win::strip_visible(false);
        })
        .detach();
    });
}
