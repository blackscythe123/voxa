//! The birthday wish through the Android-facing API: what Kotlin schedules and posts.

use voxa_ffi::*;

#[test]
fn the_birthday_alarm_is_set_for_the_coming_birthday_midnight() {
    assert_eq!(birthday_next_wish("2026-11-28".into(), 11, 29, 0).as_deref(), Some("2026-11-29"));
    assert_eq!(birthday_next_wish("2026-11-29".into(), 11, 29, 0).as_deref(), Some("2027-11-29"));
    assert_eq!(birthday_next_wish("2026-11-28".into(), 2, 29, 0).as_deref(), Some("2027-02-28"));
    assert_eq!(birthday_next_wish("2027-11-28".into(), 11, 29, 2027).as_deref(), Some("2028-11-29"));
}

#[test]
fn no_birthday_alarm_for_a_birthday_that_is_not_a_date() {
    assert_eq!(birthday_next_wish("2026-11-28".into(), 0, 0, 0), None);
    assert_eq!(birthday_next_wish("2026-11-28".into(), 4, 31, 0), None);
    assert_eq!(birthday_next_wish("not a date".into(), 3, 14, 0), None);
    assert!(!birthday_wish_due("2027-03-14".into(), 0, 0, 0, 0));
}

#[test]
fn an_android_wish_held_back_by_doze_still_shows_on_the_day_only() {
    assert!(birthday_wish_due("2027-03-14".into(), 0, 3, 14, 0));
    assert!(birthday_wish_due("2027-03-14".into(), 7 * 60, 3, 14, 0));
    assert!(!birthday_wish_due("2027-03-15".into(), 0, 3, 14, 0));
    assert!(!birthday_wish_due("2027-03-14".into(), 0, 3, 14, 2027), "once a year");
}

#[test]
fn the_android_birthday_notice_says_happy_birthday() {
    let n = notice_birthday(Some("Sam Rivera".into()));
    assert_eq!((n.title.as_str(), n.body.as_str(), n.action), ("Happy birthday, Sam", "Have a lovely day.", None));
    assert_eq!(notice_birthday(None).title, "Happy birthday");
}
