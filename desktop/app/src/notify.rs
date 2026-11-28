//! Desktop notifications (docs/notifications.md): D1 signed out, D2 ready to copy after a
//! reconnect, D3 couldn't send, and the birthday wish at midnight. The words and the anti-spam rules come from voxa-core
//! (`voxa_core::notices`); this file keeps the state and hands the toast to the OS layer
//! (notify_win.rs: Windows toasts; notify_mac.rs: UserNotifications).
//!
//! Everything else (offline, mic, disk, "N not sent", the strip's NeedsYou states) stays on the
//! strip, the Home bar and the tray, never a toast.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use voxa_core::greeting::{Date, MonthDay};
use voxa_core::notices::{self, Decision, Kind};

#[cfg_attr(windows, path = "notify_win.rs")]
#[cfg_attr(target_os = "macos", path = "notify_mac.rs")]
mod backend;

/// Desktop toast identity: Windows tag / macOS request identifier.
pub const TAG_SIGNIN: &str = "signin";
pub const TAG_DICTATIONS: &str = "dictations";
pub const TAG_BIRTHDAY: &str = "birthday";

/// What the user clicked in a toast. Collected on the OS's thread, handled by the tray loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Click {
    /// The sign-in toast or its "Sign in" button.
    SignIn,
    /// A dictations toast: open History.
    Open,
    /// "Copy" on a one-dictation toast: this History entry.
    Copy(u64),
    /// "Send now" on a couldn't-send toast.
    SendNow,
    /// The birthday wish: open Home (it shows the birthday doodle today).
    Home,
}

static CLICKS: Mutex<Vec<Click>> = Mutex::new(Vec::new());

pub(crate) fn clicked(c: Click) {
    crate::os::log(&format!("notification clicked: {c:?}"));
    CLICKS.lock().unwrap_or_else(|e| e.into_inner()).push(c);
}

pub fn take_clicks() -> Vec<Click> {
    std::mem::take(&mut *CLICKS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// When notifications were posted, kept across restarts (data folder, notify.json).
#[derive(Default, Serialize, Deserialize)]
struct Pacing {
    #[serde(default)]
    posted: Vec<u64>,
    #[serde(default)]
    last_dictations: u64,
    /// The user was already asked to allow notifications (macOS).
    #[serde(default)]
    asked: bool,
    /// The year the birthday wish was last shown (0: never). At most once a year.
    #[serde(default)]
    birthday_wished: i32,
}

pub struct Notifier {
    pacing: Pacing,
    signin_showing: bool,
    /// The dictations notification on screen, and what it counts.
    dictations: Option<Dictations>,
    ready: bool,
    /// Held back while the user was looking at Voxa; posted by [`Notifier::flush`] once they
    /// aren't (never dropped).
    pending: Pending,
}

#[derive(Default)]
struct Pending {
    signed_out: Option<u32>,
    ready: Vec<(u64, String)>,
    not_sent: u32,
    /// The birthday wish, with the first name it greets.
    birthday: Option<Option<String>>,
}

#[derive(Clone, Debug, PartialEq)]
enum Dictations {
    Ready(Vec<u64>),
    NotSent(u32),
}

fn path() -> std::path::PathBuf {
    crate::data_dir().join("notify.json")
}

impl Notifier {
    pub fn new() -> Self {
        let pacing = std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let ready = backend::init();
        crate::os::log(&format!("notifications {}", if ready { "ready" } else { "unavailable" }));
        Self { pacing, signin_showing: false, dictations: None, ready, pending: Pending::default() }
    }

    fn save(&self) {
        if let Ok(b) = serde_json::to_vec(&self.pacing) {
            let _ = crate::store::write_atomic(&path(), &b);
        }
    }

    fn decide(&mut self, kind: Kind, showing: bool) -> Decision {
        self.forget_dismissed();
        let showing = showing && match kind {
            Kind::SignIn => self.signin_showing,
            Kind::Dictations => self.dictations.is_some(),
        };
        notices::decide(kind, crate::now_ms(), showing, &self.pacing.posted, self.pacing.last_dictations)
    }

    /// Toasts the user closed are gone: the next one is a new post, counted against the limits,
    /// not a silent update of one that isn't there.
    fn forget_dismissed(&mut self) {
        for tag in backend::take_dismissed() {
            match tag.as_str() {
                TAG_SIGNIN => self.signin_showing = false,
                TAG_DICTATIONS => self.dictations = None,
                _ => {}
            }
        }
    }

    /// Post what was held back while the user was looking, now that they aren't.
    pub fn flush(&mut self, looking: bool) {
        if looking || !self.ready {
            return;
        }
        let p = std::mem::take(&mut self.pending);
        if let Some(waiting) = p.signed_out {
            crate::os::log("sign-in notification: posting the one held back");
            self.signed_out(waiting, false);
        }
        for (entry, app) in p.ready {
            self.ready(entry, &app, false);
        }
        for _ in 0..p.not_sent {
            self.not_sent(false);
        }
        if let Some(name) = p.birthday {
            self.show_birthday(name.as_deref());
        }
    }

    /// The birthday wish, checked by the tray every few seconds with the local date and minutes
    /// after midnight. Shown only in the first minutes after midnight on the birthday (the PC was
    /// awake then; one that wakes later never gets a late wish), once a year, if the user added a
    /// birthday and left "Wish me on my birthday" on. The one notification allowed at night, and
    /// not counted against the daily cap. Held back while the user looks at Voxa, like the others.
    pub fn birthday(&mut self, today: Date, minutes: u16, birthday: Option<MonthDay>, wish_on: bool, name: Option<&str>, looking: bool) {
        if !birthday_due(today, minutes, birthday, wish_on, self.pacing.birthday_wished) || !self.ready {
            return;
        }
        // Once a year, decided now: also when it waits for the user to stop looking.
        self.pacing.birthday_wished = today.y;
        self.save();
        if looking {
            crate::os::log("birthday wish held back until the user stops looking at Voxa");
            self.pending.birthday = Some(name.map(str::to_string));
            return;
        }
        self.show_birthday(name);
    }

    fn show_birthday(&mut self, name: Option<&str>) {
        let n = notices::birthday(name);
        let shown = backend::show(TAG_BIRTHDAY, &n.title, &n.body, None, "home", true, false);
        crate::os::log(&format!("birthday wish: {}", if shown { "shown" } else { "not shown" }));
    }

    fn posted(&mut self, kind: Kind) {
        let now = crate::now_ms();
        self.pacing.posted = notices::record(&self.pacing.posted, now);
        if kind == Kind::Dictations {
            self.pacing.last_dictations = now;
        }
        self.save();
    }

    /// D1: a definite, confirmed expiry. `looking`: the strip is up or Voxa's window is in front.
    pub fn signed_out(&mut self, waiting: u32, looking: bool) {
        if !self.ready {
            crate::os::log("sign-in notification not shown (unavailable)");
            return;
        }
        if looking {
            crate::os::log("sign-in notification held back until the user stops looking at Voxa");
            self.pending.signed_out = Some(waiting);
            return;
        }
        let d = self.decide(Kind::SignIn, true);
        crate::os::log(&format!("sign-in notification: {d:?} ({waiting} waiting)"));
        if d == Decision::Skip {
            return;
        }
        let n = notices::signed_out(waiting);
        if backend::show(TAG_SIGNIN, &n.title, &n.body, n.action.as_deref().map(|a| (a, "signin")), "signin", true, d == Decision::Update) && d == Decision::Post {
            self.posted(Kind::SignIn);
        }
        self.signin_showing = true;
    }

    /// The session is good again, or the user signed out on purpose: the sign-in toast goes.
    pub fn clear_signin(&mut self) {
        self.pending.signed_out = None;
        if self.signin_showing {
            crate::os::log("sign-in notification removed");
        }
        self.signin_showing = false;
        if self.ready {
            backend::remove(TAG_SIGNIN);
        }
    }

    /// D2: a dictation that wasn't on screen was written out and waits in History.
    pub fn ready(&mut self, entry: u64, app: &str, looking: bool) {
        if !self.ready {
            return;
        }
        if looking {
            self.pending.ready.push((entry, app.to_string()));
            return;
        }
        let mut ids = match &self.dictations {
            Some(Dictations::Ready(ids)) => ids.clone(),
            _ => Vec::new(),
        };
        if !ids.contains(&entry) {
            ids.push(entry);
        }
        let d = self.decide(Kind::Dictations, true);
        if self.dictations.is_none() {
            // The last one was closed: start counting again.
            ids.retain(|i| *i == entry);
        }
        crate::os::log(&format!("dictations notification (ready {}): {d:?}", ids.len()));
        if d == Decision::Skip {
            return;
        }
        let n = notices::ready(ids.len() as u32, Some(app));
        let copy = format!("copy:{entry}");
        let button = if ids.len() == 1 { n.action.as_deref().map(|a| (a, copy.as_str())) } else { None };
        if backend::show(TAG_DICTATIONS, &n.title, &n.body, button, "open", false, d == Decision::Update) && d == Decision::Post {
            self.posted(Kind::Dictations);
        }
        self.dictations = Some(Dictations::Ready(ids));
    }

    /// D3: a send after a reconnect failed. Most recent wins over a ready notice.
    pub fn not_sent(&mut self, looking: bool) {
        if !self.ready {
            return;
        }
        if looking {
            self.pending.not_sent += 1;
            return;
        }
        let d = self.decide(Kind::Dictations, true);
        let count = match &self.dictations {
            Some(Dictations::NotSent(n)) => n + 1,
            _ => 1,
        };
        crate::os::log(&format!("dictations notification (not sent {count}): {d:?}"));
        if d == Decision::Skip {
            return;
        }
        let n = notices::not_sent(count, "Send now");
        if backend::show(TAG_DICTATIONS, &n.title, &n.body, n.action.as_deref().map(|a| (a, "send")), "open", false, d == Decision::Update) && d == Decision::Post {
            self.posted(Kind::Dictations);
        }
        self.dictations = Some(Dictations::NotSent(count));
    }

    /// History was opened, the dictation copied or resent: the dictations toast goes.
    pub fn clear_dictations(&mut self) {
        self.pending.ready.clear();
        self.pending.not_sent = 0;
        if self.dictations.take().is_some() && self.ready {
            crate::os::log("dictations notification removed");
            backend::remove(TAG_DICTATIONS);
        }
    }

    /// macOS asks once, at the moments Android does (after a sign-in that followed an expiry,
    /// or once the first dictation worked); never at launch. Windows needs no permission.
    pub fn ask_permission(&mut self) {
        if self.pacing.asked || !self.ready {
            return;
        }
        if backend::ask_permission() {
            self.pacing.asked = true;
            self.save();
        }
    }
}

/// Map a toast's arguments ("signin", "open", "send", "copy:<id>") to a click.
pub(crate) fn parse(args: &str) -> Option<Click> {
    match args {
        "signin" => Some(Click::SignIn),
        "open" | "" => Some(Click::Open),
        "send" => Some(Click::SendNow),
        "home" => Some(Click::Home),
        a => a.strip_prefix("copy:").and_then(|id| id.parse().ok()).map(Click::Copy),
    }
}

/// Whether the tray shows the birthday wish now (see [`Notifier::birthday`]).
pub(crate) fn birthday_due(today: Date, minutes: u16, birthday: Option<MonthDay>, wish_on: bool, wished_year: i32) -> bool {
    wish_on && birthday.is_some_and(|b| notices::birthday_wish_due(today, minutes, b, wished_year, notices::BIRTHDAY_GRACE_DESKTOP_MIN))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BDAY: Option<MonthDay> = Some(MonthDay { month: 11, day: 29 });
    const DAY: Date = Date::new(2026, 11, 29);

    #[test]
    fn the_desktop_wishes_at_midnight_on_the_birthday() {
        assert!(birthday_due(DAY, 0, BDAY, true, 0));
        assert!(birthday_due(DAY, 1, BDAY, true, 2025));
        assert!(!birthday_due(Date::new(2026, 11, 28), 23 * 60 + 59, BDAY, true, 0), "not the evening before");
    }

    #[test]
    fn a_pc_asleep_at_midnight_shows_no_late_birthday_toast() {
        // Woke at 00:05 or at 07:30 on the birthday: nothing, and nothing later that day.
        assert!(!birthday_due(DAY, 5, BDAY, true, 0));
        assert!(!birthday_due(DAY, 7 * 60 + 30, BDAY, true, 0));
    }

    #[test]
    fn the_birthday_switch_off_or_no_birthday_means_no_wish() {
        assert!(!birthday_due(DAY, 0, BDAY, false, 0));
        assert!(!birthday_due(DAY, 0, None, true, 0));
    }

    #[test]
    fn the_desktop_wishes_once_a_year() {
        assert!(!birthday_due(DAY, 0, BDAY, true, 2026));
        assert!(birthday_due(Date::new(2027, 11, 29), 0, BDAY, true, 2026));
    }

    #[test]
    fn a_birthday_toast_click_opens_home() {
        assert_eq!(parse("home"), Some(Click::Home));
        assert_eq!(parse("open"), Some(Click::Open));
    }

    /// Never touches the OS: `ready` is true only so the held-back paths run; nothing here posts.
    fn notifier() -> Notifier {
        Notifier { pacing: Pacing::default(), signin_showing: false, dictations: None, ready: true, pending: Pending::default() }
    }

    /// Toasts that came up while the user looked at Voxa were dropped; they are held back instead.
    #[test]
    fn a_toast_while_the_user_looks_at_voxa_is_held_back_not_dropped() {
        let mut n = notifier();
        n.signed_out(2, true);
        n.ready(7, "Notes", true);
        n.not_sent(true);
        n.flush(true);
        assert_eq!(n.pending.signed_out, Some(2), "still held while looking");
        assert_eq!(n.pending.ready, vec![(7, "Notes".to_string())]);
        assert_eq!(n.pending.not_sent, 1);
        // What passed meanwhile is not posted later: clearing drops the held ones too.
        n.ready = false;
        n.clear_dictations();
        n.clear_signin();
        assert!(n.pending.ready.is_empty() && n.pending.not_sent == 0 && n.pending.signed_out.is_none());
    }

    /// A toast the user closed bypassed the 6 h and 2 a day limits (it was "updated" silently):
    /// once dismissed, the next one is a new post counted against the limits.
    #[cfg(windows)]
    #[test]
    fn a_dismissed_toast_counts_again_against_the_limits() {
        let mut n = notifier();
        n.dictations = Some(Dictations::Ready(vec![1]));
        assert_eq!(n.decide(Kind::Dictations, true), Decision::Update, "on screen: updated in place");
        backend::dismissed_for_test(TAG_DICTATIONS);
        assert_eq!(n.decide(Kind::Dictations, true), Decision::Post, "closed: a new post");
        let now = crate::now_ms();
        n.pacing.posted = vec![now - 1000, now - 2000];
        assert_eq!(n.decide(Kind::SignIn, true), Decision::Skip, "two in a day already");
    }
}
