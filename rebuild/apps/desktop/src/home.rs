//! The Voxa window (docs/ui-mocks/v7.1-native.html, "Windows · the window"): History is the home
//! page, with what needs you on top as InfoBars, then a one-line ready status, then the days.
//! Insights and Settings are their own pages. A guided setup runs on first launch.

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, Render, SharedString, Window, div,
    prelude::*, px, rgb, rgba,
};

use crate::store::{self, Settings, Status};
use crate::{INK, Voxa};
use voxa_core::{Blocker, Network, Session};

const TEXT: u32 = 0x1A1A1A;
const MUTED: u32 = 0x5F5F5F;
const PAGE_BG: u32 = 0xFBFAF8;
const NAV_BG: u32 = 0xF1EEEA;
const CARD_BORDER: u32 = 0x0000000F;

// Segoe Fluent Icons code points.
const I_HISTORY: char = '\u{E81C}';
const I_INSIGHTS: char = '\u{E9D2}';
const I_SETTINGS: char = '\u{E713}';
const I_COPY: char = '\u{E8C8}';
const I_SEARCH: char = '\u{E721}';
const I_ACCOUNT: char = '\u{E77B}';
const I_KEYBOARD: char = '\u{E765}';
const I_MIC: char = '\u{E720}';
const I_KEEP: char = '\u{E72E}';
const I_DELETE: char = '\u{E74D}';
const I_POWER: char = '\u{E7E8}';
const I_CLOSE: char = '\u{E711}';
const I_CHECK: char = '\u{E73E}';
const I_DOWN: char = '\u{E70D}';
const I_TEXT: char = '\u{E8D2}';
const I_SEND: char = '\u{E724}';

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    History,
    Insights,
    Settings,
    /// Guided setup, steps 1 to 4.
    Setup(u8),
}

pub struct Home {
    voxa: Entity<Voxa>,
    pub page: Page,
    focus: FocusHandle,
    /// Entry whose Copy button was just pressed.
    copied: Option<u64>,
    /// Waiting for the user to press a new shortcut.
    capturing: bool,
    hotkey_error: Option<String>,
    confirm_delete: bool,
    search: String,
    search_active: bool,
    dismissed: Vec<&'static str>,
    /// Newest history id when the "Try it" step opened, to spot the first dictation.
    try_since: u64,
}

impl Home {
    pub fn new(voxa: Entity<Voxa>, page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&voxa, |_, _, cx| cx.notify()).detach();
        let focus = cx.focus_handle();
        window.focus(&focus);
        let mut me = Self {
            voxa,
            page: Page::History,
            focus,
            copied: None,
            capturing: false,
            hotkey_error: None,
            confirm_delete: false,
            search: String::new(),
            search_active: false,
            dismissed: Vec::new(),
            try_since: 0,
        };
        me.go(page, cx);
        me
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.capturing {
            self.stop_capture(cx);
        }
        let testing = matches!(page, Page::Setup(2));
        self.voxa.update(cx, |v, cx| v.mic_test(testing, cx));
        if page == Page::Setup(3) {
            self.try_since = self.voxa.read(cx).history.entries.last().map(|e| e.id).unwrap_or(0);
        }
        self.confirm_delete = false;
        self.page = page;
        cx.notify();
    }

    fn stop_capture(&mut self, cx: &mut Context<Self>) {
        self.capturing = false;
        self.voxa.update(cx, |v, _| v.pause_hotkey(false));
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if self.capturing {
            if k.key == "escape" && !k.modifiers.control && !k.modifiers.alt {
                self.stop_capture(cx);
                cx.notify();
                return;
            }
            match shortcut_from(&k.modifiers, &k.key) {
                Some(s) => {
                    self.stop_capture(cx);
                    let r = self.voxa.update(cx, |v, cx| v.set_hotkey(&s, cx));
                    self.hotkey_error = r.err();
                }
                None => self.hotkey_error = Some("Use Ctrl or Alt with a letter, number, Space or F-key".into()),
            }
            cx.notify();
            return;
        }
        if self.page == Page::History && self.search_active {
            match k.key.as_str() {
                "escape" => {
                    self.search.clear();
                    self.search_active = false;
                }
                "backspace" => {
                    self.search.pop();
                }
                _ if !k.modifiers.control && !k.modifiers.alt => {
                    if let Some(c) = k.key_char.as_ref() {
                        self.search.push_str(c);
                    }
                }
                _ => {}
            }
            cx.notify();
        }
    }
}

/// "Ctrl+Alt+Space" from a GPUI keystroke; None unless it has Ctrl or Alt and a usable key.
pub fn shortcut_from(m: &gpui::Modifiers, key: &str) -> Option<String> {
    if !(m.control || m.alt) {
        return None;
    }
    let key = match key {
        "space" => "Space".to_string(),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => k.to_uppercase(),
        k if k.starts_with('f') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => k.to_uppercase(),
        _ => return None,
    };
    let mut parts = Vec::new();
    if m.control {
        parts.push("Ctrl");
    }
    if m.alt {
        parts.push("Alt");
    }
    if m.shift {
        parts.push("Shift");
    }
    let mut s = parts.join("+");
    s.push('+');
    s.push_str(&key);
    s.parse::<global_hotkey::hotkey::HotKey>().ok().map(|_| s)
}

// ── building blocks ───────────────────────────────────────────────────────

fn icon(c: char, size: f32) -> gpui::Div {
    div().font_family("Segoe Fluent Icons").text_size(px(size)).child(c.to_string())
}

fn card() -> gpui::Div {
    div().flex().items_center().gap(px(16.)).px(px(16.)).py(px(12.)).min_h(px(64.)).bg(rgb(0xFFFFFF)).border_1().border_color(rgba(CARD_BORDER)).rounded(px(4.))
}

fn section(title: impl Into<SharedString>) -> gpui::Div {
    div().mt(px(24.)).mb(px(8.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title.into())
}

fn wbtn(id: impl Into<gpui::ElementId>, label: impl Into<SharedString>, accent: bool) -> gpui::Stateful<gpui::Div> {
    let (bg, fg, hover) = if accent { (INK, 0xFFFFFF, 0x7C4125) } else { (0xFFFFFF, TEXT, 0xF6F4F1) };
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(32.))
        .px(px(12.))
        .rounded(px(4.))
        .border_1()
        .border_color(rgba(0x0000001A))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_size(px(14.))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(hover)))
        .child(label.into())
}

fn keycaps(shortcut: &str) -> gpui::Div {
    div().flex().gap(px(4.)).children(shortcut.split('+').map(|k| {
        div()
            .h(px(24.))
            .min_w(px(24.))
            .px(px(7.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_b_2()
            .border_color(rgba(0x0000001F))
            .text_size(px(12.))
            .child(k.to_string())
    }))
}

fn toggle(id: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    let knob = div().size(px(12.)).rounded_full().bg(rgb(if on { 0xFFFFFF } else { 0x5F5F5F }));
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .cursor_pointer()
        .text_size(px(14.))
        .child(if on { "On" } else { "Off" })
        .child(
            div()
                .w(px(40.))
                .h(px(20.))
                .rounded(px(10.))
                .px(px(4.))
                .flex()
                .items_center()
                .when(on, |d| d.justify_end().bg(rgb(INK)))
                .when(!on, |d| d.bg(rgb(0xFFFFFF)).border_1().border_color(rgba(0x0000008A)))
                .child(knob),
        )
}

/// A coloured square with the app's initial (no emoji, no third-party logos).
fn app_badge(app: &str, size: f32) -> gpui::Div {
    const COLORS: [u32; 8] = [0x25D366, 0x4A154B, 0xEA4335, 0x0078D4, 0x8F4C2C, 0x5B5FC7, 0x107C10, 0xC239B3];
    let h = app.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let letter = app.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(6.))
        .bg(rgb(COLORS[(h % 8) as usize]))
        .text_color(rgb(0xFFFFFF))
        .text_size(px(size * 0.5))
        .font_weight(FontWeight::SEMIBOLD)
        .flex()
        .items_center()
        .justify_center()
        .child(letter)
}

enum Bar {
    Error,
    Warning,
}

fn info_bar(kind: Bar, title: &str, body: &str, action: Option<AnyElement>, close: Option<AnyElement>) -> gpui::Div {
    let (bg, dot) = match kind {
        Bar::Error => (0xFDE7E9, 0xC42B1C),
        Bar::Warning => (0xFFF4CE, 0x9D5D00),
    };
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .mt(px(8.))
        .pl(px(14.))
        .pr(px(8.))
        .py(px(12.))
        .rounded(px(4.))
        .border_1()
        .border_color(rgba(CARD_BORDER))
        .bg(rgb(bg))
        .child(
            div()
                .mt(px(2.))
                .size(px(16.))
                .flex_none()
                .rounded_full()
                .bg(rgb(dot))
                .text_color(rgb(0xFFFFFF))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(if matches!(kind, Bar::Error) { I_CLOSE } else { '!' }, 8.)),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(14.))
                .line_height(px(20.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                .child(div().child(body.to_string())),
        )
        .children(action)
        .children(close)
}

fn time_label(e: &store::Entry) -> String {
    e.local_time().format("%H:%M").to_string()
}

fn secs_label(secs: f32) -> String {
    let s = secs.round() as u32;
    if s < 60 { format!("{s} second{}", if s == 1 { "" } else { "s" }) } else { format!("{}:{:02}", s / 60, s % 60) }
}

// ── pages ────────────────────────────────────────────────────────────────

impl Home {
    fn nav_item(&self, id: &'static str, glyph: char, label: &'static str, page: Page, cx: &mut Context<Self>) -> AnyElement {
        let on = self.page == page;
        div()
            .id(id)
            .relative()
            .h(px(36.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(14.))
            .rounded(px(4.))
            .text_size(px(14.))
            .cursor_pointer()
            .when(on, |d| d.bg(rgba(0x0000000E)))
            .hover(|s| s.bg(rgba(0x00000009)))
            .when(on, |d| d.child(div().absolute().left_0().top(px(9.)).w(px(3.)).h(px(18.)).rounded(px(2.)).bg(rgb(INK))))
            .child(icon(glyph, 16.))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(page, cx)))
            .into_any_element()
    }

    fn history(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.search.clone();
        let (r, shortcut, waiting, waiting_live, entries) = {
            let v = self.voxa.read(cx);
            let waiting: Vec<u64> = v
                .history
                .entries
                .iter()
                .filter(|e| matches!(e.status, Status::Waiting(_) | Status::Failed(_)) && !v.is_live(e.id) && e.audio.as_ref().is_some_and(|a| a.exists()))
                .map(|e| e.id)
                .collect();
            let waiting_live = v.history.entries.iter().filter(|e| matches!(e.status, Status::Waiting(_)) && v.is_live(e.id)).count();
            let entries: Vec<store::Entry> = v.history.search(&query).into_iter().cloned().collect();
            (v.flow.readiness(), v.settings.hotkey.clone(), waiting, waiting_live, entries)
        };
        let mut out: Vec<AnyElement> = vec![div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("History").into_any_element()];
        if r.session == Session::Expired && !self.dismissed.contains(&"signin") {
            let body = if waiting_live > 0 {
                format!("Voxa can't write anything until you sign in. {waiting_live} dictation{} waiting and will be written out when you do.", if waiting_live == 1 { " is" } else { "s are" })
            } else {
                "Voxa can't write anything until you sign in.".into()
            };
            out.push(
                info_bar(
                    Bar::Error,
                    "You're signed out of ChatGPT.",
                    &body,
                    Some(wbtn("ib-signin", "Sign in", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx)))).into_any_element()),
                    None,
                )
                .into_any_element(),
            );
        }
        match r.blocker() {
            Some(Blocker::MicDenied) | Some(Blocker::MicMissing) if !self.dismissed.contains(&"mic") => {
                let missing = r.blocker() == Some(Blocker::MicMissing);
                out.push(
                    info_bar(
                        Bar::Error,
                        if missing { "No microphone found." } else { "Voxa can't use the microphone." },
                        if missing { "Plug one in, or turn on the built-in one in Windows settings." } else { "Allow microphone access for desktop apps in Windows settings." },
                        Some(wbtn("ib-mic", "Open settings", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.open_mic_settings(cx)))).into_any_element()),
                        Some(self.close_btn("mic", cx)),
                    )
                    .into_any_element(),
                );
            }
            _ => {}
        }
        if r.network == Network::Down && !self.dismissed.contains(&"offline") {
            out.push(info_bar(Bar::Warning, "You're offline.", "You can still dictate. Voxa keeps what you say and sends it when you're back.", None, Some(self.close_btn("offline", cx))).into_any_element());
        }
        if !waiting.is_empty() && r.session != Session::Expired && !self.dismissed.contains(&"waiting") {
            let n = waiting.len();
            out.push(
                info_bar(
                    Bar::Warning,
                    &format!("{n} dictation{} not written out yet.", if n == 1 { " was" } else { "s were" }),
                    "The recording is kept on this PC. Voxa can send it now; the text will be ready to copy.",
                    Some(wbtn("ib-send", "Send now", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.resend_all(cx)))).into_any_element()),
                    Some(self.close_btn("waiting", cx)),
                )
                .into_any_element(),
            );
        }

        // One-line ready status.
        let ready = r.blocker().is_none() && r.session == Session::Valid;
        let status = if ready {
            div().flex().items_center().gap(px(10.)).child(icon(I_CHECK, 14.).text_color(rgb(0x0F7B0F))).child("Ready. Press").child(keycaps(&shortcut)).child("in any app to dictate.")
        } else if r.session == Session::Unknown {
            div().child("Checking your ChatGPT sign-in...")
        } else {
            div().flex().items_center().gap(px(10.)).child("Shortcut:").child(keycaps(&shortcut))
        };
        out.push(div().mt(px(14.)).text_size(px(14.)).text_color(rgb(0x3D3D3D)).child(status).into_any_element());

        // Days, newest first, with search.
        let today = chrono::Local::now().date_naive();
        let search_box = div()
            .id("search")
            .h(px(32.))
            .w(px(280.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(4.))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgba(if self.search_active { 0x8F4C2CFF } else { 0x00000014 }))
            .border_b_2()
            .text_size(px(14.))
            .font_weight(FontWeight::NORMAL)
            .cursor_text()
            .child(icon(I_SEARCH, 12.).text_color(rgb(MUTED)))
            .child(if query.is_empty() && !self.search_active {
                div().text_color(rgb(MUTED)).child("Search history")
            } else {
                div().text_color(rgb(TEXT)).child(format!("{query}{}", if self.search_active { "|" } else { "" }))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.search_active = true;
                window.focus(&this.focus);
                cx.notify();
            }));

        if entries.is_empty() {
            out.push(section("").flex().justify_end().child(search_box).into_any_element());
            let msg = if !query.is_empty() {
                format!("Nothing matches \"{query}\".")
            } else {
                format!("Nothing yet. Press {shortcut} in any app, talk, and what you say shows up here.")
            };
            out.push(div().mt(px(40.)).text_size(px(14.)).text_color(rgb(MUTED)).child(msg).into_any_element());
            return out;
        }
        // The search box sits on the first day's header row.
        let mut search_box = Some(search_box);
        let mut last_day = None;
        for e in entries.iter().take(300) {
            let d = e.local_time().date_naive();
            if last_day != Some(d) {
                let mut s = section(store::day_label(d, today)).flex().items_center().justify_between();
                if let Some(sb) = search_box.take() {
                    s = s.child(sb);
                }
                out.push(s.into_any_element());
                last_day = Some(d);
            }
            out.push(self.history_row(e, cx));
        }
        out
    }

    fn close_btn(&self, key: &'static str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(SharedString::from(format!("close-{key}")))
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0x0000000F)))
            .text_color(rgb(MUTED))
            .child(icon(I_CLOSE, 10.))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.dismissed.push(key);
                cx.notify();
            }))
            .into_any_element()
    }

    fn history_row(&self, e: &store::Entry, cx: &mut Context<Self>) -> AnyElement {
        let id = e.id;
        let (body, muted) = match &e.status {
            Status::Pasted | Status::Ready => (e.text.clone(), false),
            Status::Waiting(why) => (format!("{} of speech. {why}.", secs_label(e.secs)), true),
            Status::Failed(why) => (format!("{} of speech. {why}.", secs_label(e.secs)), true),
        };
        let action = match &e.status {
            Status::Pasted | Status::Ready => {
                let copied = self.copied == Some(id);
                let text = e.text.clone();
                Some(
                    wbtn(SharedString::from(format!("copy-{id}")), if copied { "Copied" } else { "Copy" }, false)
                        .h(px(28.))
                        .text_size(px(13.))
                        .child(icon(if copied { I_CHECK } else { I_COPY }, 12.))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            this.copied = Some(id);
                            cx.notify();
                        }))
                        .into_any_element(),
                )
            }
            _ if e.audio.as_ref().is_some_and(|a| a.exists()) && !self.voxa.read(cx).is_live(id) => Some(
                wbtn(SharedString::from(format!("send-{id}")), "Send", false)
                    .h(px(28.))
                    .text_size(px(13.))
                    .child(icon(I_SEND, 12.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.resend(id, cx))))
                    .into_any_element(),
            ),
            _ => None,
        };
        let note = match &e.status {
            Status::Ready => Some("Not pasted"),
            _ => None,
        };
        div()
            .flex()
            .items_start()
            .gap(px(14.))
            .px(px(14.))
            .py(px(12.))
            .mb(px(3.))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgba(CARD_BORDER))
            .rounded(px(4.))
            .child(app_badge(&e.app, 28.).mt(px(2.)))
            .child(
                div()
                    .w(px(120.))
                    .flex_none()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(rgb(MUTED))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)).child(time_label(e)))
                    .child(e.app.clone())
                    .children(note.map(|n| div().text_color(rgb(0x9D5D00)).child(n))),
            )
            .child(div().flex_1().min_w(px(0.)).text_size(px(14.)).line_height(px(21.)).text_color(rgb(if muted { MUTED } else { TEXT })).child(body))
            .children(action)
            .into_any_element()
    }

    fn insights(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let v = self.voxa.read(cx);
        let i = store::insights(&v.history.entries, chrono::Local::now());
        let stat = |n: String, l: &str| {
            div()
                .flex_1()
                .px(px(18.))
                .py(px(16.))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgba(CARD_BORDER))
                .rounded(px(4.))
                .child(div().text_size(px(34.)).line_height(px(40.)).font_weight(FontWeight::SEMIBOLD).child(n))
                .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(l.to_string()))
        };
        let max = i.per_day.iter().copied().max().unwrap_or(0).max(1) as f32;
        let bars = div().flex().items_end().gap(px(18.)).h(px(150.)).children(i.per_day.iter().enumerate().map(|(d, w)| {
            let h = if *w == 0 { 2.0 } else { 6.0 + 144.0 * (*w as f32 / max) };
            div().flex_1().h(px(h)).rounded_t(px(3.)).bg(rgb(if d == i.today { INK } else { 0xE7E1DA }))
        }));
        let labels = div().flex().gap(px(18.)).mt(px(8.)).children(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().map(|d| {
            div().flex_1().flex().justify_center().text_size(px(12.)).text_color(rgb(MUTED)).child(d.to_string())
        }));
        let mut out: Vec<AnyElement> = vec![
            div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("Insights").into_any_element(),
            div().mt(px(2.)).text_size(px(14.)).text_color(rgb(MUTED)).child(i.week_label.clone()).into_any_element(),
            div()
                .mt(px(18.))
                .flex()
                .gap(px(3.))
                .child(stat(fmt_thousands(i.week_words), "words this week"))
                .child(stat(if i.minutes_saved >= 60 { format!("{}h {}m", i.minutes_saved / 60, i.minutes_saved % 60) } else { format!("{} min", i.minutes_saved) }, "faster than typing"))
                .child(stat(format!("{} day{}", i.streak_days, if i.streak_days == 1 { "" } else { "s" }), "in a row"))
                .into_any_element(),
            div()
                .mt(px(3.))
                .px(px(20.))
                .py(px(18.))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgba(CARD_BORDER))
                .rounded(px(4.))
                .child(div().mb(px(14.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Words per day"))
                .child(bars)
                .child(labels)
                .into_any_element(),
        ];
        if !i.per_app.is_empty() {
            out.push(section("Where you dictate").into_any_element());
            for (app, words) in i.per_app.iter().take(8) {
                out.push(
                    card()
                        .min_h(px(52.))
                        .py(px(10.))
                        .mb(px(3.))
                        .child(app_badge(app, 28.))
                        .child(div().flex_1().text_size(px(14.)).child(app.clone()))
                        .child(div().text_size(px(14.)).text_color(rgb(MUTED)).child(format!("{} words", fmt_thousands(*words))))
                        .into_any_element(),
                );
            }
        }
        out.push(div().mt(px(16.)).text_size(px(12.)).text_color(rgb(MUTED)).child(format!("\"Faster than typing\" compares your speaking time with typing the same words at {} words a minute.", store::TYPING_WPM as u32)).into_any_element());
        out
    }

    fn settings(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let v = self.voxa.read(cx);
        let s = v.settings.clone();
        let session = v.flow.readiness().session;
        let mic = v.mic_name.clone();
        let autostart = v.start_with_windows;
        let count = v.history.entries.len();
        let mb = v.history.audio_bytes() as f64 / 1e6;
        let row_text = |h: &str, sub: String| {
            div().flex_1().child(div().text_size(px(14.)).line_height(px(20.)).child(h.to_string())).child(div().mt(px(1.)).text_size(px(12.)).line_height(px(16.)).text_color(rgb(MUTED)).child(sub))
        };
        let mut out: Vec<AnyElement> = vec![div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("Settings").into_any_element()];

        out.push(section("Account").mt(px(16.)).into_any_element());
        let (acct, btn) = match session {
            Session::Valid => (
                "Signed in. Voxa sends what you say while dictating to ChatGPT to turn it into text. Nothing else leaves this PC.".to_string(),
                wbtn("signout", "Sign out", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_out(cx)))),
            ),
            Session::Expired => (
                "Signed out. Voxa sends what you say while dictating to ChatGPT to turn it into text.".to_string(),
                wbtn("signin", "Sign in", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx)))),
            ),
            Session::Unknown => ("Checking your sign-in...".to_string(), wbtn("signin", "Sign in", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx))))),
        };
        out.push(card().mb(px(3.)).child(icon(I_ACCOUNT, 18.).w(px(20.))).child(row_text("ChatGPT", acct)).child(btn).into_any_element());

        out.push(section("Dictation").into_any_element());
        let shortcut_right = if self.capturing {
            div().flex().items_center().gap(px(10.)).text_size(px(14.)).text_color(rgb(INK)).child("Press the new shortcut...").child(
                wbtn("cancel-capture", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.stop_capture(cx);
                    cx.notify();
                })),
            )
        } else {
            div().flex().items_center().gap(px(12.)).child(keycaps(&s.hotkey)).child(wbtn("change-hotkey", "Change", false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.capturing = true;
                this.hotkey_error = None;
                this.voxa.update(cx, |v, _| v.pause_hotkey(true));
                window.focus(&this.focus);
                cx.notify();
            })))
        };
        let sub = self.hotkey_error.clone().unwrap_or_else(|| "Press to start, press again to stop. Esc cancels while recording.".into());
        out.push(card().mb(px(3.)).child(icon(I_KEYBOARD, 18.).w(px(20.))).child(row_text("Shortcut", sub)).child(shortcut_right).into_any_element());
        out.push(card().mb(px(3.)).child(icon(I_MIC, 18.).w(px(20.))).child(row_text("Microphone", "Voxa uses the Windows default microphone. Change it in Windows sound settings.".into())).child(div().text_size(px(14.)).text_color(rgb(MUTED)).max_w(px(260.)).child(mic)).into_any_element());
        out.push(
            card()
                .mb(px(3.))
                .child(icon(I_TEXT, 18.).w(px(20.)))
                .child(row_text("Remove filler words", "Takes out \"um\", \"uh\" and \"hmm\". Off keeps your words exactly as spoken.".into()))
                .child(toggle("fillers", s.remove_fillers).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.update_settings(|s| s.remove_fillers = !s.remove_fillers, cx)))))
                .into_any_element(),
        );

        out.push(section("Privacy").into_any_element());
        out.push(
            card()
                .mb(px(3.))
                .child(icon(I_KEEP, 18.).w(px(20.)))
                .child(row_text("Keep history", "Text and audio stay on this PC. Older dictations are deleted.".into()))
                .child(
                    div()
                        .id("keep")
                        .h(px(32.))
                        .min_w(px(150.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.))
                        .rounded(px(4.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_b_2()
                        .border_color(rgba(0x00000020))
                        .text_size(px(14.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0xF6F4F1)))
                        .child(Settings::keep_label(s.keep_days))
                        .child(icon(I_DOWN, 10.).text_color(rgb(MUTED)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.voxa.update(cx, |v, cx| {
                                let c = Settings::KEEP_CHOICES;
                                let next = c[(c.iter().position(|d| *d == v.settings.keep_days).unwrap_or(0) + 1) % c.len()];
                                v.update_settings(|s| s.keep_days = next, cx);
                            })
                        })),
                )
                .into_any_element(),
        );
        let delete = if self.confirm_delete {
            div()
                .flex()
                .gap(px(8.))
                .child(wbtn("delete-cancel", "Keep", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.confirm_delete = false;
                    cx.notify();
                })))
                .child(wbtn("delete-confirm", "Delete everything", true).bg(rgb(0xC42B1C)).hover(|s| s.bg(rgb(0xA32416))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.confirm_delete = false;
                    this.voxa.update(cx, |v, cx| v.delete_history(cx));
                })))
        } else {
            div().child(wbtn("delete", "Delete all", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.confirm_delete = true;
                cx.notify();
            })))
        };
        out.push(
            card()
                .mb(px(3.))
                .child(icon(I_DELETE, 18.).w(px(20.)))
                .child(row_text("Delete history", format!("{count} dictation{}, {mb:.1} MB", if count == 1 { "" } else { "s" })))
                .child(delete)
                .into_any_element(),
        );

        out.push(section("System").into_any_element());
        out.push(
            card()
                .mb(px(3.))
                .child(icon(I_POWER, 18.).w(px(20.)))
                .child(row_text("Start with Windows", "Starts quietly in the tray when you sign in".into()))
                .child(toggle("autostart", autostart).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.set_autostart(!v.start_with_windows, cx)))))
                .into_any_element(),
        );
        out.push(
            div()
                .mt(px(16.))
                .flex()
                .items_center()
                .gap(px(12.))
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(format!("Voxa {}", env!("CARGO_PKG_VERSION")))
                .child(div().id("rerun-setup").text_color(rgb(INK)).cursor_pointer().child("Run setup again").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Setup(1), cx))))
                .into_any_element(),
        );
        out
    }

    fn setup(&mut self, step: u8, cx: &mut Context<Self>) -> AnyElement {
        let v = self.voxa.read(cx);
        let shortcut = v.settings.hotkey.clone();
        let session = v.flow.readiness().session;
        let p = |t: String| div().mt(px(10.)).text_size(px(14.)).line_height(px(21.)).text_color(rgb(0x3D3D3D)).child(t);
        let h2 = |t: &str| div().mt(px(10.)).text_size(px(24.)).line_height(px(32.)).font_weight(FontWeight::SEMIBOLD).child(t.to_string());
        let mut body: Vec<AnyElement> = vec![div().text_size(px(12.)).text_color(rgb(MUTED)).child(format!("Step {step} of 4")).into_any_element()];
        let mut can_next = true;
        match step {
            1 => {
                body.push(h2("Voxa writes through your ChatGPT account").into_any_element());
                body.push(p("Press a shortcut in any app, talk, and Voxa pastes the text where your cursor is. Your voice goes to ChatGPT to become text; your history stays on this PC.".into()).into_any_element());
                let state = match session {
                    Session::Valid => div().flex().items_center().gap(px(8.)).text_color(rgb(0x0F7B0F)).child(icon(I_CHECK, 14.)).child("Signed in to ChatGPT"),
                    _ => div().flex().items_center().gap(px(12.)).child(wbtn("setup-signin", "Sign in to ChatGPT", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx))))).child(
                        div().text_size(px(13.)).text_color(rgb(MUTED)).child(if session == Session::Unknown { "Checking..." } else { "A sign-in window opens" }),
                    ),
                };
                body.push(div().mt(px(20.)).text_size(px(14.)).child(state).into_any_element());
            }
            2 => {
                body.push(h2("Say something").into_any_element());
                body.push(p("If the line moves, Voxa can hear you. Nothing is recorded or sent on this step.".into()).into_any_element());
                let mic = v.mic_name.clone();
                let levels: Vec<f32> = v.levels.iter().copied().collect();
                let tick = levels.len().saturating_sub(1);
                body.push(
                    div()
                        .mt(px(20.))
                        .px(px(16.))
                        .py(px(12.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgba(CARD_BORDER))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .child(icon(I_MIC, 18.))
                        .child(crate::ink_line(levels, tick))
                        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(mic))
                        .into_any_element(),
                );
            }
            3 => {
                body.push(h2("Try it").into_any_element());
                body.push(
                    p(String::new())
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child("Open any app with a text box, like Notepad. Click in it, press")
                        .child(keycaps(&shortcut))
                        .child(", say a sentence, then press it again.")
                        .into_any_element(),
                );
                let first = v.history.entries.iter().rev().find(|e| e.id > self.try_since && matches!(e.status, Status::Pasted | Status::Ready));
                body.push(match first {
                    Some(e) => div()
                        .mt(px(20.))
                        .px(px(16.))
                        .py(px(12.))
                        .bg(rgb(0xDFF6DD))
                        .rounded(px(4.))
                        .flex()
                        .gap(px(10.))
                        .text_size(px(14.))
                        .child(icon(I_CHECK, 14.).text_color(rgb(0x0F7B0F)).mt(px(3.)))
                        .child(div().flex_1().child(format!("It worked. Voxa wrote \"{}\" into {}.", e.text, e.app)))
                        .into_any_element(),
                    None => div().mt(px(20.)).text_size(px(14.)).text_color(rgb(MUTED)).child("Waiting for your first dictation. You can also skip this.").into_any_element(),
                });
            }
            _ => {
                body.push(h2("You're set").into_any_element());
                body.push(p(format!("Voxa lives in the tray. Press {shortcut} in any app to dictate. Open this window from the tray icon to see your history.")).into_any_element());
                let autostart = v.start_with_windows;
                body.push(
                    card()
                        .mt(px(20.))
                        .child(icon(I_POWER, 18.).w(px(20.)))
                        .child(div().flex_1().text_size(px(14.)).child("Start Voxa when I sign in to Windows"))
                        .child(toggle("setup-autostart", autostart).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.set_autostart(!v.start_with_windows, cx)))))
                        .into_any_element(),
                );
                can_next = true;
            }
        }
        let next_label = if step >= 4 { "Finish" } else { "Next" };
        let buttons = div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .px(px(32.))
            .py(px(20.))
            .bg(rgb(NAV_BG))
            .border_t_1()
            .border_color(rgba(CARD_BORDER))
            .when(step > 1, |d| d.child(wbtn("setup-back", "Back", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(Page::Setup(step - 1), cx)))))
            .when(step < 4, |d| d.child(wbtn("setup-skip", "Skip setup", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finish_setup(cx)))))
            .when(can_next, |d| {
                d.child(wbtn("setup-next", next_label, true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if step >= 4 { this.finish_setup(cx) } else { this.go(Page::Setup(step + 1), cx) }
                })))
            });
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(NAV_BG))
            .child(
                div()
                    .w(px(640.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(rgb(PAGE_BG))
                    .border_1()
                    .border_color(rgba(0x0000001A))
                    .shadow_lg()
                    .child(div().px(px(32.)).pt(px(28.)).pb(px(24.)).min_h(px(300.)).children(body))
                    .child(buttons),
            )
            .into_any_element()
    }

    fn finish_setup(&mut self, cx: &mut Context<Self>) {
        self.voxa.update(cx, |v, cx| v.update_settings(|s| s.onboarded = true, cx));
        self.go(Page::History, cx);
    }
}

fn fmt_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl Render for Home {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .font_family("Segoe UI")
            .text_color(rgb(TEXT))
            .bg(rgb(NAV_BG));
        if let Page::Setup(step) = self.page {
            return root.child(self.setup(step, cx));
        }
        let content = match self.page {
            Page::History => self.history(cx),
            Page::Insights => self.insights(cx),
            _ => self.settings(cx),
        };
        let nav = div()
            .w(px(240.))
            .flex_none()
            .h_full()
            .px(px(6.))
            .pt(px(4.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(div().px(px(12.)).pt(px(8.)).pb(px(14.)).font_family("Fraunces NonWonky").font_weight(FontWeight::SEMIBOLD).text_size(px(24.)).child("voxa"))
            .child(self.nav_item("nav-history", I_HISTORY, "History", Page::History, cx))
            .child(self.nav_item("nav-insights", I_INSIGHTS, "Insights", Page::Insights, cx))
            .child(div().flex_1())
            .child(self.nav_item("nav-settings", I_SETTINGS, "Settings", Page::Settings, cx));
        root.flex().child(nav).child(
            div()
                .id("page")
                .flex_1()
                // Without this the column grows to its widest row and runs off the window.
                .min_w(px(0.))
                .h_full()
                .overflow_y_scroll()
                .bg(rgb(PAGE_BG))
                .rounded_tl(px(8.))
                .border_t_1()
                .border_l_1()
                .border_color(rgba(CARD_BORDER))
                .px(px(40.))
                .pt(px(28.))
                .pb(px(30.))
                .children(content),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_need_ctrl_or_alt_and_a_real_key() {
        let m = |control, alt, shift| gpui::Modifiers { control, alt, shift, ..Default::default() };
        assert_eq!(shortcut_from(&m(true, true, false), "space").as_deref(), Some("Ctrl+Alt+Space"));
        assert_eq!(shortcut_from(&m(true, false, true), "d").as_deref(), Some("Ctrl+Shift+D"));
        assert_eq!(shortcut_from(&m(false, true, false), "f9").as_deref(), Some("Alt+F9"));
        assert_eq!(shortcut_from(&m(false, false, true), "d"), None, "shift alone would hijack typing");
        assert_eq!(shortcut_from(&m(true, false, false), "escape"), None);
    }

    #[test]
    fn thousands() {
        assert_eq!(fmt_thousands(2418), "2,418");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(fmt_thousands(1_000_000), "1,000,000");
    }
}
