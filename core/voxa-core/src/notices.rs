//! System notifications: their words and how often they may appear, decided once here so the
//! desktop toasts and the Android notifications always agree (docs/notifications.md).
//!
//! Copy rules: sentence case, no emojis, no dashes. Never transcript text, cookies or the account
//! email. A notification may name the app a dictation was for.

/// The two kinds Voxa posts. Each is one notification, updated in place (Android ID 1 and 2,
/// desktop tags "signin" and "dictations").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    SignIn,
    Dictations,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// Label of the one button, if any.
    pub action: Option<String>,
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

/// A1 / D1: the ChatGPT session ended. `waiting`: dictations kept until the user signs in.
pub fn signed_out(waiting: u32) -> Notice {
    let mut body = "Voxa can't write anything until you sign in.".to_string();
    if waiting > 0 {
        let verb = if waiting == 1 { "is" } else { "are" };
        body.push_str(&format!(" {} {verb} waiting and will send when you do.", plural(waiting, "dictation", "dictations")));
    }
    Notice { title: "You're signed out of ChatGPT".into(), body, action: Some("Sign in".into()) }
}

/// A2 / D2: dictations written out while nobody was looking, kept in History. `app` names the
/// app the one dictation was for (ignored for several). Only one dictation gets a Copy button.
pub fn ready(count: u32, app: Option<&str>) -> Notice {
    if count <= 1 {
        let title = match app.map(str::trim).filter(|a| !a.is_empty()) {
            Some(a) => format!("Your dictation for {a} is ready"),
            None => "Your dictation is ready".into(),
        };
        Notice { title, body: "It couldn't paste, so it's waiting in History.".into(), action: Some("Copy".into()) }
    } else {
        Notice { title: format!("{count} dictations are ready"), body: "They're waiting in History.".into(), action: None }
    }
}

/// A3 / D3: a background send gave up. `action` is the shell's own label ("Retry" on Android,
/// "Send now" on the desktop).
pub fn not_sent(count: u32, action: &str) -> Notice {
    Notice {
        title: format!("Voxa couldn't send {}", plural(count.max(1), "dictation", "dictations")),
        body: "Your recordings are kept. Your sign-in is fine.".into(),
        action: Some(action.into()),
    }
}

/// What the Android lock screen shows instead (the public version).
pub const PUBLIC_TITLE: &str = "Voxa needs you";

const HOUR_MS: u64 = 60 * 60 * 1000;
/// Dictations alert at most once in this long; in between the count is only updated silently.
pub const DICTATIONS_EVERY_MS: u64 = 6 * HOUR_MS;
/// At most this many new Voxa notifications in [`CAP_WINDOW_MS`], across kinds.
pub const CAP: usize = 2;
pub const CAP_WINDOW_MS: u64 = 24 * HOUR_MS;

/// Whether to post, and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// A new notification that may make a sound (or not, in quiet hours: the shell decides).
    Post,
    /// Update the one already showing, without a sound.
    Update,
    Skip,
}

/// The anti-spam rules. `showing`: this kind is on screen now (so this is an update in place).
/// `posted`: when earlier new notifications were posted (any kind; older ones are ignored).
/// `last_dictations`: when the last new Dictations notification was posted (0 = never).
/// The shell has already checked that the user isn't looking at Voxa (never post then).
pub fn decide(kind: Kind, now_ms: u64, showing: bool, posted: &[u64], last_dictations: u64) -> Decision {
    if showing {
        return Decision::Update;
    }
    if kind == Kind::Dictations && last_dictations > 0 && now_ms.saturating_sub(last_dictations) < DICTATIONS_EVERY_MS {
        return Decision::Skip;
    }
    let recent = posted.iter().filter(|t| now_ms.saturating_sub(**t) < CAP_WINDOW_MS).count();
    if recent >= CAP { Decision::Skip } else { Decision::Post }
}

/// The post times worth keeping (the last day), with `now_ms` added.
pub fn record(posted: &[u64], now_ms: u64) -> Vec<u64> {
    let mut v: Vec<u64> = posted.iter().copied().filter(|t| now_ms.saturating_sub(*t) < CAP_WINDOW_MS).collect();
    v.push(now_ms);
    v
}

/// Sign-in notifications are posted silently between 21:00 and 08:00 local time.
pub fn quiet_hour(hour: u32) -> bool {
    !(8..21).contains(&hour)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: u64 = HOUR_MS;

    #[test]
    fn signed_out_copy() {
        assert_eq!(signed_out(0).title, "You're signed out of ChatGPT");
        assert_eq!(signed_out(0).body, "Voxa can't write anything until you sign in.");
        assert_eq!(signed_out(1).body, "Voxa can't write anything until you sign in. 1 dictation is waiting and will send when you do.");
        assert_eq!(signed_out(3).body, "Voxa can't write anything until you sign in. 3 dictations are waiting and will send when you do.");
        assert_eq!(signed_out(2).action.as_deref(), Some("Sign in"));
    }

    #[test]
    fn ready_copy() {
        let one = ready(1, Some("WhatsApp"));
        assert_eq!(one.title, "Your dictation for WhatsApp is ready");
        assert_eq!(one.body, "It couldn't paste, so it's waiting in History.");
        assert_eq!(one.action.as_deref(), Some("Copy"));
        assert_eq!(ready(1, Some(" ")).title, "Your dictation is ready");
        let many = ready(4, Some("WhatsApp"));
        assert_eq!(many.title, "4 dictations are ready");
        assert_eq!(many.body, "They're waiting in History.");
        assert_eq!(many.action, None);
    }

    #[test]
    fn not_sent_copy() {
        assert_eq!(not_sent(1, "Retry").title, "Voxa couldn't send 1 dictation");
        assert_eq!(not_sent(2, "Send now").title, "Voxa couldn't send 2 dictations");
        assert_eq!(not_sent(2, "Retry").body, "Your recordings are kept. Your sign-in is fine.");
    }

    #[test]
    fn copy_has_no_dashes_or_emojis() {
        let all = [signed_out(2), ready(1, Some("Notes")), ready(3, None), not_sent(2, "Retry")];
        for n in all {
            for s in [n.title, n.body] {
                assert!(!s.contains(" - ") && !s.contains('\u{2013}') && !s.contains('\u{2014}'), "{s}");
                assert!(s.is_ascii(), "{s}");
            }
        }
    }

    #[test]
    fn showing_updates_in_place() {
        assert_eq!(decide(Kind::SignIn, 100 * H, true, &[99 * H, 99 * H], 0), Decision::Update);
        assert_eq!(decide(Kind::Dictations, 100 * H, true, &[], 99 * H), Decision::Update);
    }

    #[test]
    fn dictations_at_most_every_six_hours() {
        assert_eq!(decide(Kind::Dictations, 100 * H, false, &[95 * H], 95 * H), Decision::Skip);
        assert_eq!(decide(Kind::Dictations, 100 * H, false, &[94 * H], 94 * H), Decision::Post);
        assert_eq!(decide(Kind::Dictations, 100 * H, false, &[], 0), Decision::Post);
        // Sign-in isn't held back by a recent dictations notice.
        assert_eq!(decide(Kind::SignIn, 100 * H, false, &[99 * H], 99 * H), Decision::Post);
    }

    #[test]
    fn two_a_day_at_most() {
        assert_eq!(decide(Kind::SignIn, 100 * H, false, &[90 * H, 99 * H], 0), Decision::Skip);
        assert_eq!(decide(Kind::SignIn, 100 * H, false, &[70 * H, 99 * H], 0), Decision::Post);
        let kept = record(&[10 * H, 90 * H], 100 * H);
        assert_eq!(kept, vec![90 * H, 100 * H]);
    }

    #[test]
    fn quiet_hours() {
        assert!(quiet_hour(21) && quiet_hour(23) && quiet_hour(0) && quiet_hour(7));
        assert!(!quiet_hour(8) && !quiet_hour(12) && !quiet_hour(20));
    }
}
