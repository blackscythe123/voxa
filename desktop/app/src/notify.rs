//! Desktop notifications (docs/notifications.md): D1 signed out, D2 ready to copy after a
//! reconnect, D3 couldn't send. The words and the anti-spam rules come from voxa-core
//! (`voxa_core::notices`); this file keeps the state and hands the toast to the OS layer
//! (notify_win.rs: Windows toasts; notify_mac.rs: UserNotifications).
//!
//! Everything else (offline, mic, disk, "N not sent", the strip's NeedsYou states) stays on the
//! strip, the Home bar and the tray, never a toast.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use voxa_core::notices::{self, Decision, Kind};

#[cfg_attr(windows, path = "notify_win.rs")]
#[cfg_attr(target_os = "macos", path = "notify_mac.rs")]
mod backend;

/// Desktop toast identity: Windows tag / macOS request identifier.
pub const TAG_SIGNIN: &str = "signin";
pub const TAG_DICTATIONS: &str = "dictations";

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
}

pub struct Notifier {
    pacing: Pacing,
    signin_showing: bool,
    /// The dictations notification on screen, and what it counts.
    dictations: Option<Dictations>,
    ready: bool,
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
        Self { pacing, signin_showing: false, dictations: None, ready }
    }

    fn save(&self) {
        if let Ok(b) = serde_json::to_vec(&self.pacing) {
            let _ = std::fs::write(path(), b);
        }
    }

    fn decide(&self, kind: Kind, showing: bool) -> Decision {
        notices::decide(kind, crate::now_ms(), showing, &self.pacing.posted, self.pacing.last_dictations)
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
        if !self.ready || looking {
            crate::os::log(&format!("sign-in notification not shown ({})", if looking { "user is looking" } else { "unavailable" }));
            return;
        }
        let d = self.decide(Kind::SignIn, self.signin_showing);
        crate::os::log(&format!("sign-in notification: {d:?} ({waiting} waiting)"));
        if d == Decision::Skip {
            return;
        }
        let n = notices::signed_out(waiting);
        if backend::show(TAG_SIGNIN, &n.title, &n.body, n.action.as_deref().map(|a| (a, "signin")), "signin", true) && d == Decision::Post {
            self.posted(Kind::SignIn);
        }
        self.signin_showing = true;
    }

    /// The session is good again, or the user signed out on purpose: the sign-in toast goes.
    pub fn clear_signin(&mut self) {
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
        if !self.ready || looking {
            return;
        }
        let mut ids = match &self.dictations {
            Some(Dictations::Ready(ids)) => ids.clone(),
            _ => Vec::new(),
        };
        if !ids.contains(&entry) {
            ids.push(entry);
        }
        let showing = self.dictations.is_some();
        let d = self.decide(Kind::Dictations, showing);
        crate::os::log(&format!("dictations notification (ready {}): {d:?}", ids.len()));
        if d == Decision::Skip {
            return;
        }
        let n = notices::ready(ids.len() as u32, Some(app));
        let copy = format!("copy:{entry}");
        let button = if ids.len() == 1 { n.action.as_deref().map(|a| (a, copy.as_str())) } else { None };
        if backend::show(TAG_DICTATIONS, &n.title, &n.body, button, "open", false) && d == Decision::Post {
            self.posted(Kind::Dictations);
        }
        self.dictations = Some(Dictations::Ready(ids));
    }

    /// D3: a send after a reconnect failed. Most recent wins over a ready notice.
    pub fn not_sent(&mut self, looking: bool) {
        if !self.ready || looking {
            return;
        }
        let count = match &self.dictations {
            Some(Dictations::NotSent(n)) => n + 1,
            _ => 1,
        };
        let d = self.decide(Kind::Dictations, self.dictations.is_some());
        crate::os::log(&format!("dictations notification (not sent {count}): {d:?}"));
        if d == Decision::Skip {
            return;
        }
        let n = notices::not_sent(count, "Send now");
        if backend::show(TAG_DICTATIONS, &n.title, &n.body, n.action.as_deref().map(|a| (a, "send")), "open", false) && d == Decision::Post {
            self.posted(Kind::Dictations);
        }
        self.dictations = Some(Dictations::NotSent(count));
    }

    /// History was opened, the dictation copied or resent: the dictations toast goes.
    pub fn clear_dictations(&mut self) {
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
        a => a.strip_prefix("copy:").and_then(|id| id.parse().ok()).map(Click::Copy),
    }
}
