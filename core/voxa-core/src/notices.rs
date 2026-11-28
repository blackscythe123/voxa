//! System notifications: their words and how often they may appear, decided once here so the
//! desktop toasts and the Android notifications always agree (docs/notifications.md).
//!
//! Copy rules: sentence case, no emojis, no dashes. Never transcript text, cookies or the account
//! email. A notification may name the app a dictation was for.

use crate::greeting::{Date, MonthDay};

/// The kinds Voxa posts. Each is one notification, updated in place (Android ID 1 and 2,
/// desktop tags "signin" and "dictations"). `Update` (A5, the update spec's A4) is Android only, ID 4: the desktop
/// shows updates in context, never as a toast (docs/notifications.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    SignIn,
    Dictations,
    Update,
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

/// A5 (Android only): a verified update finished downloading. Its lock screen text is
/// [`UPDATE_PUBLIC_TITLE`], not "Voxa needs you".
pub fn update_ready(version: &str) -> Notice {
    Notice {
        title: format!("Voxa {} is ready to install", version.trim()),
        body: "Tap to install. Your history and settings stay as they are.".into(),
        action: Some("Install".into()),
    }
}

/// A5's public (lock screen) title: the content is not private.
pub const UPDATE_PUBLIC_TITLE: &str = "Voxa update ready";

/// What to do with A5 now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateDecision {
    Post,
    /// Not now (Voxa is on screen, or the daily cap is used): try again at the next allowed
    /// moment. A5 is never urgent, so it is deferred, never dropped.
    Wait,
    /// Already posted for this version (even if it was swiped away): never again.
    Never,
}

/// A5's rules: at most once per version (`posted_for` is the version last posted, "" for none),
/// never while Voxa is visible, and it counts toward the 2 per 24 hours cap like every kind.
/// Quiet hours don't hold it back: its channel is low importance, so it never makes a sound.
pub fn decide_update(version: &str, posted_for: &str, visible: bool, now_ms: u64, posted: &[u64]) -> UpdateDecision {
    if version.trim().is_empty() || posted_for.trim() == version.trim() {
        return UpdateDecision::Never;
    }
    if visible {
        return UpdateDecision::Wait;
    }
    match decide(Kind::Update, now_ms, false, posted, 0) {
        Decision::Post => UpdateDecision::Post,
        _ => UpdateDecision::Wait,
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
    // A time after `now` comes from a clock that was ahead and has been corrected: it says nothing
    // about what was posted recently, so it never holds a notification back.
    if kind == Kind::Dictations && last_dictations > 0 && last_dictations <= now_ms && now_ms - last_dictations < DICTATIONS_EVERY_MS {
        return Decision::Skip;
    }
    let recent = posted.iter().filter(|t| **t <= now_ms && now_ms - **t < CAP_WINDOW_MS).count();
    if recent >= CAP { Decision::Skip } else { Decision::Post }
}

/// The post times worth keeping (the last day), with `now_ms` added.
pub fn record(posted: &[u64], now_ms: u64) -> Vec<u64> {
    // Times in the future (a clock that was ahead) are dropped, so they can't pile up.
    let mut v: Vec<u64> = posted.iter().copied().filter(|t| *t <= now_ms && now_ms - *t < CAP_WINDOW_MS).collect();
    v.push(now_ms);
    v
}

/// Sign-in notifications are posted silently between 21:00 and 08:00 local time. The birthday
/// wish is the one exception to "never at night": it arrives at midnight on purpose (owner
/// decision, 2026-11-28), outside these rules (see [`birthday_wish_due`]).
pub fn quiet_hour(hour: u32) -> bool {
    !(8..21).contains(&hour)
}

// ── The birthday wish ────────────────────────────────────────────────────────

/// The birthday wish: one notification at midnight on the user's birthday, if they added one and
/// left "Wish me on my birthday" on. `name`: the account's name; only its first word is used.
pub fn birthday(name: Option<&str>) -> Notice {
    let first = name.and_then(|n| n.split_whitespace().next()).filter(|n| !n.is_empty());
    let title = match first {
        Some(n) => format!("Happy birthday, {n}"),
        None => "Happy birthday".into(),
    };
    Notice { title, body: "Have a lovely day.".into(), action: None }
}

/// Android: the alarm is inexact and Doze can hold it back for a while, so a wish delivered any
/// time on the day itself still counts (never on another day).
pub const BIRTHDAY_GRACE_ANDROID_MIN: u16 = 24 * 60;
/// Desktop: the wish only shows if the PC was awake at midnight. A PC that wakes up later in the
/// night never gets a late wish.
pub const BIRTHDAY_GRACE_DESKTOP_MIN: u16 = 2;

/// The local date of the next birthday wish (at 00:00 that day), seen from `today`. Today's
/// midnight has always passed already, so a birthday today means next year's. `wished_year`: the
/// year the last wish was shown (0: never); a year already wished is skipped, so a clock set back
/// never wishes twice. 29 February is wished on 28 February in other years. Local dates only, so
/// a time zone or daylight saving change can't move the day.
pub fn next_birthday_wish(today: Date, birthday: MonthDay, wished_year: i32) -> Date {
    let mut y = today.y;
    loop {
        let d = Date::new(y, birthday.month, birthday.day).in_year(y);
        if d > today && y > wished_year {
            return d;
        }
        y += 1;
    }
}

/// Whether to show the wish now: it is the birthday, `minutes` after local midnight is still
/// within `grace_minutes`, and this year's wish hasn't been shown.
pub fn birthday_wish_due(today: Date, minutes: u16, birthday: MonthDay, wished_year: i32, grace_minutes: u16) -> bool {
    birthday.valid() && birthday.is(today) && wished_year < today.y && minutes < grace_minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_ready_copy_has_no_dash_and_is_sentence_case() {
        let n = update_ready("1.1.0");
        assert_eq!(n.title, "Voxa 1.1.0 is ready to install");
        assert_eq!(n.body, "Tap to install. Your history and settings stay as they are.");
        assert_eq!(n.action.as_deref(), Some("Install"));
        assert_eq!(UPDATE_PUBLIC_TITLE, "Voxa update ready");
        for s in [n.title.as_str(), n.body.as_str(), UPDATE_PUBLIC_TITLE, "Install"] {
            assert!(!s.contains(" - ") && !s.contains('\u{2013}') && !s.contains('\u{2014}') && s.is_ascii(), "{s}");
            // Sentence case: a capital only starts a sentence, or is the name Voxa.
            let words: Vec<&str> = s.split_whitespace().collect();
            for (i, w) in words.iter().enumerate() {
                let cap = w.chars().next().is_some_and(char::is_uppercase);
                let starts = i == 0 || words[i - 1].ends_with('.');
                assert!(!cap || starts || *w == "Voxa", "{s}: {w}");
            }
        }
    }

    #[test]
    fn update_ready_posts_once_per_version() {
        let now = 10 * CAP_WINDOW_MS;
        assert_eq!(decide_update("1.1.0", "", false, now, &[]), UpdateDecision::Post);
        assert_eq!(decide_update("1.1.0", "1.1.0", false, now, &[]), UpdateDecision::Never, "swiped away or not, once per version");
        assert_eq!(decide_update("1.2.0", "1.1.0", false, now, &[]), UpdateDecision::Post);
        assert_eq!(decide_update("1.1.0", "", true, now, &[]), UpdateDecision::Wait, "never while Voxa is on screen");
    }

    #[test]
    fn update_ready_waits_when_the_daily_cap_is_used() {
        let now = 10 * CAP_WINDOW_MS;
        let used = [now - 1000, now - 2000];
        assert_eq!(decide_update("1.1.0", "", false, now, &used), UpdateDecision::Wait, "deferred, not dropped");
        // A day later the cap has room again and the same version still posts.
        assert_eq!(decide_update("1.1.0", "", false, now + CAP_WINDOW_MS, &used), UpdateDecision::Post);
        // And A5 itself counts toward the cap for the other kinds.
        let after = record(&[now - 1000], now);
        assert_eq!(decide(Kind::SignIn, now + 1, false, &after, 0), Decision::Skip);
    }

    const MARCH_14: MonthDay = MonthDay { month: 3, day: 14 };
    const FEB_29: MonthDay = MonthDay { month: 2, day: 29 };

    #[test]
    fn the_birthday_wish_names_the_first_name_only() {
        let n = birthday(Some("Sam Rivera"));
        assert_eq!(n.title, "Happy birthday, Sam");
        assert_eq!(n.body, "Have a lovely day.");
        assert_eq!(n.action, None);
        assert_eq!(birthday(None).title, "Happy birthday");
        assert_eq!(birthday(Some("  ")).title, "Happy birthday");
        for s in [n.title, n.body, birthday(None).title] {
            assert!(!s.contains(" - ") && !s.contains('\u{2013}') && !s.contains('\u{2014}') && s.is_ascii(), "{s}");
        }
    }

    #[test]
    fn the_next_birthday_wish_is_the_coming_midnight_of_the_birthday() {
        assert_eq!(next_birthday_wish(Date::new(2026, 11, 28), MARCH_14, 0), Date::new(2027, 3, 14));
        assert_eq!(next_birthday_wish(Date::new(2027, 3, 13), MARCH_14, 2026), Date::new(2027, 3, 14));
        // Birthday tomorrow: tonight at midnight.
        assert_eq!(next_birthday_wish(Date::new(2026, 11, 28), MonthDay { month: 11, day: 29 }, 0), Date::new(2026, 11, 29));
        // New Year's Eve, birthday on 1 January.
        assert_eq!(next_birthday_wish(Date::new(2026, 12, 31), MonthDay { month: 1, day: 1 }, 2026), Date::new(2027, 1, 1));
    }

    #[test]
    fn a_birthday_today_after_midnight_waits_for_next_year() {
        // Added (or rescheduled after a reboot) during the birthday: no late wish, next year's.
        assert_eq!(next_birthday_wish(Date::new(2027, 3, 14), MARCH_14, 0), Date::new(2028, 3, 14));
        assert!(!birthday_wish_due(Date::new(2027, 3, 14), 9 * 60, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
    }

    #[test]
    fn a_birthday_already_wished_this_year_is_not_wished_again() {
        // The clock went back a day after the wish: the next one is still next year.
        assert_eq!(next_birthday_wish(Date::new(2027, 3, 13), MARCH_14, 2027), Date::new(2028, 3, 14));
        assert!(!birthday_wish_due(Date::new(2027, 3, 14), 0, MARCH_14, 2027, BIRTHDAY_GRACE_ANDROID_MIN));
        assert!(birthday_wish_due(Date::new(2027, 3, 14), 0, MARCH_14, 2026, BIRTHDAY_GRACE_ANDROID_MIN));
    }

    #[test]
    fn a_29_february_birthday_is_wished_on_28_february_in_other_years() {
        assert_eq!(next_birthday_wish(Date::new(2026, 11, 28), FEB_29, 0), Date::new(2027, 2, 28));
        assert_eq!(next_birthday_wish(Date::new(2027, 2, 28), FEB_29, 2027), Date::new(2028, 2, 29));
        assert!(birthday_wish_due(Date::new(2027, 2, 28), 0, FEB_29, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        assert!(!birthday_wish_due(Date::new(2028, 2, 28), 0, FEB_29, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        assert!(birthday_wish_due(Date::new(2028, 2, 29), 0, FEB_29, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
    }

    #[test]
    fn a_pc_asleep_at_midnight_gets_no_late_birthday_wish() {
        let day = Date::new(2027, 3, 14);
        assert!(birthday_wish_due(day, 0, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        assert!(birthday_wish_due(day, 1, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        // Woke up at 00:05, or in the morning: nothing.
        assert!(!birthday_wish_due(day, 5, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        assert!(!birthday_wish_due(day, 8 * 60, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        // Never on another day.
        assert!(!birthday_wish_due(Date::new(2027, 3, 15), 0, MARCH_14, 0, BIRTHDAY_GRACE_ANDROID_MIN));
    }

    /// Owner decision (2026-11-28): "never at night" has exactly one exception, the birthday wish
    /// at midnight. Everything else still keeps quiet hours.
    #[test]
    fn the_birthday_wish_is_the_only_notice_allowed_at_midnight() {
        assert!(quiet_hour(0));
        assert!(birthday_wish_due(Date::new(2027, 3, 14), 0, MARCH_14, 0, BIRTHDAY_GRACE_DESKTOP_MIN));
        assert!(BIRTHDAY_GRACE_DESKTOP_MIN <= 5, "the desktop wish is at midnight, not later in the night");
    }

    #[test]
    fn birthday_dates_are_local_calendar_dates_so_time_zones_cannot_move_them() {
        // Local dates in and out: the same answer whatever the offset or daylight saving. The
        // shells turn the date into 00:00 in the current zone only when scheduling.
        let d = next_birthday_wish(Date::new(2027, 3, 13), MARCH_14, 0);
        assert_eq!(d.to_string(), "2027-03-14");
        assert_eq!(Date::parse("2027-03-14"), Some(d));
    }

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
