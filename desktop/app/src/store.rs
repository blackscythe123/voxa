//! What Voxa keeps on this PC: dictation history, settings, and the numbers on the Insights page.
//! Everything lives in %APPDATA%\voxa-next as JSON, written atomically.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration as Days, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Status {
    /// Pasted into the app the user was in.
    Pasted,
    /// Written out but not pasted (the user moved, or paste failed): ready to copy.
    Ready,
    /// Not written yet; the reason says what it is waiting for.
    Waiting(String),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Milliseconds since the epoch when recording started. Also the entry's identity.
    pub id: u64,
    pub app: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub secs: f32,
    pub status: Status,
    #[serde(default)]
    pub audio: Option<PathBuf>,
    /// The app the user started in, when they moved and chose to paste somewhere else.
    #[serde(default)]
    pub started_in: Option<String>,
    /// The browser, when `app` is a site in a browser tab ("ChatGPT" via "Brave").
    #[serde(default)]
    pub via: Option<String>,
}

impl Entry {
    pub fn words(&self) -> usize {
        self.text.split_whitespace().count()
    }
    pub fn local_time(&self) -> DateTime<Local> {
        Local.timestamp_millis_opt(self.id as i64).single().unwrap_or_else(Local::now)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

pub struct History {
    path: PathBuf,
    /// Oldest first.
    pub entries: Vec<Entry>,
}

impl History {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.json");
        let entries = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, entries }
    }

    pub fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.entries) {
            let _ = write_atomic(&self.path, &bytes);
        }
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Insert or replace. Returns whether anything changed.
    pub fn upsert(&mut self, entry: Entry) -> bool {
        match self.entries.iter_mut().find(|e| e.id == entry.id) {
            Some(e) if *e == entry => false,
            Some(e) => {
                *e = entry;
                true
            }
            None => {
                let at = self.entries.partition_point(|e| e.id < entry.id);
                self.entries.insert(at, entry);
                true
            }
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        before != self.entries.len()
    }

    /// Drop entries (and their audio) older than `keep_days`. 0 keeps everything.
    pub fn prune(&mut self, keep_days: u32, now_ms: u64) -> usize {
        if keep_days == 0 {
            return 0;
        }
        let cutoff = now_ms.saturating_sub(keep_days as u64 * 86_400_000);
        let (old, keep): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| e.id < cutoff);
        self.entries = keep;
        for e in &old {
            if let Some(a) = &e.audio {
                let _ = std::fs::remove_file(a);
            }
        }
        old.len()
    }

    pub fn delete_all(&mut self) {
        for e in &self.entries {
            if let Some(a) = &e.audio {
                let _ = std::fs::remove_file(a);
            }
        }
        self.entries.clear();
        self.save();
    }

    /// Bytes of audio kept on disk.
    pub fn audio_bytes(&self) -> u64 {
        self.entries.iter().filter_map(|e| e.audio.as_ref()).filter_map(|a| std::fs::metadata(a).ok()).map(|m| m.len()).sum()
    }

    /// Entries matching `query` (case-insensitive, text or app), newest first.
    pub fn search(&self, query: &str) -> Vec<&Entry> {
        let q = query.trim().to_lowercase();
        self.entries
            .iter()
            .rev()
            .filter(|e| q.is_empty() || e.text.to_lowercase().contains(&q) || e.app.to_lowercase().contains(&q))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Strip "um", "uh", "hmm" from transcripts. Off by default: wording is kept as spoken.
    pub remove_fillers: bool,
    /// Days of history to keep; 0 = forever.
    pub keep_days: u32,
    pub hotkey: String,
    /// The guided setup has been finished or skipped.
    pub onboarded: bool,
    /// The old (Electron) Voxa's history and shortcut were brought over.
    pub imported_old_voxa: bool,
    /// Hold the shortcut while talking; a quick tap still toggles.
    pub hold_to_talk: bool,
    /// Microphone chosen in Settings, by name; None follows the system default.
    pub mic_device: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { remove_fillers: false, keep_days: 30, hotkey: "Ctrl+Alt+Space".into(), onboarded: false, imported_old_voxa: false, hold_to_talk: false, mic_device: None }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = write_atomic(&dir.join("settings.json"), &bytes);
        }
    }
    pub const KEEP_CHOICES: [u32; 4] = [7, 30, 90, 0];
    pub fn keep_label(days: u32) -> String {
        if days == 0 { "Forever".into() } else { format!("{days} days") }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pair {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppRule {
    pub app: String,
    pub no_final_period: bool,
    pub lowercase: bool,
}

/// The user's word list (words.json): fixes for words ChatGPT gets wrong, spoken snippets,
/// spoken formatting, and per-app style. Applied on this PC after every dictation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Words {
    pub fixes: Vec<Pair>,
    pub snippets: Vec<Pair>,
    pub spoken_formatting: bool,
    pub rules: Vec<AppRule>,
}

impl Words {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("words.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = write_atomic(&dir.join("words.json"), &bytes);
        }
    }
    /// Fixes, then snippets, then spoken formatting, then the rule for `app` if there is one.
    pub fn apply(&self, text: &str, app: &str) -> String {
        use voxa_core::text::{apply_style, replace_phrases, spoken_formatting};
        let pairs = |v: &[Pair]| v.iter().map(|p| (p.from.clone(), p.to.clone())).collect::<Vec<_>>();
        let mut s = replace_phrases(text, &pairs(&self.fixes));
        s = replace_phrases(&s, &pairs(&self.snippets));
        if self.spoken_formatting {
            s = spoken_formatting(&s);
        }
        if let Some(r) = self.rules.iter().find(|r| r.app.eq_ignore_ascii_case(app)) {
            s = apply_style(&s, r.no_final_period, r.lowercase);
        }
        s
    }
}

/// Typing speed used for "faster than typing": a typical 40 words a minute.
pub const TYPING_WPM: f32 = 40.0;

#[derive(Debug, PartialEq)]
pub struct Insights {
    /// "16 to 22 November"
    pub week_label: String,
    pub week_words: usize,
    pub minutes_saved: u32,
    /// Consecutive days with a dictation, ending today (or yesterday if none yet today).
    pub streak_days: u32,
    /// Words per day, Monday first.
    pub per_day: [usize; 7],
    /// Index of today in `per_day`.
    pub today: usize,
    /// Words per app this week, most first.
    pub per_app: Vec<(String, usize)>,
}

#[allow(dead_code)]
pub fn insights(entries: &[Entry], now: DateTime<Local>) -> Insights {
    let today = now.date_naive();
    let monday = today - Days::days(today.weekday().num_days_from_monday() as i64);
    let sunday = monday + Days::days(6);
    let mut per_day = [0usize; 7];
    let mut per_app: Vec<(String, usize)> = Vec::new();
    let mut week_secs = 0f32;
    let written = entries.iter().filter(|e| matches!(e.status, Status::Pasted | Status::Ready) && !e.text.is_empty());
    for e in written.clone() {
        let d = e.local_time().date_naive();
        if d < monday || d > sunday {
            continue;
        }
        let w = e.words();
        per_day[(d - monday).num_days() as usize] += w;
        week_secs += e.secs;
        match per_app.iter_mut().find(|(a, _)| *a == e.app) {
            Some((_, n)) => *n += w,
            None => per_app.push((e.app.clone(), w)),
        }
    }
    per_app.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let week_words: usize = per_day.iter().sum();
    let minutes_saved = ((week_words as f32 / TYPING_WPM) - week_secs / 60.0).max(0.0).round() as u32;

    let days: std::collections::BTreeSet<NaiveDate> = written.map(|e| e.local_time().date_naive()).collect();
    let mut day = if days.contains(&today) { today } else { today - Days::days(1) };
    let mut streak_days = 0;
    while days.contains(&day) {
        streak_days += 1;
        day -= Days::days(1);
    }

    let week_label = if monday.month() == sunday.month() {
        format!("{} to {} {}", monday.day(), sunday.day(), sunday.format("%B"))
    } else {
        format!("{} {} to {} {}", monday.day(), monday.format("%B"), sunday.day(), sunday.format("%B"))
    };
    Insights { week_label, week_words, minutes_saved, streak_days, per_day, today: (today - monday).num_days() as usize, per_app }
}

/// Numbers for any date range (Insights page).
#[derive(Debug, PartialEq)]
pub struct Stats {
    pub words: usize,
    pub dictations: usize,
    pub secs_spoken: f32,
    pub minutes_saved: u32,
    /// Words a minute while talking.
    pub wpm: u32,
    pub avg_words: usize,
    pub longest_words: usize,
    /// (label, words) of the day with the most words.
    pub busiest_day: Option<(String, usize)>,
    /// Consecutive days with a dictation, ending today (or yesterday). Not limited to the range.
    pub streak_days: u32,
    /// Chart: (label, words, contains today).
    pub buckets: Vec<(String, usize, bool)>,
    /// Words per app in the range, most first.
    pub per_app: Vec<(String, usize)>,
}

/// Stats for `from..=to` (local dates). Buckets are days up to 31 days, weeks up to 26 weeks,
/// else months, so the chart stays readable for any range.
pub fn stats(entries: &[Entry], from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Stats {
    let written: Vec<&Entry> = entries.iter().filter(|e| matches!(e.status, Status::Pasted | Status::Ready) && !e.text.trim().is_empty()).collect();
    let in_range: Vec<&Entry> = written.iter().copied().filter(|e| (from..=to).contains(&e.local_time().date_naive())).collect();
    let words: usize = in_range.iter().map(|e| e.words()).sum();
    let secs_spoken: f32 = in_range.iter().map(|e| e.secs).sum();
    let dictations = in_range.len();

    let mut per_app: Vec<(String, usize)> = Vec::new();
    let mut per_day: std::collections::BTreeMap<NaiveDate, usize> = Default::default();
    for e in &in_range {
        let w = e.words();
        *per_day.entry(e.local_time().date_naive()).or_default() += w;
        match per_app.iter_mut().find(|(a, _)| *a == e.app) {
            Some((_, n)) => *n += w,
            None => per_app.push((e.app.clone(), w)),
        }
    }
    per_app.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let busiest_day = per_day.iter().max_by_key(|(_, w)| **w).map(|(d, w)| (d.format("%a %-d %b").to_string(), *w));

    let span = (to - from).num_days() + 1;
    let mut buckets = Vec::new();
    if span <= 31 {
        let mut d = from;
        while d <= to {
            let label = if span <= 7 { if d == today { "Today".into() } else { d.format("%a").to_string() } } else { d.format("%-d").to_string() };
            buckets.push((label, per_day.get(&d).copied().unwrap_or(0), d == today));
            d += Days::days(1);
        }
    } else if span <= 182 {
        let mut start = from - Days::days(from.weekday().num_days_from_monday() as i64);
        while start <= to {
            let end = start + Days::days(6);
            let w: usize = per_day.range(start.max(from)..=end.min(to)).map(|(_, w)| *w).sum();
            buckets.push((start.max(from).format("%-d %b").to_string(), w, (start..=end).contains(&today)));
            start = end + Days::days(1);
        }
    } else {
        let mut m = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).unwrap();
        while m <= to {
            let next = if m.month() == 12 { NaiveDate::from_ymd_opt(m.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(m.year(), m.month() + 1, 1) }.unwrap();
            let w: usize = per_day.range(m.max(from)..next).map(|(_, w)| *w).sum();
            let label = if from.year() == to.year() { m.format("%b").to_string() } else { m.format("%b %y").to_string() };
            buckets.push((label, w, (m..next).contains(&today)));
            m = next;
        }
    }

    let days: std::collections::BTreeSet<NaiveDate> = written.iter().map(|e| e.local_time().date_naive()).collect();
    let mut day = if days.contains(&today) { today } else { today - Days::days(1) };
    let mut streak_days = 0;
    while days.contains(&day) {
        streak_days += 1;
        day -= Days::days(1);
    }

    Stats {
        words,
        dictations,
        secs_spoken,
        minutes_saved: ((words as f32 / TYPING_WPM) - secs_spoken / 60.0).max(0.0).round() as u32,
        wpm: if secs_spoken > 1.0 { (words as f32 / (secs_spoken / 60.0)).round() as u32 } else { 0 },
        avg_words: if dictations > 0 { words / dictations } else { 0 },
        longest_words: in_range.iter().map(|e| e.words()).max().unwrap_or(0),
        busiest_day,
        streak_days,
        buckets,
        per_app,
    }
}

/// "Today", "Yesterday", or "Monday 17 November".
pub fn day_label(d: NaiveDate, today: NaiveDate) -> String {
    match (today - d).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => d.format("%A %-d %B").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, 0, 0).single().unwrap()
    }
    fn entry(t: DateTime<Local>, app: &str, text: &str, secs: f32, status: Status) -> Entry {
        Entry { id: t.timestamp_millis() as u64, app: app.into(), text: text.into(), secs, status, audio: None, started_in: None, via: None }
    }

    #[test]
    fn insights_count_this_week_only_and_skip_unwritten() {
        // Sunday 22 November 2026.
        let now = at(2026, 11, 22, 18);
        let entries = vec![
            entry(at(2026, 11, 15, 10), "Slack", "last week does not count", 3.0, Status::Pasted),
            entry(at(2026, 11, 16, 9), "Gmail", "one two three four", 2.0, Status::Pasted),
            entry(at(2026, 11, 21, 9), "WhatsApp", "five six", 1.0, Status::Ready),
            entry(at(2026, 11, 22, 9), "WhatsApp", "seven eight nine", 2.0, Status::Pasted),
            entry(at(2026, 11, 22, 10), "WhatsApp", "", 4.0, Status::Waiting("Sign in to send".into())),
        ];
        let i = insights(&entries, now);
        assert_eq!(i.week_label, "16 to 22 November");
        assert_eq!(i.week_words, 9);
        assert_eq!(i.per_day, [4, 0, 0, 0, 0, 2, 3]);
        assert_eq!(i.today, 6);
        assert_eq!(i.per_app, vec![("WhatsApp".into(), 5), ("Gmail".into(), 4)]);
        // 21 and 22 November in a row; 16 is not adjacent.
        assert_eq!(i.streak_days, 2);
    }

    #[test]
    fn minutes_saved_compares_with_typing() {
        let now = at(2026, 11, 22, 18);
        let words = vec!["w"; 400].join(" ");
        // 400 words: 10 minutes to type, 3 minutes to say.
        let i = insights(&[entry(at(2026, 11, 22, 9), "Notepad", &words, 180.0, Status::Pasted)], now);
        assert_eq!(i.minutes_saved, 7);
    }

    #[test]
    fn streak_survives_until_today_is_used() {
        let now = at(2026, 11, 22, 8);
        let e = vec![entry(at(2026, 11, 20, 9), "a", "x", 1.0, Status::Pasted), entry(at(2026, 11, 21, 9), "a", "x", 1.0, Status::Pasted)];
        assert_eq!(insights(&e, now).streak_days, 2);
    }

    #[test]
    fn stats_for_any_range_with_fitting_buckets() {
        let e = vec![
            entry(at(2026, 11, 2, 9), "Slack", "one two three", 6.0, Status::Pasted),
            entry(at(2026, 11, 20, 9), "Gmail", "four five", 3.0, Status::Pasted),
            entry(at(2026, 11, 22, 9), "Gmail", "six seven eight nine", 6.0, Status::Pasted),
            entry(at(2026, 10, 1, 9), "Slack", "outside the range", 2.0, Status::Pasted),
        ];
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        let s = stats(&e, d(11, 1), d(11, 22), d(11, 22));
        assert_eq!((s.words, s.dictations, s.longest_words, s.avg_words), (9, 3, 4, 3));
        assert_eq!(s.buckets.len(), 22, "daily up to a month");
        assert_eq!(s.buckets.last().unwrap(), &("22".to_string(), 4, true));
        assert_eq!(s.per_app[0], ("Gmail".to_string(), 6));
        assert_eq!(s.wpm, 36, "9 words in 15 s");
        assert_eq!(s.busiest_day.as_ref().unwrap().1, 4);
        assert!(stats(&e, d(8, 1), d(11, 22), d(11, 22)).buckets.len() <= 27, "weekly for a few months");
        assert_eq!(stats(&e, d(1, 1), d(11, 22), d(11, 22)).buckets.len(), 11, "monthly for a year");
    }

    #[test]
    fn week_across_months() {
        let i = insights(&[], at(2026, 12, 2, 9));
        assert_eq!(i.week_label, "30 November to 6 December");
    }

    #[test]
    fn history_upsert_keeps_order_and_persists() {
        let dir = std::env::temp_dir().join(format!("voxa-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut h = History::load(&dir);
        assert!(h.upsert(entry(at(2026, 11, 22, 10), "B", "b", 1.0, Status::Pasted)));
        assert!(h.upsert(entry(at(2026, 11, 22, 9), "A", "a", 1.0, Status::Pasted)));
        assert!(!h.upsert(entry(at(2026, 11, 22, 9), "A", "a", 1.0, Status::Pasted)), "no change");
        h.save();
        let back = History::load(&dir);
        assert_eq!(back.entries.iter().map(|e| e.app.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        assert_eq!(back.search("b").len(), 1);
        assert_eq!(back.search("").first().unwrap().app, "B", "newest first");
    }

    #[test]
    fn prune_drops_old_entries_and_their_audio() {
        let dir = std::env::temp_dir().join(format!("voxa-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("old.wav");
        std::fs::write(&wav, b"x").unwrap();
        let mut h = History::load(&dir);
        let mut old = entry(at(2026, 10, 1, 9), "A", "old", 1.0, Status::Pasted);
        old.audio = Some(wav.clone());
        h.upsert(old);
        h.upsert(entry(at(2026, 11, 20, 9), "A", "new", 1.0, Status::Pasted));
        assert_eq!(h.prune(30, at(2026, 11, 22, 9).timestamp_millis() as u64), 1);
        assert_eq!(h.entries.len(), 1);
        assert!(!wav.exists());
        assert_eq!(h.prune(0, u64::MAX), 0, "forever keeps all");
    }

    #[test]
    fn settings_default_and_roundtrip() {
        let dir = std::env::temp_dir().join(format!("voxa-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Settings::load(&dir);
        assert_eq!(s, Settings::default());
        assert!(!s.remove_fillers);
        let s2 = Settings { remove_fillers: true, keep_days: 7, ..s };
        s2.save(&dir);
        assert_eq!(Settings::load(&dir), s2);
        // Older files without the new fields still load.
        std::fs::write(dir.join("settings.json"), r#"{"remove_fillers": true}"#).unwrap();
        assert_eq!(Settings::load(&dir).keep_days, 30);
    }

    #[test]
    fn words_apply_in_order_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("voxa-words-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = Words {
            fixes: vec![Pair { from: "clawed code".into(), to: "Claude Code".into() }],
            snippets: vec![Pair { from: "my email".into(), to: "sam@example.com".into() }],
            spoken_formatting: true,
            rules: vec![AppRule { app: "Claude Code".into(), no_final_period: true, lowercase: false }],
        };
        w.save(&dir);
        let back = Words::load(&dir);
        assert_eq!(back, w);
        assert_eq!(back.apply("Ask clawed code. New line. Mail my email.", "Claude Code"), "Ask Claude Code.\nMail sam@example.com");
        assert_eq!(back.apply("Ask clawed code.", "Slack"), "Ask Claude Code.", "rules only for their app");
    }

    #[test]
    fn hotkey_strings_parse() {
        for s in ["Ctrl+Alt+Space", "Ctrl+Shift+D", "Alt+F9"] {
            assert!(s.parse::<global_hotkey::hotkey::HotKey>().is_ok(), "{s}");
        }
    }
}
