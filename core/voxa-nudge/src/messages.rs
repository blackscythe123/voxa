//! The reminder wording. Plain sentence case, no emojis, no dashes, no guilt.

use serde::{Deserialize, Serialize};

pub const K_STYLES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Style {
    /// A plain "Voxa is ready" reminder.
    Plain,
    /// The time the user has saved this month by speaking.
    Progress,
    /// A tip about something Voxa can do.
    Tip,
    /// "We'll stay quiet": the last reminder until the user dictates again.
    Quiet,
}

impl Style {
    pub const ALL: [Style; K_STYLES] = [Style::Plain, Style::Progress, Style::Tip, Style::Quiet];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Style {
        Self::ALL[i.min(K_STYLES - 1)]
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Plain => "plain",
            Style::Progress => "progress",
            Style::Tip => "tip",
            Style::Quiet => "quiet",
        }
    }
}

/// The progress style needs at least a minute saved this month to say something true.
pub const MIN_SAVED_SECS_FOR_PROGRESS: u32 = 60;

pub fn available(saved_secs_month: u32) -> [bool; K_STYLES] {
    [true, saved_secs_month >= MIN_SAVED_SECS_FOR_PROGRESS, true, true]
}

/// "2h 10m", "45m", "1h".
pub fn format_saved(secs: u32) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

const TIPS: [&str; 4] = [
    "Say \"new line\" while you talk to start a new line.",
    "Say \"new paragraph\" to start a fresh paragraph without touching the screen.",
    "Say \"bullet point\" to begin a list as you speak.",
    "Voxa takes out the ums and uhs for you, so you can think out loud.",
];

/// Title and body for a style. `day` rotates the tips. Falls back to the plain wording when the
/// progress style has nothing true to say.
pub fn message(style: Style, saved_secs_month: u32, day: i64) -> (String, String) {
    match style {
        Style::Progress if saved_secs_month >= MIN_SAVED_SECS_FOR_PROGRESS => (
            "Your time saved this month".into(),
            format!("You've saved {} this month by speaking instead of typing.", format_saved(saved_secs_month)),
        ),
        Style::Tip => ("A quick Voxa tip".into(), TIPS[day.rem_euclid(TIPS.len() as i64) as usize].into()),
        Style::Quiet => (
            "We'll stay quiet".into(),
            "This is our last reminder for now. Voxa is on your keyboard whenever you want it.".into(),
        ),
        _ => ("Ready when you are".into(), "Tap the mic on the Voxa keyboard and say what you want to write.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_time_reads_naturally() {
        assert_eq!(format_saved(45 * 60), "45m");
        assert_eq!(format_saved(3600), "1h");
        assert_eq!(format_saved(2 * 3600 + 10 * 60 + 30), "2h 10m");
    }

    #[test]
    fn copy_has_no_emojis_dashes_or_shouting() {
        for style in Style::ALL {
            for day in 0..8 {
                for saved in [0, 7800] {
                    let (title, body) = message(style, saved, day);
                    for text in [&title, &body] {
                        assert!(text.is_ascii(), "non-ASCII (emoji or typographic dash) in {text:?}");
                        assert!(!text.contains('-'), "dash in {text:?}");
                        assert!(!text.contains('!'), "exclamation in {text:?}");
                        assert!(text.chars().next().unwrap().is_uppercase());
                    }
                    // Sentence case titles: only the first word (and the name Voxa) capitalised.
                    for w in title.split_whitespace().skip(1) {
                        assert!(w == "Voxa" || !w.chars().next().unwrap().is_uppercase(), "title case in {title:?}");
                    }
                    for guilt in ["miss", "forgot", "lonely", "sad", "abandon"] {
                        assert!(!body.to_lowercase().contains(guilt), "guilt trip in {body:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn progress_needs_real_progress() {
        assert_eq!(message(Style::Progress, 0, 0), message(Style::Plain, 0, 0));
        assert!(message(Style::Progress, 7800, 0).1.contains("2h 10m"));
        assert_eq!(available(0), [true, false, true, true]);
    }
}
