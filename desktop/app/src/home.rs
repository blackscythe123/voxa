//! The Voxa window (docs/ui-mocks/v7.1-native.html, "Windows · the window"): History is the home
//! page, with what needs you on top as InfoBars, then a one-line ready status, then the days.
//! Insights and Settings are their own pages. A guided setup runs on first launch.

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, Render, SharedString, Window, div,
    prelude::*, px, rgb, rgba,
};

use crate::store::{self, Settings, Status};
use crate::INK;
use crate::input::TextInput;
use crate::remote::Backend as Voxa;
use voxa_core::{Blocker, Network, Session};

const TEXT: u32 = 0x1A1A1A;
const MUTED: u32 = 0x5F5F5F;
const PAGE_BG: u32 = 0xFBFAF8;
const NAV_BG: u32 = 0xF1EEEA;
const CARD_BORDER: u32 = 0x0000000F;

// Segoe Fluent Icons code points.
const I_HISTORY: char = '\u{E81C}';
const I_HOME: char = '\u{E80F}';
const I_WORDS: char = '\u{E82D}';
const I_EDIT: char = '\u{E70F}';
const I_BACK: char = '\u{E72B}';
const I_ARROW: char = '\u{E72A}';
const I_PREV: char = '\u{E76B}';
const I_NEXT: char = '\u{E76C}';
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
const I_PLAY: char = '\u{E768}';
const I_STOP: char = '\u{E71A}';
const I_RETRY: char = '\u{E72C}';
const I_FOLDER: char = '\u{E838}';
const I_SORT: char = '\u{E8CB}';

pub const SETUP_STEPS: u8 = 6;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Home,
    History,
    Insights,
    Settings,
    Words,
    /// One dictation, opened from History or Home.
    Detail(u64),
    /// Guided setup (the first-run tour), steps 1 to SETUP_STEPS.
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
    app_filter: Option<String>,
    status_filter: StatusFilter,
    oldest_first: bool,
    /// Entry whose recording is playing.
    playing: Option<u64>,
    /// Entry whose Delete was pressed once (press again to delete).
    confirm_entry: Option<u64>,
    /// Start with Windows as chosen on the last setup step (On unless turned off).
    autostart_choice: Option<bool>,
    range: Range,
    /// Custom range (local dates).
    custom: (chrono::NaiveDate, chrono::NaiveDate),
    fix_from: Entity<TextInput>,
    fix_to: Entity<TextInput>,
    snip_from: Entity<TextInput>,
    snip_to: Entity<TextInput>,
    edit_box: Entity<TextInput>,
    /// The dictation being edited on its detail page.
    editing: Option<u64>,
    /// After an edit: offer to always fix what changed (from, to).
    suggestion: Option<(String, String)>,
    /// When playback started, to draw progress.
    play_started: Option<(std::time::Instant, f32)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Range {
    Days7,
    Days30,
    ThisMonth,
    Days90,
    ThisYear,
    All,
    Custom,
}

impl Range {
    const ALL: [(Range, &'static str); 7] = [
        (Range::Days7, "7 days"),
        (Range::Days30, "30 days"),
        (Range::ThisMonth, "This month"),
        (Range::Days90, "90 days"),
        (Range::ThisYear, "This year"),
        (Range::All, "All time"),
        (Range::Custom, "Custom"),
    ];
}

#[derive(Clone, Copy, PartialEq)]
enum StatusFilter {
    All,
    NotPasted,
    NotSent,
}

impl Home {
    pub fn new(voxa: Entity<Voxa>, page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&voxa, |_, _, cx| cx.notify()).detach();
        let input = |ph: &str, cx: &mut Context<Self>| cx.new(|cx| TextInput::new(ph, cx));
        let (fix_from, fix_to) = (input("When it writes", cx), input("Write instead", cx));
        let (snip_from, snip_to) = (input("When I say", cx), input("Type this", cx));
        let edit_box = input("", cx);
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
            app_filter: None,
            status_filter: StatusFilter::All,
            oldest_first: false,
            playing: None,
            confirm_entry: None,
            autostart_choice: None,
            range: Range::Days7,
            custom: {
                let t = chrono::Local::now().date_naive();
                (t - chrono::Duration::days(29), t)
            },
            fix_from,
            fix_to,
            snip_from,
            snip_to,
            edit_box,
            editing: None,
            suggestion: None,
            play_started: None,
        };
        me.go(page, cx);
        me
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.capturing {
            self.stop_capture(cx);
        }
        let testing = matches!(page, Page::Setup(3));
        self.voxa.update(cx, |v, cx| v.mic_test(testing, cx));
        if page == Page::Setup(4) {
            self.try_since = self.voxa.read(cx).history.entries.last().map(|e| e.id).unwrap_or(0);
        }
        self.confirm_delete = false;
        if !matches!(page, Page::Detail(_)) {
            self.editing = None;
            self.suggestion = None;
        }
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

/// The app's badge, with the browser's icon in the corner when the app is a site in a tab.
fn app_badge_via(app: &str, via: Option<&str>, size: f32) -> gpui::Div {
    let base = app_badge(app, size);
    match via.map(crate::os::icon_file).filter(|f| f.exists()) {
        Some(f) => div().relative().size(px(size)).flex_none().child(base).child(
            div()
                .absolute()
                .right(px(-5.))
                .bottom(px(-5.))
                .size(px(size * 0.55))
                .rounded_full()
                .bg(rgb(0xFFFFFF))
                .flex()
                .items_center()
                .justify_center()
                .child(gpui::img(f).size(px(size * 0.45))),
        ),
        None => base,
    }
}

/// The app's real icon (cached from its exe), or a coloured square with its initial.
fn app_badge(app: &str, size: f32) -> gpui::Div {
    let file = crate::os::icon_file(app);
    if file.exists() {
        return div().size(px(size)).flex_none().child(gpui::img(file).size(px(size)));
    }
    const COLORS: [u32; 10] = [0x0078D4, 0x8F4C2C, 0x107C10, 0xC239B3, 0x5B5FC7, 0xCA5010, 0x038387, 0x8764B8, 0x486860, 0xE3008C];
    let known = [
        ("whatsapp", 0x25D366), ("slack", 0x4A154B), ("gmail", 0xEA4335), ("outlook", 0x0F6CBD), ("vs code", 0x007ACC), ("visual studio", 0x68217A),
        ("notion", 0x191919), ("chrome", 0x1A73E8), ("edge", 0x0C59A4), ("firefox", 0xE66000), ("teams", 0x5B5FC7), ("discord", 0x5865F2),
        ("word", 0x185ABD), ("excel", 0x107C41), ("powerpoint", 0xC43E1C), ("notepad", 0x2B579A), ("telegram", 0x229ED9), ("chatgpt", 0x10A37F),
        ("claude", 0xD97757), ("youtube", 0xFF0000), ("github", 0x24292F), ("linkedin", 0x0A66C2), ("google docs", 0x4285F4), ("google sheets", 0x0F9D58),
        ("google meet", 0x00897B), ("reddit", 0xFF4500), ("figma", 0xF24E1E), ("instagram", 0xE1306C), ("jira", 0x0052CC), ("messenger", 0x0084FF),
        ("gemini", 0x4285F4), ("perplexity", 0x20808D), ("brave", 0xFB542B),
    ];
    let lower = app.to_lowercase();
    let brand = known.iter().find(|(k, _)| lower.contains(k)).map(|(_, c)| *c);
    let h = app.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let letter = app.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(6.))
        .bg(rgb(brand.unwrap_or(COLORS[(h % 10) as usize])))
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
        let (r, shortcut, _waiting, _waiting_live, entries) = {
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
            (v.readiness(), v.settings.hotkey.clone(), waiting, waiting_live, entries)
        };
        let mut out: Vec<AnyElement> = vec![div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("History").into_any_element()];
        out.extend(self.needs_you(cx));
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

        // Toolbar: search, sort, app and status filters.
        let today = chrono::Local::now().date_naive();
        let all_count = entries.len();
        let mut apps: Vec<(String, usize)> = Vec::new();
        for e in &entries {
            match apps.iter_mut().find(|(a, _)| *a == e.app) {
                Some((_, n)) => *n += 1,
                None => apps.push((e.app.clone(), 1)),
            }
        }
        apps.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut entries: Vec<store::Entry> = entries
            .into_iter()
            .filter(|e| self.app_filter.as_ref().is_none_or(|a| *a == e.app))
            .filter(|e| match self.status_filter {
                StatusFilter::All => true,
                StatusFilter::NotPasted => e.status == Status::Ready,
                StatusFilter::NotSent => matches!(e.status, Status::Waiting(_) | Status::Failed(_)),
            })
            .collect();
        if self.oldest_first {
            entries.reverse();
        }
        let words: usize = entries.iter().map(|e| e.words()).sum();

        let search_box = div()
            .id("search")
            .h(px(32.))
            .w(px(300.))
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
            .cursor_text()
            .child(icon(I_SEARCH, 12.).text_color(rgb(MUTED)))
            .child(if query.is_empty() && !self.search_active {
                div().text_color(rgb(MUTED)).child("Search words or apps")
            } else {
                div().text_color(rgb(TEXT)).child(format!("{query}{}", if self.search_active { "|" } else { "" }))
            })
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.search_active = true;
                window.focus(&this.focus);
                cx.notify();
            }));
        let sort = wbtn("sort", if self.oldest_first { "Oldest first" } else { "Newest first" }, false).child(icon(I_SORT, 12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.oldest_first = !this.oldest_first;
            cx.notify();
        }));
        out.push(
            div()
                .mt(px(22.))
                .flex()
                .items_center()
                .gap(px(10.))
                .child(search_box)
                .child(div().flex_1().text_size(px(13.)).text_color(rgb(MUTED)).child(format!(
                    "{} dictation{}, {} words",
                    entries.len(),
                    if entries.len() == 1 { "" } else { "s" },
                    fmt_thousands(words)
                )))
                .child(sort)
                .into_any_element(),
        );
        if all_count > 0 {
            let mut chips = div().mt(px(10.)).flex().flex_wrap().items_center().gap(px(6.));
            chips = chips.child(self.chip("app-all", "All apps", self.app_filter.is_none(), cx.listener(|this, _: &ClickEvent, _, cx| {
                this.app_filter = None;
                cx.notify();
            })));
            for (i, (app, n)) in apps.iter().take(6).enumerate() {
                let a = app.clone();
                let on = self.app_filter.as_deref() == Some(app.as_str());
                chips = chips.child(self.chip(SharedString::from(format!("app-{i}")), &format!("{app}  {n}"), on, cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.app_filter = if this.app_filter.as_deref() == Some(a.as_str()) { None } else { Some(a.clone()) };
                    cx.notify();
                })));
            }
            chips = chips.child(div().w(px(14.)));
            for (id, label, f) in [("st-all", "Everything", StatusFilter::All), ("st-np", "Not pasted", StatusFilter::NotPasted), ("st-ns", "Not sent", StatusFilter::NotSent)] {
                chips = chips.child(self.chip(id, label, self.status_filter == f, cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.status_filter = f;
                    cx.notify();
                })));
            }
            out.push(chips.into_any_element());
        }

        if entries.is_empty() {
            let msg = if all_count > 0 || !query.is_empty() {
                "Nothing matches these filters.".to_string()
            } else {
                format!("Nothing yet. Press {shortcut} in any app, talk, and what you say shows up here.")
            };
            out.push(div().mt(px(40.)).text_size(px(14.)).text_color(rgb(MUTED)).child(msg).into_any_element());
            return out;
        }
        let mut last_day = None;
        for e in entries.iter().take(400) {
            let d = e.local_time().date_naive();
            if last_day != Some(d) {
                out.push(section(store::day_label(d, today)).into_any_element());
                last_day = Some(d);
            }
            out.push(self.history_row(e, cx));
        }
        out
    }

    /// What needs the user: one InfoBar per problem, each with one action.
    fn needs_you(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (r, waiting, waiting_live) = {
            let v = self.voxa.read(cx);
            let waiting: Vec<u64> = v
                .history
                .entries
                .iter()
                .filter(|e| matches!(e.status, Status::Waiting(_) | Status::Failed(_)) && !v.is_live(e.id) && e.audio.as_ref().is_some_and(|a| a.exists()))
                .map(|e| e.id)
                .collect();
            let waiting_live = v.history.entries.iter().filter(|e| matches!(e.status, Status::Waiting(_)) && v.is_live(e.id)).count();
            (v.readiness(), waiting, waiting_live)
        };
        let mut out: Vec<AnyElement> = Vec::new();
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

        out
    }

    fn chip(&self, id: impl Into<gpui::ElementId>, label: &str, on: bool, f: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static) -> AnyElement {
        div()
            .id(id.into())
            .h(px(28.))
            .px(px(12.))
            .flex()
            .items_center()
            .rounded(px(14.))
            .text_size(px(13.))
            .cursor_pointer()
            .border_1()
            .when(on, |d| d.bg(rgb(INK)).text_color(rgb(0xFFFFFF)).border_color(rgb(INK)))
            .when(!on, |d| d.bg(rgb(0xFFFFFF)).border_color(rgba(0x0000001A)).hover(|s| s.bg(rgb(0xF6F4F1))))
            .child(label.to_string())
            .on_click(f)
            .into_any_element()
    }

    fn icon_btn(&self, id: String, glyph: char, tip: &'static str, danger: bool, f: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static) -> AnyElement {
        div()
            .id(SharedString::from(id))
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .text_color(rgb(if danger { 0xC42B1C } else { 0x3D3D3D }))
            .hover(|s| s.bg(rgba(0x0000000F)))
            .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
            .child(icon(glyph, 13.))
            .on_click(f)
            .into_any_element()
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
        let live = self.voxa.read(cx).is_live(id);
        let has_audio = e.audio.as_ref().is_some_and(|a| a.exists());
        let (body, muted) = match &e.status {
            Status::Pasted | Status::Ready => (e.text.clone(), false),
            Status::Waiting(why) | Status::Failed(why) => (format!("{} of speech. {why}.", secs_label(e.secs)), true),
        };
        let note = match &e.status {
            Status::Ready => Some(("Not pasted", "You had moved to another app, so Voxa kept it here. Copy it, or paste it where you need it.")),
            Status::Failed(_) => Some(("Not sent", "")),
            Status::Waiting(_) => Some(("Waiting", "")),
            Status::Pasted => None,
        };
        let mut actions = div().flex().items_center().gap(px(2.)).flex_none();
        if matches!(e.status, Status::Pasted | Status::Ready) {
            let copied = self.copied == Some(id);
            let text = e.text.clone();
            actions = actions.child(
                wbtn(SharedString::from(format!("copy-{id}")), if copied { "Copied" } else { "Copy" }, false)
                    .h(px(28.))
                    .text_size(px(13.))
                    .mr(px(6.))
                    .child(icon(if copied { I_CHECK } else { I_COPY }, 12.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        this.copied = Some(id);
                        cx.notify();
                    })),
            );
        }
        if has_audio {
            let playing = self.playing == Some(id);
            actions = actions.child(self.icon_btn(format!("play-{id}"), if playing { I_STOP } else { I_PLAY }, if playing { "Stop" } else { "Play the recording" }, false, cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_play(id, cx))));
            let unsent = matches!(e.status, Status::Waiting(_) | Status::Failed(_));
            if !live || unsent {
                let (glyph, tip) = if unsent { (I_SEND, "Send it now") } else { (I_RETRY, "Write it out again") };
                actions = actions.child(self.icon_btn(format!("retry-{id}"), glyph, tip, false, cx.listener(move |this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.resend(id, cx)))));
            }
            actions = actions.child(self.icon_btn(format!("folder-{id}"), I_FOLDER, "Show the recording in its folder", false, cx.listener(move |this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, _| v.show_in_folder(id)))));
        }
        if !live {
            if self.confirm_entry == Some(id) {
                actions = actions.child(
                    wbtn(SharedString::from(format!("del-yes-{id}")), "Delete", true)
                        .h(px(28.))
                        .text_size(px(13.))
                        .bg(rgb(0xC42B1C))
                        .hover(|s| s.bg(rgb(0xA32416)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.confirm_entry = None;
                            if this.playing == Some(id) {
                                this.playing = None;
                                crate::os::stop_audio();
                            }
                            this.voxa.update(cx, |v, cx| v.delete_entry(id, cx));
                        })),
                );
            } else {
                actions = actions.child(self.icon_btn(format!("del-{id}"), I_DELETE, "Delete this dictation and its recording", false, cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.confirm_entry = Some(id);
                    cx.notify();
                })));
            }
        }
        let note_color = match &e.status {
            Status::Failed(_) => 0xC42B1C,
            _ => 0x9D5D00,
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
            .child(app_badge_via(&e.app, e.via.as_deref(), 28.).mt(px(2.)))
            .child(
                div()
                    .w(px(110.))
                    .flex_none()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(rgb(MUTED))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)).child(time_label(e)))
                    .child(e.app.clone())
                    .children({
                        // One line: "in Brave, from Gmail".
                        let from = e.started_in.as_ref().filter(|s| **s != e.app);
                        let line = match (e.via.as_ref(), from) {
                            (Some(b), Some(f)) => Some(format!("in {b}, from {f}")),
                            (Some(b), None) => Some(format!("in {b}")),
                            (None, Some(f)) => Some(format!("from {f}")),
                            _ => None,
                        };
                        line.map(|l| div().child(l))
                    })
                    .children(note.map(|(n, _)| div().text_color(rgb(note_color)).child(n))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("open-{id}")))
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(Page::Detail(id), cx)))
                    .child(div().text_color(rgb(if muted { MUTED } else { TEXT })).hover(|s| s.text_color(rgb(INK))).child(body))
                    .children(note.filter(|(_, why)| !why.is_empty()).map(|(_, why)| div().mt(px(4.)).text_size(px(12.)).line_height(px(16.)).text_color(rgb(MUTED)).child(why))),
            )
            .child(actions)
            .into_any_element()
    }

    fn detail(&mut self, id: u64, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(e) = self.voxa.read(cx).history.get(id).cloned() else {
            return vec![
                div().text_size(px(14.)).text_color(rgb(MUTED)).child("This dictation was deleted.").into_any_element(),
                wbtn("back-gone", "Back to History", false).mt(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::History, cx))).into_any_element(),
            ];
        };
        let live = self.voxa.read(cx).is_live(id);
        let has_audio = e.audio.as_ref().is_some_and(|a| a.exists());
        let when = e.local_time();
        let mut out: Vec<AnyElement> = vec![
            div()
                .id("back")
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .text_color(rgb(INK))
                .cursor_pointer()
                .child(icon(I_BACK, 12.))
                .child("History")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::History, cx)))
                .into_any_element(),
        ];
        // Where and when.
        let place = match (e.via.as_ref(), e.started_in.as_ref().filter(|s| **s != e.app)) {
            (Some(b), Some(f)) => format!("in {b}, started in {f}"),
            (Some(b), None) => format!("in {b}"),
            (None, Some(f)) => format!("started in {f}"),
            _ => String::new(),
        };
        let status = match &e.status {
            Status::Pasted => "Pasted".to_string(),
            Status::Ready => "Not pasted".to_string(),
            Status::Waiting(w) => w.clone(),
            Status::Failed(f) => f.clone(),
        };
        out.push(
            div()
                .mt(px(14.))
                .flex()
                .items_center()
                .gap(px(14.))
                .child(app_badge_via(&e.app, e.via.as_deref(), 44.))
                .child(
                    div()
                        .child(div().text_size(px(24.)).line_height(px(30.)).font_weight(FontWeight::SEMIBOLD).child(e.app.clone()))
                        .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(
                            [when.format("%A %-d %B %Y, %H:%M").to_string(), secs_label(e.secs), format!("{} words", e.words()), status, place]
                                .into_iter()
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join("  ·  "),
                        )),
                )
                .into_any_element(),
        );

        // The text: read, copy, or correct it.
        let editing = self.editing == Some(id);
        let text_card = if editing {
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Correct the text. Voxa keeps the recording either way."))
                .child(self.edit_box.clone())
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .justify_end()
                        .child(wbtn("edit-cancel", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.editing = None;
                            cx.notify();
                        })))
                        .child(wbtn("edit-save", "Save", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let new = this.edit_box.read(cx).text();
                            let old = this.voxa.read(cx).history.get(id).map(|e| e.text.clone()).unwrap_or_default();
                            this.editing = None;
                            if new.trim() != old.trim() && !new.trim().is_empty() {
                                this.suggestion = voxa_core::text::changed_phrase(&old, &new);
                                this.voxa.update(cx, |v, cx| v.edit_entry(id, new, cx));
                            }
                            cx.notify();
                        }))),
                )
        } else {
            let copied = self.copied == Some(id);
            let text = e.text.clone();
            let can_edit = matches!(e.status, Status::Pasted | Status::Ready);
            div()
                .child(div().text_size(px(16.)).line_height(px(26.)).child(if e.text.is_empty() { "No text yet.".to_string() } else { e.text.clone() }))
                .child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .gap(px(8.))
                        .when(can_edit, |d| {
                            d.child(
                                wbtn("copy-detail", if copied { "Copied" } else { "Copy" }, false).child(icon(if copied { I_CHECK } else { I_COPY }, 12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                    this.copied = Some(id);
                                    cx.notify();
                                })),
                            )
                            .child(wbtn("edit", "Edit", false).child(icon(I_EDIT, 12.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                let t = this.voxa.read(cx).history.get(id).map(|e| e.text.clone()).unwrap_or_default();
                                this.edit_box.update(cx, |b, cx| b.set_text(&t, cx));
                                this.edit_box.read(cx).focus(window);
                                this.editing = Some(id);
                                this.suggestion = None;
                                cx.notify();
                            })))
                        }),
                )
        };
        out.push(card().mt(px(18.)).flex_col().items_start().py(px(18.)).child(text_card.w_full()).into_any_element());

        if let Some((from, to)) = self.suggestion.clone() {
            let (f2, t2) = (from.clone(), to.clone());
            out.push(
                div()
                    .mt(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(4.))
                    .bg(rgb(0xF3E7DE))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon(I_WORDS, 14.).text_color(rgb(INK)))
                    .child(div().flex_1().text_size(px(14.)).child(format!("Always change \"{from}\" to \"{to}\" in future dictations?")))
                    .child(wbtn("sug-no", "No thanks", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.suggestion = None;
                        cx.notify();
                    })))
                    .child(wbtn("sug-yes", "Add to Words", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let (f, t) = (f2.clone(), t2.clone());
                        this.voxa.update(cx, |v, cx| v.update_words(|w| w.fixes.push(store::Pair { from: f, to: t }), cx));
                        this.suggestion = None;
                        cx.notify();
                    })))
                    .into_any_element(),
            );
        }

        // The recording.
        if has_audio {
            let playing = self.playing == Some(id);
            let progress = match (playing, self.play_started) {
                (true, Some((t, len))) if len > 0.0 => (t.elapsed().as_secs_f32() / len).clamp(0.0, 1.0),
                _ => 0.0,
            };
            let file = e.audio.clone().unwrap_or_default();
            out.push(section("Recording").into_any_element());
            out.push(
                card()
                    .child(self.icon_btn(format!("dplay-{id}"), if playing { I_STOP } else { I_PLAY }, if playing { "Stop" } else { "Play" }, false, cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_play(id, cx))))
                    .child(div().flex_1().h(px(4.)).rounded(px(2.)).bg(rgb(0xF0EBE5)).child(div().h_full().w(gpui::relative(progress)).rounded(px(2.)).bg(rgb(INK))))
                    .child(div().w(px(60.)).flex().justify_end().text_size(px(13.)).text_color(rgb(MUTED)).child(secs_label(e.secs)))
                    .child(self.icon_btn(format!("dfolder-{id}"), I_FOLDER, "Show the file", false, cx.listener(move |_, _: &ClickEvent, _, _| crate::os::reveal(&file))))
                    .into_any_element(),
            );
        }

        // What else can be done with it.
        let unsent = matches!(e.status, Status::Waiting(_) | Status::Failed(_));
        let mut actions = div().mt(px(18.)).flex().gap(px(8.));
        if has_audio && (!live || unsent) {
            actions = actions.child(
                wbtn("dretry", if unsent { "Send now" } else { "Write it out again" }, false)
                    .child(icon(if unsent { I_SEND } else { I_RETRY }, 12.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.resend(id, cx)))),
            );
        }
        if !live {
            actions = actions.child(if self.confirm_entry == Some(id) {
                wbtn("ddel-yes", "Delete it and its recording", true).bg(rgb(0xC42B1C)).hover(|s| s.bg(rgb(0xA32416))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.confirm_entry = None;
                    if this.playing == Some(id) {
                        this.playing = None;
                        crate::os::stop_audio();
                    }
                    this.voxa.update(cx, |v, cx| v.delete_entry(id, cx));
                    this.go(Page::History, cx);
                }))
            } else {
                wbtn("ddel", "Delete", false).child(icon(I_DELETE, 12.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.confirm_entry = Some(id);
                    cx.notify();
                }))
            });
        }
        out.push(actions.into_any_element());
        out
    }

    fn words_page(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let w = self.voxa.read(cx).words();
        let apps: Vec<String> = {
            let v = self.voxa.read(cx);
            let mut counts: Vec<(String, usize)> = Vec::new();
            for e in v.history.entries.iter().rev().take(2000) {
                if e.app == crate::import::UNKNOWN_APP {
                    continue;
                }
                match counts.iter_mut().find(|(a, _)| *a == e.app) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((e.app.clone(), 1)),
                }
            }
            counts.sort_by(|a, b| b.1.cmp(&a.1));
            counts.into_iter().map(|(a, _)| a).filter(|a| !w.rules.iter().any(|r| r.app == *a)).take(8).collect()
        };
        let mut out: Vec<AnyElement> = vec![
            div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("Words").into_any_element(),
            div()
                .mt(px(4.))
                .text_size(px(14.))
                .text_color(rgb(MUTED))
                .child("Fix what ChatGPT keeps getting wrong, and expand things you say often. Applied on this PC after every dictation, before it is pasted.")
                .into_any_element(),
        ];

        // Fixes and snippets share a shape: a pair list with an add row.
        let pair_list = |this: &mut Self, title: &str, sub: &str, key: &'static str, items: Vec<store::Pair>, from: Entity<TextInput>, to: Entity<TextInput>, cx: &mut Context<Self>| -> Vec<AnyElement> {
            let mut v: Vec<AnyElement> = vec![section(title.to_string()).into_any_element(), div().mt(px(-4.)).mb(px(8.)).text_size(px(12.)).text_color(rgb(MUTED)).child(sub.to_string()).into_any_element()];
            let (f, t) = (from.clone(), to.clone());
            v.push(
                card()
                    .mb(px(3.))
                    .child(div().flex_1().child(from))
                    .child(icon(I_ARROW, 12.).text_color(rgb(MUTED)))
                    .child(div().flex_1().child(to))
                    .child(wbtn(SharedString::from(format!("{key}-add")), "Add", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let (a, b) = (f.read(cx).text(), t.read(cx).text());
                        if a.trim().is_empty() || b.trim().is_empty() {
                            return;
                        }
                        let pair = store::Pair { from: a.trim().to_string(), to: b.trim().to_string() };
                        this.voxa.update(cx, |v, cx| {
                            v.update_words(|w| if key == "fix" { w.fixes.push(pair) } else { w.snippets.push(pair) }, cx)
                        });
                        f.update(cx, |i, cx| i.set_text("", cx));
                        t.update(cx, |i, cx| i.set_text("", cx));
                    })))
                    .into_any_element(),
            );
            for (i, p) in items.iter().enumerate() {
                v.push(
                    card()
                        .min_h(px(48.))
                        .py(px(8.))
                        .mb(px(3.))
                        .child(div().flex_1().min_w(px(0.)).text_size(px(14.)).text_color(rgb(MUTED)).child(p.from.clone()))
                        .child(icon(I_ARROW, 12.).text_color(rgb(MUTED)))
                        .child(div().flex_1().min_w(px(0.)).text_size(px(14.)).child(p.to.clone()))
                        .child(this.icon_btn(format!("{key}-del-{i}"), I_DELETE, "Remove", false, cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.voxa.update(cx, |v, cx| {
                                v.update_words(|w| {
                                    let list = if key == "fix" { &mut w.fixes } else { &mut w.snippets };
                                    if i < list.len() {
                                        list.remove(i);
                                    }
                                }, cx)
                            })
                        })))
                        .into_any_element(),
                );
            }
            v
        };
        let (ff, ft, sf, st) = (self.fix_from.clone(), self.fix_to.clone(), self.snip_from.clone(), self.snip_to.clone());
        out.extend(pair_list(self, "Fixes", "Names and words ChatGPT spells wrong. Tip: edit a dictation and Voxa offers to add the fix for you.", "fix", w.fixes.clone(), ff, ft, cx));
        out.extend(pair_list(self, "Snippets", "Say a short phrase, get the full text: an email address, a sign-off, a link.", "snip", w.snippets.clone(), sf, st, cx));

        out.push(section("Spoken formatting").into_any_element());
        out.push(
            card()
                .child(icon(I_TEXT, 18.).w(px(20.)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().text_size(px(14.)).child("Say \"new line\", \"new paragraph\" or \"bullet point\""))
                        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Turns them into line breaks and bullets. In chats and terminals a line break may send the message.")),
                )
                .child(toggle("spoken-fmt", w.spoken_formatting).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.update_words(|w| w.spoken_formatting = !w.spoken_formatting, cx)))))
                .into_any_element(),
        );

        out.push(section("Per-app rules").into_any_element());
        out.push(div().mt(px(-4.)).mb(px(8.)).text_size(px(12.)).text_color(rgb(MUTED)).child("How text should look in a particular app or site.").into_any_element());
        for (i, r) in w.rules.iter().enumerate() {
            let (np, lc) = (r.no_final_period, r.lowercase);
            out.push(
                card()
                    .mb(px(3.))
                    .child(app_badge(&r.app, 28.))
                    .child(div().w(px(160.)).text_size(px(14.)).child(r.app.clone()))
                    .child(div().flex_1().flex().gap(px(18.)).text_size(px(13.)).child(div().flex().items_center().gap(px(8.)).child("No full stop at the end").child(toggle(Box::leak(format!("rule-np-{i}").into_boxed_str()), np).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.voxa.update(cx, |v, cx| v.update_words(|w| if let Some(r) = w.rules.get_mut(i) { r.no_final_period = !np }, cx))
                    })))).child(div().flex().items_center().gap(px(8.)).child("lowercase").child(toggle(Box::leak(format!("rule-lc-{i}").into_boxed_str()), lc).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.voxa.update(cx, |v, cx| v.update_words(|w| if let Some(r) = w.rules.get_mut(i) { r.lowercase = !lc }, cx))
                    })))))
                    .child(self.icon_btn(format!("rule-del-{i}"), I_DELETE, "Remove", false, cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.voxa.update(cx, |v, cx| v.update_words(|w| if i < w.rules.len() { w.rules.remove(i); }, cx))
                    })))
                    .into_any_element(),
            );
        }
        if !apps.is_empty() {
            let mut chips = div().mt(px(6.)).flex().flex_wrap().items_center().gap(px(6.)).child(div().text_size(px(13.)).text_color(rgb(MUTED)).mr(px(4.)).child("Add a rule for"));
            for (i, a) in apps.into_iter().enumerate() {
                let app = a.clone();
                chips = chips.child(self.chip(SharedString::from(format!("rule-add-{i}")), &a, false, cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let app = app.clone();
                    this.voxa.update(cx, |v, cx| v.update_words(|w| w.rules.push(store::AppRule { app, no_final_period: true, lowercase: false }), cx))
                })));
            }
            out.push(chips.into_any_element());
        }
        out.push(div().h(px(30.)).into_any_element());
        out
    }

    fn toggle_play(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.playing == Some(id) {
            crate::os::stop_audio();
            self.playing = None;
            cx.notify();
            return;
        }
        let secs = self.voxa.read(cx).play_entry(id);
        if let Some(secs) = secs {
            self.playing = Some(id);
            self.play_started = Some((std::time::Instant::now(), secs));
            // Redraw the progress bar while it plays.
            cx.spawn(async move |this, cx| loop {
                cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
                let go_on = this.update(cx, |h, cx| {
                    cx.notify();
                    h.playing == Some(id)
                });
                if !matches!(go_on, Ok(true)) {
                    break;
                }
            })
            .detach();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(std::time::Duration::from_secs_f32(secs + 0.3)).await;
                let _ = this.update(cx, |h, cx| {
                    if h.playing == Some(id) {
                        h.playing = None;
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn range_dates(&self, entries: &[store::Entry]) -> (chrono::NaiveDate, chrono::NaiveDate) {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        let days = |n: i64| today - chrono::Duration::days(n - 1);
        match self.range {
            Range::Days7 => (days(7), today),
            Range::Days30 => (days(30), today),
            Range::ThisMonth => (today.with_day(1).unwrap(), today),
            Range::Days90 => (days(90), today),
            Range::ThisYear => (chrono::NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap(), today),
            Range::All => (entries.first().map(|e| e.local_time().date_naive()).unwrap_or(today).min(today), today),
            Range::Custom => self.custom,
        }
    }

    fn stat_card(n: String, l: &str) -> gpui::Div {
        div()
            .flex_1()
            .min_w(px(0.))
            .px(px(18.))
            .py(px(16.))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgba(CARD_BORDER))
            .rounded(px(4.))
            .child(div().text_size(px(30.)).line_height(px(38.)).font_weight(FontWeight::SEMIBOLD).child(n))
            .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(l.to_string()))
    }

    fn insights(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let entries = self.voxa.read(cx).history.entries.clone();
        let (from, to) = self.range_dates(&entries);
        let today = chrono::Local::now().date_naive();
        let st = store::stats(&entries, from, to, today);
        let label = if chrono::Datelike::year(&from) == chrono::Datelike::year(&to) { format!("{} to {}", from.format("%-d %B"), to.format("%-d %B %Y")) } else { format!("{} to {}", from.format("%-d %B %Y"), to.format("%-d %B %Y")) };
        let dur = |m: u32| if m >= 60 { format!("{}h {}m", m / 60, m % 60) } else { format!("{m} min") };

        let mut out: Vec<AnyElement> = vec![
            div()
                .flex()
                .items_end()
                .justify_between()
                .child(div().child(div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child("Insights")).child(div().mt(px(2.)).text_size(px(14.)).text_color(rgb(MUTED)).child(label)))
                .into_any_element(),
        ];
        // Range picker.
        let mut chips = div().mt(px(14.)).flex().flex_wrap().items_center().gap(px(6.));
        for (r, name) in Range::ALL {
            chips = chips.child(self.chip(SharedString::from(format!("range-{name}")), name, self.range == r, cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.range = r;
                cx.notify();
            })));
        }
        out.push(chips.into_any_element());
        if self.range == Range::Custom {
            let stepper = |id: &'static str, which: usize, d: chrono::NaiveDate, cx: &mut Context<Self>| {
                let step = move |delta: i64| {
                    move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                        let t = chrono::Local::now().date_naive();
                        let (mut a, mut b) = this.custom;
                        if which == 0 { a = (a + chrono::Duration::days(delta)).min(b) } else { b = (b + chrono::Duration::days(delta)).max(a).min(t) }
                        this.custom = (a, b);
                        cx.notify();
                    }
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(self.icon_btn(format!("{id}-m30"), '\u{E892}', "30 days earlier", false, cx.listener(step(-30))))
                    .child(self.icon_btn(format!("{id}-m1"), I_PREV, "A day earlier", false, cx.listener(step(-1))))
                    .child(div().w(px(120.)).flex().justify_center().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(d.format("%-d %b %Y").to_string()))
                    .child(self.icon_btn(format!("{id}-p1"), I_NEXT, "A day later", false, cx.listener(step(1))))
                    .child(self.icon_btn(format!("{id}-p30"), '\u{E893}', "30 days later", false, cx.listener(step(30))))
            };
            let (a, b) = self.custom;
            out.push(
                div()
                    .mt(px(10.))
                    .px(px(14.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .bg(rgb(0xFFFFFF))
                    .border_1()
                    .border_color(rgba(CARD_BORDER))
                    .rounded(px(4.))
                    .text_size(px(13.))
                    .child("From")
                    .child(stepper("from", 0, a, cx))
                    .child("to")
                    .child(stepper("to", 1, b, cx))
                    .into_any_element(),
            );
        }
        out.push(
            div()
                .mt(px(16.))
                .flex()
                .gap(px(3.))
                .child(Self::stat_card(fmt_thousands(st.words), "words"))
                .child(Self::stat_card(fmt_thousands(st.dictations), "dictations"))
                .child(Self::stat_card(dur(st.minutes_saved), "faster than typing"))
                .into_any_element(),
        );
        out.push(
            div()
                .mt(px(3.))
                .flex()
                .gap(px(3.))
                .child(Self::stat_card(dur((st.secs_spoken / 60.0).round() as u32), "spent talking"))
                .child(Self::stat_card(if st.wpm > 0 { format!("{} wpm", st.wpm) } else { "-".into() }, "speaking speed"))
                .child(Self::stat_card(format!("{} day{}", st.streak_days, if st.streak_days == 1 { "" } else { "s" }), "in a row, up to today"))
                .into_any_element(),
        );

        // Chart.
        let max = st.buckets.iter().map(|b| b.1).max().unwrap_or(0).max(1) as f32;
        let n = st.buckets.len().max(1);
        let gap = if n > 20 { 4. } else if n > 10 { 8. } else { 18. };
        let every = n.div_ceil(12);
        let show_values = n <= 12;
        let bars = div().flex().items_end().gap(px(gap)).h(px(170.)).children(st.buckets.iter().map(|(_, w, now)| {
            let col = div().flex_1().min_w(px(0.)).flex().flex_col().items_center().justify_end().h_full();
            if *w == 0 {
                col.child(div().size(px(5.)).rounded_full().bg(rgb(0xE7E1DA)))
            } else {
                let h = 6.0 + 140.0 * (*w as f32 / max);
                col.when(show_values, |c| c.child(div().text_size(px(11.)).text_color(rgb(MUTED)).mb(px(4.)).child(fmt_thousands(*w))))
                    .child(div().w_full().h(px(h)).rounded_t(px(3.)).bg(rgb(if *now { INK } else { 0xE2D6CB })))
            }
        }));
        let labels = div().flex().gap(px(gap)).mt(px(8.)).children(st.buckets.iter().enumerate().map(|(i, (l, _, now))| {
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .justify_center()
                .text_size(px(11.))
                .text_color(rgb(if *now { INK } else { MUTED }))
                .when(*now, |x| x.font_weight(FontWeight::SEMIBOLD))
                .child(if i % every == 0 || *now { l.clone() } else { String::new() })
        }));
        let unit = if n == (to - from).num_days() as usize + 1 { "Words per day" } else if st.buckets.first().is_some_and(|b| b.0.len() <= 6 && !b.0.contains(' ')) { "Words per month" } else { "Words per week" };
        out.push(
            div()
                .mt(px(3.))
                .px(px(20.))
                .py(px(18.))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgba(CARD_BORDER))
                .rounded(px(4.))
                .child(div().mb(px(14.)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(unit))
                .child(bars)
                .child(labels)
                .into_any_element(),
        );

        // Highlights.
        let mut facts: Vec<String> = Vec::new();
        if let Some((d, w)) = &st.busiest_day {
            facts.push(format!("Busiest day: {d}, {} words", fmt_thousands(*w)));
        }
        if st.dictations > 0 {
            facts.push(format!("Average dictation: {} words. Longest: {} words.", st.avg_words, fmt_thousands(st.longest_words)));
        }
        if !facts.is_empty() {
            out.push(div().mt(px(10.)).flex().flex_col().gap(px(4.)).text_size(px(13.)).text_color(rgb(0x3D3D3D)).children(facts).into_any_element());
        }

        if !st.per_app.is_empty() {
            out.push(section("Where you dictate").into_any_element());
            let top = st.per_app.first().map(|a| a.1).unwrap_or(1).max(1) as f32;
            for (app, words) in st.per_app.iter().take(10) {
                let share = *words as f32 / top;
                out.push(
                    card()
                        .min_h(px(52.))
                        .py(px(10.))
                        .mb(px(3.))
                        .child(app_badge(app, 28.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .child(div().text_size(px(14.)).child(app.clone()))
                                .child(div().mt(px(5.)).h(px(4.)).rounded(px(2.)).bg(rgb(0xF0EBE5)).child(div().h_full().w(gpui::relative(share)).rounded(px(2.)).bg(rgb(INK)))),
                        )
                        .child(div().w(px(110.)).flex().justify_end().text_size(px(14.)).text_color(rgb(MUTED)).child(format!("{} words", fmt_thousands(*words))))
                        .into_any_element(),
                );
            }
        } else {
            out.push(div().mt(px(30.)).text_size(px(14.)).text_color(rgb(MUTED)).child("No dictations in this range.").into_any_element());
        }
        out.push(div().mt(px(16.)).mb(px(24.)).text_size(px(12.)).text_color(rgb(MUTED)).child(format!("\"Faster than typing\" compares your speaking time with typing the same words at {} words a minute.", store::TYPING_WPM as u32)).into_any_element());
        out
    }

    fn home(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (entries, shortcut, r, account) = {
            let v = self.voxa.read(cx);
            (v.history.entries.clone(), v.settings.hotkey.clone(), v.readiness(), v.account())
        };
        use chrono::Timelike;
        let now = chrono::Local::now();
        let today = now.date_naive();
        let greeting = match now.hour() {
            5..=11 => "Good morning",
            12..=16 => "Good afternoon",
            _ => "Good evening",
        };
        let first = account.as_ref().and_then(|a| a.name.split_whitespace().next().map(str::to_string)).filter(|n| !n.is_empty());
        let mut out: Vec<AnyElement> = vec![
            div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child(match first { Some(n) => format!("{greeting}, {n}"), None => greeting.to_string() }).into_any_element(),
        ];
        out.extend(self.needs_you(cx));

        // The one thing Voxa does, front and centre.
        let ready = r.blocker().is_none() && r.session == Session::Valid;
        out.push(
            div()
                .mt(px(16.))
                .px(px(24.))
                .py(px(22.))
                .rounded(px(8.))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgba(CARD_BORDER))
                .flex()
                .items_center()
                .gap(px(24.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().flex().items_center().gap(px(8.)).text_size(px(13.)).text_color(rgb(if ready { 0x0E5C0E } else { 0x9D5D00 })).child(icon(if ready { I_CHECK } else { '!' }, 12.)).child(if ready { "Ready to dictate" } else if r.session == Session::Unknown { "Checking your sign-in" } else { "Needs you first" }))
                        .child(div().mt(px(8.)).text_size(px(20.)).line_height(px(28.)).font_weight(FontWeight::SEMIBOLD).child("Click where you want to type, press your shortcut, and talk."))
                        .child(div().mt(px(6.)).text_size(px(13.)).text_color(rgb(MUTED)).child("Press it again to finish. Esc cancels. Works in every app.")),
                )
                .child(div().flex().gap(px(6.)).children(shortcut.split('+').map(|k| {
                    div().h(px(48.)).min_w(px(48.)).px(px(14.)).flex().items_center().justify_center().rounded(px(6.)).bg(rgb(0xFBFAF8)).border_1().border_b_4().border_color(rgba(0x0000002A)).text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(k.to_string())
                })))
                .into_any_element(),
        );

        // Today and this week at a glance.
        let day = store::stats(&entries, today, today, today);
        let week = store::stats(&entries, today - chrono::Duration::days(6), today, today);
        out.push(
            div()
                .mt(px(3.))
                .flex()
                .gap(px(3.))
                .child(Self::stat_card(fmt_thousands(day.words), "words today"))
                .child(Self::stat_card(fmt_thousands(week.words), "words in the last 7 days"))
                .child(Self::stat_card(format!("{} day{}", week.streak_days, if week.streak_days == 1 { "" } else { "s" }), "in a row"))
                .into_any_element(),
        );

        // Recent dictations.
        let recent: Vec<store::Entry> = entries.iter().rev().take(4).cloned().collect();
        out.push(
            section("Recent")
                .flex()
                .items_center()
                .justify_between()
                .child(div().id("see-all").text_size(px(13.)).font_weight(FontWeight::NORMAL).text_color(rgb(INK)).cursor_pointer().child("See all in History").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::History, cx))))
                .into_any_element(),
        );
        if recent.is_empty() {
            out.push(div().text_size(px(14.)).text_color(rgb(MUTED)).child(format!("Nothing yet. Press {shortcut} anywhere and your words show up here.")).into_any_element());
        }
        for e in &recent {
            out.push(self.history_row(e, cx));
        }

        // Things worth knowing.
        out.push(section("Good to know").into_any_element());
        let tip = |glyph: char, title: &str, body: &str| {
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(16.))
                .py(px(14.))
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgba(CARD_BORDER))
                .rounded(px(4.))
                .child(div().flex().items_center().gap(px(8.)).text_color(rgb(INK)).child(icon(glyph, 14.)).child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)).child(title.to_string())))
                .child(div().mt(px(6.)).text_size(px(13.)).line_height(px(19.)).text_color(rgb(MUTED)).child(body.to_string()))
        };
        out.push(
            div()
                .flex()
                .gap(px(3.))
                .child(tip(I_KEEP, "Nothing is lost", "If you're signed out, offline or switch apps, your words wait in History."))
                .child(tip(I_TEXT, "Long takes work", "Pauses are trimmed before sending, so a long ramble still goes through."))
                .child(tip(I_SETTINGS, "Make it yours", "Change the shortcut, remove filler words, or keep history longer in Settings."))
                .into_any_element(),
        );
        out.push(div().h(px(24.)).into_any_element());
        out
    }

    fn settings(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let v = self.voxa.read(cx);
        let s = v.settings.clone();
        let session = v.readiness().session;
        let mic = v.mic_name.clone();
        let autostart = v.start_with_windows;
        let count = v.history.entries.len();
        let mb = v.history.audio_bytes() as f64 / 1e6;
        let row_text = |h: &str, sub: String| {
            div().flex_1().min_w(px(0.)).child(div().text_size(px(14.)).line_height(px(20.)).child(h.to_string())).child(div().mt(px(1.)).text_size(px(12.)).line_height(px(16.)).text_color(rgb(MUTED)).child(sub))
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
        let account = self.voxa.read(cx).account().filter(|_| session == Session::Valid);
        match account {
            Some(a) => {
                let avatar_file = crate::data_dir().join("avatar.png");
                let initial = a.name.chars().next().or(a.email.chars().next()).map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                let avatar = if avatar_file.exists() {
                    div().size(px(48.)).flex_none().rounded_full().overflow_hidden().child(gpui::img(avatar_file).size(px(48.)).rounded_full())
                } else {
                    div().size(px(48.)).flex_none().rounded_full().bg(rgb(0x10A37F)).text_color(rgb(0xFFFFFF)).text_size(px(20.)).flex().items_center().justify_center().child(initial)
                };
                out.push(
                    card()
                        .mb(px(3.))
                        .py(px(16.))
                        .child(avatar)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(if a.name.is_empty() { a.email.clone() } else { a.name.clone() }))
                                .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(a.email.clone()))
                                .child(
                                    div()
                                        .mt(px(6.))
                                        .flex()
                                        .gap(px(6.))
                                        .child(div().px(px(8.)).py(px(2.)).rounded(px(10.)).bg(rgb(0xE8F5EF)).text_color(rgb(0x0E6B4F)).text_size(px(12.)).child(a.plan_label()))
                                        .child(div().px(px(8.)).py(px(2.)).rounded(px(10.)).bg(rgb(0xDFF6DD)).text_color(rgb(0x0E5C0E)).text_size(px(12.)).child("Signed in")),
                                ),
                        )
                        .child(btn)
                        .into_any_element(),
                );
                out.push(div().mb(px(3.)).px(px(16.)).py(px(10.)).text_size(px(12.)).text_color(rgb(MUTED)).child(acct).into_any_element());
            }
            None => out.push(card().mb(px(3.)).child(icon(I_ACCOUNT, 18.).w(px(20.))).child(row_text("ChatGPT", acct)).child(btn).into_any_element()),
        }

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
        out.push(card().mb(px(3.)).child(icon(I_MIC, 18.).w(px(20.))).child(row_text("Microphone", "Voxa uses the default microphone. Change it in your sound settings.".into())).child(div().flex_none().text_size(px(13.)).text_color(rgb(MUTED)).max_w(px(200.)).child(mic)).child(wbtn("sound-settings", "Change", false).on_click(|_, _, _| {
            crate::os::open_sound_settings();
        })).into_any_element());
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
                .child(wbtn("delete-cancel", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.confirm_delete = false;
                    cx.notify();
                })))
                .child(wbtn("delete-confirm", format!("Delete {count} dictation{}", if count == 1 { "" } else { "s" }), true).bg(rgb(0xC42B1C)).hover(|s| s.bg(rgb(0xA32416))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
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
                .child(row_text("Delete history", format!("{count} dictation{} and {mb:.1} MB of recordings on this PC", if count == 1 { "" } else { "s" })))
                .child(delete)
                .into_any_element(),
        );

        out.push(section("System").into_any_element());
        out.push(
            card()
                .mb(px(3.))
                .child(icon(I_POWER, 18.).w(px(20.)))
                .child(row_text(crate::os::START_AT_LOGIN, "Starts quietly in the tray when you sign in".into()))
                .child(toggle("autostart", autostart).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.set_autostart(!v.start_with_windows, cx)))))
                .into_any_element(),
        );
        out.push(section("Help").into_any_element());
        out.push(
            card()
                .mb(px(3.))
                .child(icon('\u{E897}', 18.).w(px(20.)))
                .child(row_text("Take the tour again", "How to dictate, what the strip shows, and your shortcut".into()))
                .child(wbtn("rerun-setup", "Start tour", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.go(Page::Setup(1), cx))))
                .into_any_element(),
        );
        out.push(
            card()
                .mb(px(3.))
                .child(icon('\u{E946}', 18.).w(px(20.)))
                .child(row_text(&format!("Voxa {}", env!("CARGO_PKG_VERSION")), "Your words go to ChatGPT only while you dictate. Everything else stays on this PC.".into()))
                .child(wbtn("open-logs", "Open data folder", false).on_click(cx.listener(|_, _: &ClickEvent, _, _| {
                    crate::os::open(crate::data_dir().as_os_str());
                })))
                .into_any_element(),
        );
        out.push(div().h(px(24.)).into_any_element());
        out
    }

    fn setup(&mut self, step: u8, cx: &mut Context<Self>) -> AnyElement {
        let (shortcut, session, signing_in, mic, levels, latest) = {
            let v = self.voxa.read(cx);
            let latest = v.history.entries.iter().rev().find(|e| e.id > self.try_since && matches!(e.status, Status::Pasted | Status::Ready)).cloned();
            (v.settings.hotkey.clone(), v.readiness().session, v.signing_in, v.mic_name.clone(), v.levels.iter().copied().collect::<Vec<f32>>(), latest)
        };
        let p = |t: &str| div().mt(px(10.)).text_size(px(15.)).line_height(px(22.)).text_color(rgb(0x3D3D3D)).child(t.to_string());
        let h2 = |t: &str| div().text_size(px(28.)).line_height(px(36.)).font_weight(FontWeight::SEMIBOLD).child(t.to_string());
        let point = |glyph: char, title: &str, body: &str| {
            div()
                .flex()
                .items_start()
                .gap(px(14.))
                .mt(px(14.))
                .child(div().size(px(32.)).flex_none().rounded(px(8.)).bg(rgb(0xF3E7DE)).text_color(rgb(INK)).flex().items_center().justify_center().child(icon(glyph, 15.)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
                        .child(div().text_size(px(13.)).line_height(px(19.)).text_color(rgb(MUTED)).child(body.to_string())),
                )
        };
        let mut body: Vec<AnyElement> = Vec::new();
        let mut next: Option<&'static str> = Some("Next");
        match step {
            1 => {
                body.push(div().font_family("Fraunces NonWonky").font_weight(FontWeight::SEMIBOLD).text_size(px(56.)).line_height(px(60.)).text_color(rgb(INK)).child("voxa").into_any_element());
                let demo: Vec<f32> = (0..crate::QUILL_CYCLE).map(|i| demo_level(i)).collect();
                body.push(div().mt(px(6.)).child(crate::ink_line_faded(demo, crate::QUILL_CYCLE - 1, false)).into_any_element());
                body.push(h2("Your voice, written down.").mt(px(18.)).into_any_element());
                body.push(p("Press a shortcut in any app, talk, and your words appear where your cursor is. No window to switch to, nothing to copy.").into_any_element());
                body.push(point(I_KEYBOARD, "Works in every app", "Chats, email, documents, code: anywhere you can type.").into_any_element());
                body.push(point(I_ACCOUNT, "Uses your ChatGPT account", "Your words are written out by ChatGPT. No extra account, no extra cost.").into_any_element());
                body.push(point(I_KEEP, "Your history stays on this PC", "Every dictation is kept here, so nothing you say is ever lost.").into_any_element());
                next = Some("Get started");
            }
            2 => {
                body.push(h2("Sign in to ChatGPT").into_any_element());
                body.push(p("Voxa sends what you say to ChatGPT, through your own account, to turn it into text.").into_any_element());
                body.push(point(I_MIC, "Only while you dictate", "The microphone is used only while the Voxa strip is showing. Nothing is sent at any other time.").into_any_element());
                body.push(point(I_KEEP, "History stays here", "Text and recordings are saved on this PC, not in your ChatGPT chats. Delete them any time in Settings.").into_any_element());
                body.push(point(I_ACCOUNT, "You stay in control", "Sign out any time in Settings. Voxa never sees your password: you sign in on ChatGPT's own page.").into_any_element());
                let state = match (session, signing_in) {
                    (Session::Valid, _) => div().flex().items_center().gap(px(10.)).px(px(14.)).py(px(10.)).rounded(px(4.)).bg(rgb(0xDFF6DD)).text_color(rgb(0x0E5C0E)).child(icon(I_CHECK, 14.)).child("Signed in to ChatGPT. You're ready to dictate."),
                    (_, true) => div().flex().items_center().gap(px(10.)).text_color(rgb(MUTED)).child(icon('\u{E895}', 14.)).child("Finish signing in in the window that opened. Voxa carries on by itself."),
                    _ => div().flex().items_center().gap(px(12.)).child(wbtn("setup-signin", "Sign in to ChatGPT", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx))))).child(
                        div().text_size(px(13.)).text_color(rgb(MUTED)).child(if session == Session::Unknown { "Checking your sign-in..." } else { "Opens ChatGPT's sign-in page in a small window" }),
                    ),
                };
                body.push(div().mt(px(22.)).text_size(px(14.)).child(state).into_any_element());
                if session != Session::Valid {
                    next = Some("Not now");
                }
            }
            3 => {
                body.push(h2("Say something").into_any_element());
                body.push(p("If the line moves when you talk, Voxa can hear you. Nothing is recorded or sent on this step.").into_any_element());
                let tick = levels.len().saturating_sub(1);
                let heard = levels.iter().any(|l| *l > 0.2);
                body.push(
                    div()
                        .mt(px(20.))
                        .px(px(18.))
                        .py(px(14.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgba(CARD_BORDER))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .child(icon(I_MIC, 18.).text_color(rgb(if heard { INK } else { MUTED })))
                        .child(crate::ink_line_faded(levels, tick, false))
                        .child(div().flex_1().text_size(px(12.)).text_color(rgb(MUTED)).child(mic))
                        .into_any_element(),
                );
                body.push(
                    div()
                        .mt(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .text_color(rgb(MUTED))
                        .child(if heard { "Sounds right." } else { "Is the line flat while you talk?" })
                        .when(!heard, |d| {
                            d.child(wbtn("mic-privacy", "Microphone privacy settings", false).h(px(28.)).text_size(px(13.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.open_mic_settings(cx)))))
                                .child(wbtn("mic-sound", "Choose a microphone", false).h(px(28.)).text_size(px(13.)).on_click(|_, _, _| {
                                    crate::os::open_sound_settings();
                                }))
                        })
                        .into_any_element(),
                );
            }
            4 => {
                body.push(h2("Your shortcut").into_any_element());
                body.push(p("Press it once to start, talk, then press it again to stop. Voxa writes it out and pastes it where your cursor is.").into_any_element());
                let caps = div().mt(px(18.)).flex().items_center().gap(px(14.)).child(div().flex().gap(px(6.)).children(shortcut.split('+').map(|k| {
                    div()
                        .h(px(44.))
                        .min_w(px(44.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_b_4()
                        .border_color(rgba(0x0000002A))
                        .text_size(px(18.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(k.to_string())
                })));
                let caps = if self.capturing {
                    caps.child(div().text_size(px(14.)).text_color(rgb(INK)).child("Press the new shortcut..."))
                } else {
                    caps.child(wbtn("setup-change", "Change", false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.capturing = true;
                        this.hotkey_error = None;
                        this.voxa.update(cx, |v, _| v.pause_hotkey(true));
                        window.focus(&this.focus);
                        cx.notify();
                    })))
                };
                body.push(caps.into_any_element());
                if let Some(err) = self.hotkey_error.clone() {
                    body.push(div().mt(px(6.)).text_size(px(12.)).text_color(rgb(0xC42B1C)).child(err).into_any_element());
                }
                // Practice: dictate with this window in front; the text lands here.
                let (text, done) = match &latest {
                    Some(e) => (e.text.clone(), true),
                    None => (format!("Try it now: press {shortcut}, say a sentence, then press it again. Your words appear here."), false),
                };
                body.push(
                    div()
                        .mt(px(22.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgba(if done { 0x0F7B0F66 } else { 0x0000001F }))
                        .bg(rgb(0xFFFFFF))
                        .child(div().px(px(14.)).py(px(8.)).border_b_1().border_color(rgba(CARD_BORDER)).text_size(px(12.)).text_color(rgb(MUTED)).child("Practice"))
                        .child(div().px(px(14.)).py(px(14.)).min_h(px(64.)).text_size(px(15.)).line_height(px(22.)).text_color(rgb(if done { TEXT } else { MUTED })).child(text))
                        .into_any_element(),
                );
                if done {
                    body.push(div().mt(px(10.)).flex().items_center().gap(px(8.)).text_size(px(13.)).text_color(rgb(0x0E5C0E)).child(icon(I_CHECK, 13.)).child("That's it. It works the same in every app.").into_any_element());
                }
            }
            5 => {
                body.push(h2("The strip tells you what's happening").into_any_element());
                body.push(p("It sits just above the taskbar while you dictate and never takes your cursor away.").into_any_element());
                let row = |dot: u32, glyph: char, title: &str, body: &str| {
                    div()
                        .flex()
                        .items_start()
                        .gap(px(14.))
                        .mt(px(12.))
                        .child(div().mt(px(2.)).size(px(24.)).flex_none().rounded_full().bg(rgb(dot)).text_color(rgb(0xFFFFFF)).flex().items_center().justify_center().child(icon(glyph, 11.)))
                        .child(div().flex_1().min_w(px(0.)).child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title.to_string())).child(div().text_size(px(13.)).line_height(px(19.)).text_color(rgb(MUTED)).child(body.to_string())))
                };
                body.push(row(0xE5483D, I_MIC, "Listening", "The quill writes while you talk. Press the shortcut again to finish, or Esc to cancel; nothing is sent if you cancel.").into_any_element());
                body.push(row(INK, '\u{E895}', "Writing it out", "ChatGPT is turning your words into text. Long pauses are trimmed first, so long dictations work too.").into_any_element());
                body.push(row(0x0F7B0F, I_CHECK, "Pasted", "The text is in the app you were using. It's also in History.").into_any_element());
                body.push(row(0x9D5D00, '!', "Needs you", "Signed out, offline, or you moved to another app: Voxa keeps your words and tells you the one thing to do. Nothing is lost.").into_any_element());
            }
            _ => {
                body.push(h2("You're set").into_any_element());
                body.push(p(&format!("Voxa lives in the tray, next to the clock. Press {shortcut} in any app to dictate. Click the Voxa icon to open this window and find everything you've said.")).into_any_element());
                body.push(
                    div()
                        .mt(px(18.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(14.))
                        .py(px(10.))
                        .rounded(px(6.))
                        .bg(rgb(0x202020))
                        .text_color(rgb(0xFFFFFF))
                        .text_size(px(12.))
                        .child(icon('\u{E70E}', 10.).text_color(rgb(0xBBBBBB)))
                        .child(div().size(px(18.)).rounded(px(4.)).bg(rgb(INK)).flex().items_center().justify_center().text_size(px(11.)).font_family("Fraunces NonWonky").child("v"))
                        .child("Voxa is in the tray. If you can't see it, click the arrow and drag it onto the taskbar.")
                        .into_any_element(),
                );
                let on = self.autostart_choice.unwrap_or(true);
                body.push(
                    card()
                        .mt(px(16.))
                        .child(icon(I_POWER, 18.).w(px(20.)))
                        .child(div().flex_1().child(div().text_size(px(14.)).child(crate::os::START_AT_LOGIN_LONG)).child(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Recommended, so the shortcut always works")))
                        .child(toggle("setup-autostart", on).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.autostart_choice = Some(!on);
                            cx.notify();
                        })))
                        .into_any_element(),
                );
                next = Some("Finish");
            }
        }

        let progress = div().flex().gap(px(4.)).children((1..=SETUP_STEPS).map(|i| div().flex_1().h(px(4.)).rounded(px(2.)).bg(rgb(if i <= step { INK } else { 0xE7E1DA }))));
        let buttons = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(36.))
            .py(px(18.))
            .bg(rgb(NAV_BG))
            .border_t_1()
            .border_color(rgba(CARD_BORDER))
            .child(div().flex_1().text_size(px(12.)).text_color(rgb(MUTED)).child(format!("{step} of {SETUP_STEPS}")))
            .when(step > 1, |d| d.child(wbtn("setup-back", "Back", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(Page::Setup(step - 1), cx)))))
            .children(next.map(|label| {
                wbtn("setup-next", label, label != "Not now").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if step >= SETUP_STEPS { this.finish_setup(cx) } else { this.go(Page::Setup(step + 1), cx) }
                }))
            }));
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(NAV_BG))
            .child(
                div()
                    .w(px(720.))
                    .rounded(px(10.))
                    .overflow_hidden()
                    .bg(rgb(PAGE_BG))
                    .border_1()
                    .border_color(rgba(0x0000001A))
                    .shadow_lg()
                    .child(
                        div()
                            .px(px(36.))
                            .pt(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(16.))
                            .child(div().flex_1().child(progress))
                            .when(step < SETUP_STEPS, |d| {
                                d.child(div().id("skip-tour").text_size(px(13.)).text_color(rgb(MUTED)).cursor_pointer().hover(|s| s.text_color(rgb(INK))).child("Skip tour").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finish_setup(cx))))
                            }),
                    )
                    .child(div().px(px(36.)).pt(px(26.)).pb(px(28.)).min_h(px(440.)).children(body))
                    .child(buttons),
            )
            .into_any_element()
    }

    fn finish_setup(&mut self, cx: &mut Context<Self>) {
        let autostart = self.autostart_choice.take();
        self.voxa.update(cx, |v, cx| {
            if let Some(on) = autostart.or(if v.settings.onboarded { None } else { Some(true) }) {
                if on != v.start_with_windows {
                    v.set_autostart(on, cx);
                }
            }
            v.update_settings(|s| s.onboarded = true, cx)
        });
        self.go(Page::Home, cx);
    }
}

/// A made-up speech envelope for the welcome screen's ink line: two phrases and a pause.
fn demo_level(i: usize) -> f32 {
    let t = i as f32;
    let phrase = |c: f32, w: f32| (1.0 - ((t - c) / w).powi(2)).max(0.0);
    let v = phrase(28.0, 18.0) * (0.55 + 0.45 * (t * 0.9).sin().abs()) + phrase(72.0, 20.0) * (0.5 + 0.5 * (t * 1.3).cos().abs());
    v.min(1.0)
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
            Page::Home => self.home(cx),
            Page::History => self.history(cx),
            Page::Insights => self.insights(cx),
            Page::Words => self.words_page(cx),
            Page::Detail(id) => self.detail(id, cx),
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
            .child(self.nav_item("nav-home", I_HOME, "Home", Page::Home, cx))
            .child(self.nav_item("nav-history", I_HISTORY, "History", Page::History, cx))
            .child(self.nav_item("nav-insights", I_INSIGHTS, "Insights", Page::Insights, cx))
            .child(self.nav_item("nav-words", I_WORDS, "Words", Page::Words, cx))
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

/// Plain tooltip.
struct Tip(&'static str);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(px(8.)).py(px(4.)).rounded(px(4.)).bg(rgb(0xFFFFFF)).border_1().border_color(rgba(0x00000026)).shadow_md().font_family("Segoe UI").text_size(px(12.)).text_color(rgb(TEXT)).child(self.0)
    }
}
