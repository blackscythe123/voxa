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

/// History's word for a take that came back with nothing said in it.
pub const NO_SPEECH: &str = "No speech in the recording";
/// History's word for a take whose upload failed (the sign-in is fine).
pub const NOT_REACHED: &str = "Couldn't reach ChatGPT";
/// A take that was being recorded or written out when Voxa last stopped (written at the start
/// of a take, so a forced end, crash or power cut never loses it).
pub const RECORDING: &str = "Recording";
pub const WRITING: &str = "Writing it out";
/// What such a take becomes at the next start: kept, ready for Send now.
pub const NOT_SENT: &str = "Not sent";
pub const WAITING_NETWORK: &str = "Sends when you're back online";
pub const WAITING_SIGN_IN: &str = "Sign in to send";

impl Entry {
    /// Not written out yet, and worth sending: its recording is the only copy.
    pub fn unsent(&self) -> bool {
        match &self.status {
            Status::Waiting(_) => true,
            Status::Failed(r) => r != NO_SPEECH,
            Status::Pasted | Status::Ready => false,
        }
    }

    pub fn words(&self) -> usize {
        self.text.split_whitespace().count()
    }
    pub fn local_time(&self) -> DateTime<Local> {
        Local.timestamp_millis_opt(self.id as i64).single().unwrap_or_else(Local::now)
    }
}

/// Write a whole file or nothing: to a temporary file of its own (process id and a counter, so
/// two writers never share one), flushed to disk, then renamed over the real one. A crash, power
/// cut or second process never leaves half a file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.{}-{}.tmp", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written.and_then(|_| voxa_core::fsx::replace_file(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Read a JSON file: Ok(None) when it doesn't exist, Err when it exists but can't be read or
/// parsed (locked by antivirus or OneDrive, offline, corrupt). A corrupt file is copied aside first.
/// FNV-1a: a stable content name for a kept damaged file (not security).
fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("can't read {}: {e}", path.display())),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|e| {
        // Named by content, so reading the same damaged file again (every render, every
        // dictation) keeps one copy instead of a new one each time.
        let backup = path.with_extension(format!("broken-{:016x}.json", fnv64(&bytes)));
        if !backup.exists() {
            let _ = write_atomic(&backup, &bytes);
        }
        format!("{} is damaged ({e}); a copy was kept as {}", path.display(), backup.display())
    })
}

pub struct History {
    path: PathBuf,
    /// Oldest first.
    pub entries: Vec<Entry>,
    /// history.json exists but couldn't be read: don't save over it until it can be (then merge).
    unreadable: bool,
    /// history.json was damaged and set aside at this start: what it listed is unknown, so no
    /// recording may be treated as having no entry.
    damaged: bool,
}

impl History {
    /// What History listed at start is known for sure (read cleanly, or there was none yet).
    pub fn read_cleanly(&self) -> bool {
        !self.unreadable && !self.damaged
    }

    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.json");
        match read_json::<Vec<Entry>>(&path) {
            Ok(entries) => Self { path, entries: entries.unwrap_or_default(), unreadable: false, damaged: false },
            Err(e) if e.contains("is damaged") => {
                // The broken file is kept aside; start fresh rather than lose new dictations.
                crate::os::log(&format!("FAIL {e}"));
                Self { path, entries: Vec::new(), unreadable: false, damaged: true }
            }
            Err(e) => {
                crate::os::log(&format!("FAIL {e}: not saving history until it can be read"));
                Self { path, entries: Vec::new(), unreadable: true, damaged: false }
            }
        }
    }

    pub fn save(&mut self) {
        if self.unreadable {
            // Try again: once the file is readable, keep both what's in it and what's new.
            match read_json::<Vec<Entry>>(&self.path) {
                Ok(on_disk) => {
                    let mut merged = on_disk.unwrap_or_default();
                    for e in std::mem::take(&mut self.entries) {
                        if !merged.iter().any(|m| m.id == e.id) {
                            merged.push(e);
                        }
                    }
                    merged.sort_by_key(|e| e.id);
                    self.entries = merged;
                    self.unreadable = false;
                    crate::os::log("history file readable again: merged");
                }
                Err(_) => return,
            }
        }
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

    /// Where "older than `days`" starts. Bounded by the newest dictation: a clock that is wrong
    /// for a moment (dead CMOS battery, a VM resumed, dual boot) never deletes recent History.
    fn cutoff(&self, days: u32, now_ms: u64) -> u64 {
        let span = days as u64 * 86_400_000;
        let newest = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
        now_ms.saturating_sub(span).min(newest.saturating_sub(span))
    }

    /// Drop entries (and their audio) older than `keep_days`. 0 keeps everything. Dictations still
    /// waiting to be sent (or that failed) are kept whatever their age: their audio is the only
    /// copy. A take with no speech in it is not worth keeping and goes like any other.
    pub fn prune(&mut self, keep_days: u32, now_ms: u64) -> usize {
        if keep_days == 0 {
            return 0;
        }
        let cutoff = self.cutoff(keep_days, now_ms);
        let (old, keep): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| e.id < cutoff && !e.unsent());
        self.entries = keep;
        for e in &old {
            if let Some(a) = &e.audio {
                let _ = std::fs::remove_file(a);
            }
        }
        old.len()
    }

    /// Delete every entry (and its audio) except `keep`.
    pub fn delete_all_except(&mut self, keep: &[u64]) {
        let (kept, gone): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| keep.contains(&e.id));
        for e in &gone {
            if let Some(a) = &e.audio {
                let _ = std::fs::remove_file(a);
            }
        }
        self.entries = kept;
        self.save();
    }

    /// Delete recordings older than `days` but keep their dictations (text, app, time). Never
    /// touches a dictation that hasn't been written out yet: its recording is the only copy.
    /// Returns the bytes freed. 0 days does nothing.
    pub fn prune_audio(&mut self, days: u32, now_ms: u64) -> u64 {
        if days == 0 {
            return 0;
        }
        let cutoff = self.cutoff(days, now_ms);
        let mut freed = 0;
        for e in self.entries.iter_mut().filter(|e| e.id < cutoff && matches!(e.status, Status::Pasted | Status::Ready)) {
            if let Some(a) = e.audio.take() {
                freed += std::fs::metadata(&a).map(|m| m.len()).unwrap_or(0);
                let _ = std::fs::remove_file(a);
            }
        }
        freed
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
    /// Leave the dictation on the clipboard after pasting (as the old app did); off puts back what
    /// was copied before.
    pub keep_on_clipboard: bool,
    /// Days to keep recordings (the text stays); 0 keeps them as long as the dictation.
    pub keep_audio_days: u32,
    /// Light, dark, or whatever the system uses (the default).
    pub appearance: Appearance,
    /// "Your birthday": month 1..12 and day, 0 when not added. Day and month only, never the year;
    /// it stays on this PC.
    pub birthday_month: u8,
    pub birthday_day: u8,
    /// "Wish me on my birthday": a notification at midnight on the day. On by default.
    pub birthday_wish: bool,
    /// "Update automatically" (Windows) / "Check for updates automatically" (macOS). On by
    /// default; an older settings.json without it reads as on.
    pub auto_update: bool,
}

/// The "Appearance" choice in Settings. Stored as "system", "light" or "dark"; anything else
/// (a newer or damaged value) reads as System, so the rest of the settings still load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];
    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "Use system setting",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

impl Serialize for Appearance {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Appearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(match v.as_str() {
            Some("light") => Appearance::Light,
            Some("dark") => Appearance::Dark,
            _ => Appearance::System,
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self { remove_fillers: false, keep_days: 30, hotkey: "Ctrl+Alt+Space".into(), onboarded: false, imported_old_voxa: false, hold_to_talk: false, mic_device: None, keep_on_clipboard: true, keep_audio_days: 0, appearance: Appearance::System, birthday_month: 0, birthday_day: 0, birthday_wish: true, auto_update: true }
    }
}

impl Settings {
    /// The birthday, if one was added and it is a real day of the year.
    pub fn birthday(&self) -> Option<voxa_core::greeting::MonthDay> {
        Some(voxa_core::greeting::MonthDay { month: self.birthday_month, day: self.birthday_day }).filter(|b| b.valid())
    }

    /// "14 March".
    pub fn birthday_label(month: u8, day: u8) -> String {
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        format!("{day} {}", MONTHS.get(month.wrapping_sub(1) as usize).unwrap_or(&""))
    }

    /// Settings, or the defaults if they can't be read (logged). Use `try_load` where a failed
    /// read must not act like defaults (pruning, importing, saving).
    pub fn load(dir: &Path) -> Self {
        match Self::try_load(dir) {
            Ok(s) => s.unwrap_or_default(),
            Err(e) => {
                crate::os::log(&format!("FAIL reading settings: {e}"));
                Self::default()
            }
        }
    }
    pub fn try_load(dir: &Path) -> Result<Option<Self>, String> {
        read_json(&dir.join("settings.json"))
    }
    pub fn save(&self, dir: &Path) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = write_atomic(&dir.join("settings.json"), &bytes);
        }
    }
    /// Change one setting: read the file again, change it, save it. Refuses when the file exists
    /// but can't be read, so defaults never replace the user's settings (both processes).
    pub fn update(dir: &Path, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut s = Self::try_load(dir)?.unwrap_or_default();
        f(&mut s);
        s.save(dir);
        Ok(s)
    }
    pub const KEEP_CHOICES: [u32; 4] = [7, 30, 90, 0];
    pub const AUDIO_CHOICES: [u32; 3] = [30, 90, 0];
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
    /// For reading only (applying fixes after a dictation, showing the Words page): the list, or
    /// an empty one when the file can't be read right now (logged). Never save what this returns
    /// after a failed read; change the list with [`Words::update`].
    pub fn load(dir: &Path) -> Self {
        match Self::try_load(dir) {
            Ok(w) => w.unwrap_or_default(),
            Err(e) => {
                crate::os::log(&format!("FAIL reading words: {e}"));
                Self::default()
            }
        }
    }
    /// The list; None when there is no words.json yet. A damaged file is kept as
    /// words.broken-<hash>.json (read_json) and reported, never replaced by an empty list.
    pub fn try_load(dir: &Path) -> Result<Option<Self>, String> {
        read_json(&dir.join("words.json"))
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_atomic(&dir.join("words.json"), &bytes).map_err(|e| format!("can't save the word list: {}", e.kind()))
    }
    /// Change the list: read the file again, change it, save it (atomic, through replace_file).
    /// Refuses while words.json exists but can't be read (locked by antivirus or OneDrive, or
    /// damaged), so one added fix never wipes every fix, snippet and app rule.
    pub fn update(dir: &Path, f: impl FnOnce(&mut Words)) -> Result<Words, String> {
        let mut w = Self::try_load(dir)?.unwrap_or_default();
        f(&mut w);
        w.save(dir)?;
        Ok(w)
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

/// One bar of the Insights chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    /// Short axis label ("Mon", "22", "2 Nov", "Nov").
    pub label: String,
    /// Full period for the tooltip and details ("Sunday 22 November", "2 to 8 November").
    pub period: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub words: usize,
    pub dictations: usize,
    pub secs: f32,
    /// Contains today.
    pub current: bool,
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
    /// Chart bars: one per day, week or month, depending on the range.
    pub buckets: Vec<Bucket>,
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

    // Per day: (words, dictations, seconds talking).
    let mut daily: std::collections::BTreeMap<NaiveDate, (usize, usize, f32)> = Default::default();
    for e in &in_range {
        let d = daily.entry(e.local_time().date_naive()).or_default();
        d.0 += e.words();
        d.1 += 1;
        d.2 += e.secs;
    }
    let sum = |a: NaiveDate, b: NaiveDate| daily.range(a..=b).fold((0, 0, 0.0), |acc, (_, v)| (acc.0 + v.0, acc.1 + v.1, acc.2 + v.2));
    let bucket = |label: String, period: String, a: NaiveDate, b: NaiveDate| {
        let (words, dictations, secs) = sum(a, b);
        Bucket { label, period, from: a, to: b, words, dictations, secs, current: (a..=b).contains(&today) }
    };
    let span = (to - from).num_days() + 1;
    let mut buckets = Vec::new();
    if span <= 31 {
        let mut d = from;
        while d <= to {
            let label = if span <= 7 { if d == today { "Today".into() } else { d.format("%a").to_string() } } else { d.format("%-d").to_string() };
            buckets.push(bucket(label, d.format("%A %-d %B").to_string(), d, d));
            d += Days::days(1);
        }
    } else if span <= 182 {
        let mut start = from - Days::days(from.weekday().num_days_from_monday() as i64);
        while start <= to {
            let (a, b) = (start.max(from), (start + Days::days(6)).min(to));
            let period = if a.month() == b.month() { format!("{} to {} {}", a.day(), b.day(), b.format("%B")) } else { format!("{} to {}", a.format("%-d %B"), b.format("%-d %B")) };
            buckets.push(bucket(a.format("%-d %b").to_string(), period, a, b));
            start += Days::days(7);
        }
    } else {
        let mut m = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).unwrap();
        while m <= to {
            let next = if m.month() == 12 { NaiveDate::from_ymd_opt(m.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(m.year(), m.month() + 1, 1) }.unwrap();
            let (a, b) = (m.max(from), (next - Days::days(1)).min(to));
            let label = if from.year() == to.year() { m.format("%b").to_string() } else { m.format("%b %y").to_string() };
            buckets.push(bucket(label, m.format("%B %Y").to_string(), a, b));
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

/// Axis ticks from 0 to at least `max`: about 5 steps of 1, 2, 2.5 or 5 x 10^n.
pub fn nice_ticks(max: f32) -> Vec<f32> {
    if max <= 0.0 {
        return vec![0.0, 1.0];
    }
    let rough = max / 5.0;
    let mag = 10f32.powf(rough.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= rough).unwrap_or(10.0 * mag).max(1.0);
    let top = (max / step).ceil() * step;
    (0..=((top / step).round() as usize)).map(|i| i as f32 * step).collect()
}

/// Settings' birthday steppers: `field` 0 moves the day (wrapping within the month, 29 February
/// allowed), 1 the month (the day kept within the new month).
pub fn step_birthday(s: &mut Settings, field: u8, delta: i32) {
    let days = |m: u8| voxa_core::greeting::days_in_month(2024, m) as i32;
    let (mut m, mut d) = (s.birthday_month.clamp(1, 12) as i32, s.birthday_day.max(1) as i32);
    if field == 0 {
        d = (d - 1 + delta).rem_euclid(days(m as u8)) + 1;
    } else {
        m = (m - 1 + delta).rem_euclid(12) + 1;
        d = d.min(days(m as u8));
    }
    s.birthday_month = m as u8;
    s.birthday_day = d as u8;
}

#[cfg(test)]
mod load_tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voxa-load-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_birthday_is_kept_in_settings_as_day_and_month_with_the_wish_on_by_default() {
        let d = tmp("birthday");
        // Older settings files have no birthday: none, and the wish defaults to on.
        std::fs::write(d.join("settings.json"), br#"{"onboarded": true}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.birthday(), s.birthday_wish), (None, true));
        let s = Settings::update(&d, |s| {
            s.birthday_month = 2;
            s.birthday_day = 29;
        })
        .unwrap();
        assert_eq!(Settings::load(&d).birthday(), Some(voxa_core::greeting::MonthDay { month: 2, day: 29 }));
        assert_eq!(Settings::birthday_label(s.birthday_month, s.birthday_day), "29 February");
        let raw = std::fs::read_to_string(d.join("settings.json")).unwrap();
        assert!(raw.contains("\"birthday_month\": 2") && !raw.contains("year"), "{raw}");
        // Not a real day: no birthday.
        Settings::update(&d, |s| s.birthday_day = 31).unwrap();
        assert_eq!(Settings::load(&d).birthday(), None);
    }

    #[test]
    fn the_birthday_steppers_wrap_and_keep_the_day_inside_the_month() {
        let mut s = Settings { birthday_month: 1, birthday_day: 31, ..Settings::default() };
        step_birthday(&mut s, 0, 1);
        assert_eq!((s.birthday_month, s.birthday_day), (1, 1));
        step_birthday(&mut s, 0, -1);
        assert_eq!((s.birthday_month, s.birthday_day), (1, 31));
        step_birthday(&mut s, 1, 1);
        assert_eq!((s.birthday_month, s.birthday_day), (2, 29), "29 February is a birthday");
        step_birthday(&mut s, 1, -2);
        assert_eq!((s.birthday_month, s.birthday_day), (12, 29));
    }

    #[test]
    fn missing_settings_are_not_an_error() {
        let d = tmp("missing");
        assert!(matches!(Settings::try_load(&d), Ok(None)));
    }

    #[test]
    fn damaged_settings_are_an_error_and_kept_aside() {
        let d = tmp("damaged");
        std::fs::write(d.join("settings.json"), b"{ not json").unwrap();
        let e = Settings::try_load(&d).unwrap_err();
        assert!(e.contains("is damaged"), "{e}");
        let kept = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).any(|e| e.file_name().to_string_lossy().contains("broken"));
        assert!(kept, "a copy of the damaged file is kept");
    }

    #[test]
    fn unreadable_history_is_never_overwritten_and_merges_later() {
        let d = tmp("history");
        let path = d.join("history.json");
        let old = vec![Entry { id: 1, app: "A".into(), text: "old".into(), secs: 1.0, status: Status::Pasted, audio: None, started_in: None, via: None }];
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        // Pretend the file couldn't be read at startup (locked): Voxa starts empty and unreadable.
        let mut h = History { path: path.clone(), entries: Vec::new(), unreadable: true, damaged: false };
        h.entries.push(Entry { id: 2, app: "B".into(), text: "new".into(), secs: 1.0, status: Status::Pasted, audio: None, started_in: None, via: None });
        // Now readable again: saving merges instead of overwriting.
        h.save();
        let on_disk: Vec<Entry> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(on_disk.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2]);
    }

    /// Two writers never share a temporary file, and none is left behind.
    #[test]
    fn atomic_writes_use_their_own_temp_and_leave_none() {
        let d = tmp("atomic");
        let path = d.join("history.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let p = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        write_atomic(&p, format!("[{i}]").repeat(200).as_bytes()).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.chars().all(|c| "[]01234567".contains(c)) && body.len() % 600 == 0, "a torn or mixed file");
        let left: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".tmp")).collect();
        assert!(left.is_empty(), "temporary files left: {left:?}");
    }

    /// Hunt 4: a settings file that can't be read is never overwritten with defaults.
    #[test]
    fn unreadable_settings_are_never_saved_over() {
        let d = tmp("update");
        let damaged = b"{ \"keep_days\": 0, broken";
        std::fs::write(d.join("settings.json"), damaged).unwrap();
        let r = Settings::update(&d, |s| s.hotkey = "Ctrl+Shift+K".into());
        assert!(r.is_err(), "a change on top of an unreadable file is refused");
        assert_eq!(std::fs::read(d.join("settings.json")).unwrap(), damaged, "the user's file is untouched");
        // Readable again: the change lands on the user's own settings, not on defaults.
        std::fs::write(d.join("settings.json"), br#"{ "keep_days": 0 }"#).unwrap();
        let s = Settings::update(&d, |s| s.hotkey = "Ctrl+Shift+K".into()).unwrap();
        assert_eq!((s.keep_days, s.hotkey.as_str()), (0, "Ctrl+Shift+K"), "Forever stays Forever");
        // No file yet (first run): defaults are right.
        let fresh = tmp("update-fresh");
        assert_eq!(Settings::update(&fresh, |_| {}).unwrap(), Settings::default());
    }
}

#[cfg(test)]
mod hunt_tests {
    use super::*;

    fn entry(dir: &Path, id: u64, status: Status) -> Entry {
        let a = dir.join(format!("{id}.wav"));
        std::fs::write(&a, b"RIFF").unwrap();
        Entry { id, app: "A".into(), text: "kept".into(), secs: 1.0, status, audio: Some(a), started_in: None, via: None }
    }

    #[test]
    fn a_clock_jump_at_startup_does_not_wipe_history() {
        let day = 86_400_000u64;
        let real_now = 20_000 * day;
        let dir = std::env::temp_dir().join(format!("voxa-hunt-clock-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mk = |id: u64| entry(&dir, id, Status::Pasted);
        let mut h = History { unreadable: false, damaged: false, path: dir.join("h.json"), entries: vec![mk(real_now - 3 * day), mk(real_now - day), mk(real_now - 60_000)] };
        // The PC boots one year ahead for a moment (keep_days = 30, the default).
        let removed = h.prune(30, real_now + 365 * day);
        assert_eq!(removed, 0, "yesterday's dictations deleted because the clock jumped ({} of 3 left)", h.entries.len());
        assert_eq!(h.prune_audio(30, real_now + 365 * day), 0, "and their recordings");
        assert!(h.entries.iter().all(|e| e.audio.as_ref().is_some_and(|a| a.exists())));
    }

    /// Hunt 17/20: a take with no speech is labelled so, isn't kept forever and isn't resent.
    #[test]
    fn no_speech_takes_are_not_unsent_and_are_pruned() {
        let day = 86_400_000u64;
        let dir = std::env::temp_dir().join(format!("voxa-hunt-nospeech-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut h = History {
            unreadable: false, damaged: false,
            path: dir.join("h.json"),
            entries: vec![entry(&dir, day, Status::Failed(NO_SPEECH.into())), entry(&dir, 2 * day, Status::Failed(NOT_REACHED.into())), entry(&dir, 99 * day, Status::Pasted)],
        };
        assert!(!h.entries[0].unsent());
        assert!(h.entries[1].unsent());
        assert_eq!(h.prune(30, 100 * day), 1);
        assert_eq!(h.entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2 * day, 99 * day], "the failed upload is kept");
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
    fn settings_without_auto_update_read_as_on() {
        let old: Settings = serde_json::from_str(r#"{"remove_fillers":false,"keep_days":30,"hotkey":"Ctrl+Alt+Space","onboarded":true}"#).unwrap();
        assert!(old.auto_update, "an older settings.json means Update automatically is on");
        assert!(Settings::default().auto_update);
        let off: Settings = serde_json::from_str(r#"{"auto_update":false}"#).unwrap();
        assert!(!off.auto_update);
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
        let last = s.buckets.last().unwrap();
        assert_eq!((last.label.as_str(), last.words, last.dictations, last.current), ("22", 4, 1, true));
        assert_eq!(s.buckets.iter().map(|b| b.words).sum::<usize>(), s.words, "bars add up to the total");
        assert_eq!(s.per_app[0], ("Gmail".to_string(), 6));
        assert_eq!(s.wpm, 36, "9 words in 15 s");
        assert_eq!(s.busiest_day.as_ref().unwrap().1, 4);
        assert!(stats(&e, d(8, 1), d(11, 22), d(11, 22)).buckets.len() <= 27, "weekly for a few months");
        assert_eq!(stats(&e, d(1, 1), d(11, 22), d(11, 22)).buckets.len(), 11, "monthly for a year");
    }

    #[test]
    fn prune_audio_keeps_text_and_unsent_recordings() {
        let day = 86_400_000u64;
        let now = 100 * day;
        let dir = std::env::temp_dir().join(format!("voxa-audio-prune-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mk = |id: u64, status: Status| {
            let a = dir.join(format!("{id}.wav"));
            std::fs::write(&a, vec![0u8; 1000]).unwrap();
            Entry { id, app: "A".into(), text: format!("t{id}"), secs: 1.0, status, audio: Some(a), started_in: None, via: None }
        };
        let mut h = History { unreadable: false, damaged: false, path: dir.join("h.json"), entries: vec![mk(day, Status::Pasted), mk(2 * day, Status::Waiting("x".into())), mk(99 * day, Status::Pasted)] };
        assert_eq!(h.prune_audio(30, now), 1000);
        assert_eq!(h.entries.len(), 3, "every dictation's text stays");
        assert!(h.entries[0].audio.is_none(), "old pasted recording deleted");
        assert!(h.entries[1].audio.is_some(), "unsent recording kept");
        assert!(h.entries[2].audio.is_some(), "recent recording kept");
        assert_eq!(h.prune_audio(0, now), 0);
    }

    #[test]
    fn prune_keeps_unsent_recordings() {
        let day = 86_400_000u64;
        let now = 100 * day;
        let mk = |id: u64, status: Status| Entry { id, app: "A".into(), text: "t".into(), secs: 1.0, status, audio: None, started_in: None, via: None };
        let mut h = History { unreadable: false, damaged: false, path: std::env::temp_dir().join("voxa-prune-test.json"), entries: vec![mk(day, Status::Pasted), mk(2 * day, Status::Waiting("Sign in to send".into())), mk(3 * day, Status::Failed("x".into())), mk(99 * day, Status::Pasted)] };
        assert_eq!(h.prune(30, now), 1);
        let left: Vec<u64> = h.entries.iter().map(|e| e.id).collect();
        assert_eq!(left, vec![2 * day, 3 * day, 99 * day]);
    }

    /// "Delete all" while a dictation was live deleted its recording, then showed "Unreachable":
    /// live entries and their audio stay.
    #[test]
    fn delete_all_spares_the_dictations_in_progress() {
        let dir = std::env::temp_dir().join(format!("voxa-delete-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let wav = |n: u64| {
            let p = dir.join(format!("{n}.wav"));
            std::fs::write(&p, b"RIFF").unwrap();
            p
        };
        let mk = |id: u64, status: Status| Entry { id, app: "A".into(), text: "t".into(), secs: 1.0, status, audio: Some(wav(id)), started_in: None, via: None };
        let mut h = History { unreadable: false, damaged: false, path: dir.join("history.json"), entries: vec![mk(1, Status::Pasted), mk(2, Status::Waiting("Writing it out".into()))] };
        h.delete_all_except(&[2]);
        assert_eq!(h.entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2]);
        assert!(!dir.join("1.wav").exists(), "a finished dictation's recording goes");
        assert!(dir.join("2.wav").exists(), "the live take keeps its recording");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ticks_are_round_and_cover_the_max() {
        assert_eq!(nice_ticks(4062.0), vec![0.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0]);
        assert_eq!(nice_ticks(37.0), vec![0.0, 10.0, 20.0, 30.0, 40.0]);
        assert_eq!(nice_ticks(3.0), vec![0.0, 1.0, 2.0, 3.0]);
        assert_eq!(nice_ticks(0.0), vec![0.0, 1.0]);
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
        // The dictation no longer stayed on the clipboard after pasting: keeping it is the default.
        assert!(s.keep_on_clipboard, "the dictation stays on the clipboard by default");
        let s2 = Settings { remove_fillers: true, keep_days: 7, ..s };
        s2.save(&dir);
        assert_eq!(Settings::load(&dir), s2);
        // Older files without the new fields still load.
        std::fs::write(dir.join("settings.json"), r#"{"remove_fillers": true}"#).unwrap();
        assert_eq!(Settings::load(&dir).keep_days, 30);
        assert_eq!(Settings::load(&dir).appearance, Appearance::System, "old files follow the system");
    }

    #[test]
    fn appearance_round_trips_and_unknown_values_follow_the_system() {
        let dir = std::env::temp_dir().join(format!("voxa-appearance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for a in Appearance::ALL {
            let s = Settings { appearance: a, ..Settings::default() };
            s.save(&dir);
            assert_eq!(Settings::load(&dir).appearance, a);
        }
        let json = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(json.contains(r#""appearance": "dark""#), "{json}");
        std::fs::write(dir.join("settings.json"), r#"{"keep_days": 7, "appearance": "sepia"}"#).unwrap();
        let s = Settings::try_load(&dir).unwrap().unwrap();
        assert_eq!((s.keep_days, s.appearance), (7, Appearance::System), "an unknown value keeps the other settings");
        std::fs::write(dir.join("settings.json"), r#"{"appearance": 3}"#).unwrap();
        assert_eq!(Settings::try_load(&dir).unwrap().unwrap().appearance, Appearance::System);
        let _ = std::fs::remove_dir_all(&dir);
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
        w.save(&dir).unwrap();
        let back = Words::load(&dir);
        assert_eq!(back, w);
        assert_eq!(back.apply("Ask clawed code. New line. Mail my email.", "Claude Code"), "Ask Claude Code.\nMail sam@example.com");
        assert_eq!(back.apply("Ask clawed code.", "Slack"), "Ask Claude Code.", "rules only for their app");
    }

    /// history.json exactly as 0.4.0 to 0.4.7 wrote it (serde's default enum format; the Status
    /// enum is unchanged since v0.4.0, checked against the tags), loads in 1.0.0 with every entry.
    #[test]
    fn a_0_4_x_history_file_loads_with_every_entry() {
        let dir = std::env::temp_dir().join(format!("voxa-history-04x-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = r#"[
  { "id": 1730000000000, "app": "Slack", "text": "hello there", "secs": 2.5, "status": "Pasted",
    "audio": "C:\\Users\\a\\AppData\\Roaming\\voxa-next\\audio\\1730000000000.wav", "started_in": null, "via": null },
  { "id": 1730000001000, "app": "ChatGPT", "text": "ready to copy", "secs": 1.0, "status": "Ready", "via": "Brave" },
  { "id": 1730000002000, "app": "Notepad", "status": { "Waiting": "Sends when you're back online" }, "audio": "x.wav" },
  { "id": 1730000003000, "app": "Word", "text": "", "secs": 0.4, "status": { "Failed": "No speech in the recording" } },
  { "id": 1730000004000, "app": "Outlook", "text": "moved", "secs": 3.0, "status": "Pasted", "started_in": "Teams" }
]"#;
        std::fs::write(dir.join("history.json"), old).unwrap();
        let h = History::load(&dir);
        assert!(h.read_cleanly(), "a 0.4.x file is never treated as damaged");
        assert_eq!(h.entries.len(), 5);
        assert_eq!(h.entries[0].status, Status::Pasted);
        assert_eq!(h.entries[1].via.as_deref(), Some("Brave"));
        assert_eq!(h.entries[2].status, Status::Waiting(WAITING_NETWORK.into()));
        assert!(h.entries[2].unsent());
        assert_eq!(h.entries[3].status, Status::Failed(NO_SPEECH.into()));
        assert_eq!(h.entries[4].started_in.as_deref(), Some("Teams"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_word_list_is_kept_and_never_overwritten_by_an_edit() {
        let dir = std::env::temp_dir().join(format!("voxa-words-damaged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let damaged = b"{\"fixes\":[{\"from\":\"clawed\",\"to\":\"Claude\"}], oops";
        std::fs::write(dir.join("words.json"), damaged).unwrap();
        // Adding one fix is refused: the user's list is not replaced by an empty one plus that fix.
        let r = Words::update(&dir, |w| w.fixes.push(Pair { from: "a".into(), to: "b".into() }));
        assert!(r.unwrap_err().contains("damaged"));
        assert_eq!(std::fs::read(dir.join("words.json")).unwrap(), damaged.to_vec(), "words.json untouched");
        let _ = Words::load(&dir);
        let _ = Words::load(&dir);
        let kept = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("words.broken-")).count();
        assert_eq!(kept, 1, "one copy of the damaged file is kept, however often it is read");
        // No file yet: the first edit creates it.
        let fresh = dir.join("fresh");
        let w = Words::update(&fresh, |w| w.spoken_formatting = true).unwrap();
        assert!(w.spoken_formatting && Words::load(&fresh).spoken_formatting);
        // The Words page saves only through Words::update.
        let remote = include_str!("remote.rs");
        assert!(remote.contains("crate::store::Words::update(&data_dir(), f)"));
        assert!(!remote.contains("w.save(&data_dir())"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hotkey_strings_parse() {
        for s in ["Ctrl+Alt+Space", "Ctrl+Shift+D", "Alt+F9"] {
            assert!(s.parse::<global_hotkey::hotkey::HotKey>().is_ok(), "{s}");
        }
    }
}
