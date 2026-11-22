//! Voxa desktop strip spike on GPUI (Windows).
//!
//! What it proves, each step written to spike.log:
//!  1. The strip is a topmost overlay that never takes focus, even when its buttons are clicked.
//!  2. A global hotkey (Ctrl+Alt+Space) starts and stops dictation from any app.
//!  3. A tray icon whose status dot follows the state.
//!  4. "Stop" pastes into the app you started in and restores your clipboard.
//! There is no microphone or ChatGPT here: the ink line is driven by a fake level.

#![windows_subsystem = "windows"]

mod win;

use std::time::{Duration, Instant};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{
    App, Application, Bounds, ClickEvent, Context, Hsla, PathBuilder, Render, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, prelude::*, px, rgb, rgba, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const PASTE_TEXT: &str = "Hello from the Voxa GPUI spike.";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Ready,
    Recording,
    Writing,
    Pasted,
}

struct Strip {
    phase: Phase,
    started: Instant,
    target: usize, // HWND of the app the dictation is for
    target_name: String,
    frame: u64,
    tray: Option<TrayIcon>,
    _hotkeys: GlobalHotKeyManager,
    hotkey_id: u32,
}

impl Strip {
    fn new(cx: &mut Context<Self>) -> Self {
        let manager = GlobalHotKeyManager::new().expect("hotkey manager");
        let hk = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
        match manager.register(hk) {
            Ok(()) => win::log("hotkey Ctrl+Alt+Space registered"),
            Err(e) => win::log(&format!("FAIL hotkey register: {e}")),
        }
        let tray = TrayIconBuilder::new()
            .with_tooltip("Voxa (spike)")
            .with_icon(tray_icon_for(Phase::Ready))
            .build()
            .map_err(|e| win::log(&format!("FAIL tray: {e}")))
            .ok();
        if tray.is_some() {
            win::log("tray icon created");
        }

        // One ticker: animate the ink line and poll hotkey events.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(40)).await;
                let alive = this
                    .update(cx, |strip, cx| {
                        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                            if ev.id == strip.hotkey_id && ev.state == HotKeyState::Pressed {
                                win::log(&format!("hotkey pressed; foreground = {}", win::describe(win::foreground())));
                                match strip.phase {
                                    Phase::Recording => strip.stop(cx),
                                    _ => strip.start(cx),
                                }
                            }
                        }
                        if strip.phase == Phase::Recording {
                            strip.frame += 1;
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();

        Self {
            phase: Phase::Ready,
            started: Instant::now(),
            target: 0,
            target_name: String::new(),
            frame: 0,
            tray,
            hotkey_id: hk.id(),
            _hotkeys: manager,
        }
    }

    fn set_phase(&mut self, phase: Phase, cx: &mut Context<Self>) {
        self.phase = phase;
        if let Some(t) = &self.tray {
            let _ = t.set_icon(Some(tray_icon_for(phase)));
        }
        win::log(&format!("phase -> {phase:?}"));
        cx.notify();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let fg = win::foreground();
        self.target = fg as usize;
        self.target_name = win::describe(fg);
        self.started = Instant::now();
        win::log(&format!("start: target = {}", self.target_name));
        win::strip_visible(true);
        self.set_phase(Phase::Recording, cx);
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.set_phase(Phase::Writing, cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(700)).await;
            let _ = this.update(cx, |strip, cx| {
                win::paste_into(strip.target as _, PASTE_TEXT);
                strip.set_phase(Phase::Pasted, cx);
            });
            cx.background_executor().timer(Duration::from_millis(2000)).await;
            let _ = this.update(cx, |strip, cx| strip.set_phase(Phase::Ready, cx));
            win::strip_visible(false);
        })
        .detach();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.set_phase(Phase::Ready, cx);
    }
}

/// Log which window has focus at the moment a strip button is clicked: the pass condition.
fn clicked(name: &str) {
    win::log(&format!("CLICK {name}; foreground = {}", win::describe(win::foreground())));
}

fn tray_icon_for(phase: Phase) -> Icon {
    let (w, h) = (32u32, 32u32);
    let dot: Option<[u8; 3]> = match phase {
        Phase::Ready => None,
        Phase::Recording => Some([0xE5, 0x48, 0x3D]),
        Phase::Writing => Some([0xC9, 0x8A, 0x16]),
        Phase::Pasted => Some([0x2E, 0x8B, 0x57]),
    };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let in_square = x >= 2 && x < 26 && y >= 2 && y < 26;
            let in_dot = dot.is_some() && {
                let (dx, dy) = (x as i32 - 24, y as i32 - 24);
                dx * dx + dy * dy <= 49
            };
            if in_dot {
                let c = dot.unwrap();
                rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            } else if in_square {
                rgba[i..i + 4].copy_from_slice(&[0x8F, 0x4C, 0x2C, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, w, h).expect("icon")
}

fn button(id: &'static str, label: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, fg): (u32, u32) = if primary { (0x8F4C2C, 0xFFFFFF) } else { (0xFFFFFF, 0x1A1A1A) };
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
        .child(label)
}

impl Render for Strip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ink: Hsla = rgb(0x8F4C2C).into();
        let frame = self.frame;
        let mark = div()
            .size(px(24.))
            .rounded(px(6.))
            .bg(rgb(0x8F4C2C))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(0xFFFFFF))
            .text_size(px(14.))
            .child("v");

        let content: Vec<gpui::AnyElement> = match self.phase {
            Phase::Ready => vec![
                div().child("Ready. Ctrl+Alt+Space in any app, or").into_any_element(),
                button("start", "Start", true)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        clicked("Start");
                        this.start(cx);
                    }))
                    .into_any_element(),
            ],
            Phase::Recording => {
                let secs = self.started.elapsed().as_secs();
                vec![
                    div().text_color(rgb(0x5F5F5F)).text_size(px(12.)).child("Recording").into_any_element(),
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let mid = bounds.origin.y + bounds.size.height / 2.;
                            let w = f32::from(bounds.size.width);
                            let mut b = PathBuilder::stroke(px(2.));
                            b.move_to(point(bounds.origin.x, mid));
                            for i in 1..=90 {
                                let x = w * i as f32 / 90.;
                                let t = (frame as f32 * 0.35) + i as f32 * 0.5;
                                // bursts and pauses, like speech
                                let env = ((t * 0.21).sin() * 0.5 + 0.5).powf(2.0) * (0.6 + 0.4 * (t * 1.7).sin().abs());
                                let y = (t * 1.3).sin() * env * 11.;
                                b.line_to(point(bounds.origin.x + px(x), mid + px(y)));
                            }
                            if let Ok(path) = b.build() {
                                window.paint_path(path, ink);
                            }
                        },
                    )
                    .w(px(180.))
                    .h(px(28.))
                    .into_any_element(),
                    div().w(px(40.)).child(format!("0:{secs:02}")).into_any_element(),
                    button("cancel", "Cancel", false)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            clicked("Cancel");
                            this.cancel(cx);
                        }))
                        .into_any_element(),
                    button("stop", "Stop", true)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            clicked("Stop");
                            this.stop(cx);
                        }))
                        .into_any_element(),
                ]
            }
            Phase::Writing => vec![div().child("Writing it out...").into_any_element()],
            Phase::Pasted => vec![div().child("Pasted, clipboard restored").into_any_element()],
        };

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
                .child(mark)
                .children(content),
        )
    }
}

fn main() {
    let _ = std::fs::remove_file("spike.log");
    win::log("spike starting");
    Application::new().run(|cx: &mut App| {
        let (w, h) = (px(620.), px(84.));
        let display = cx.primary_display().expect("display");
        let db = display.bounds();
        let origin = point(
            db.origin.x + (db.size.width - w) / 2.,
            db.origin.y + db.size.height - h - px(64.),
        );
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
            match window.window_handle().map(|h| h.as_raw()) {
                Ok(RawWindowHandle::Win32(h)) => {
                    win::make_overlay(h.hwnd.get() as _);
                    win::STRIP.store(h.hwnd.get() as usize, std::sync::atomic::Ordering::Relaxed);
                }
                other => win::log(&format!("FAIL no Win32 handle: {other:?}")),
            }
            cx.new(Strip::new)
        });
        if let Err(e) = opened {
            win::log(&format!("FAIL open_window: {e}"));
        }
        win::give_focus_back(prev as _);
        win::log(&format!("strip shown; foreground = {}", win::describe(win::foreground())));
    });
}
