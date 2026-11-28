//! What Voxa keeps, and the rules around a take, shared by every app.
//!
//! Copied out of the desktop crate so Android can use the same code (the desktop switches over
//! when this is merged):
//! - [`History`], [`Entry`], [`Status`], [`Settings`], [`Words`], [`Pair`], [`AppRule`]: from
//!   `desktop/app/src/store.rs` (Insights/stats not copied yet). Same JSON files and fields.
//! - [`finish_text`]: from `desktop/app/src/prepare.rs`.
//! - [`take`]: the silence rules and the 3-try upload retry from `desktop/app/src/main.rs`.
//! - [`audio`]: preparing a recording for upload (long-silence trimming), from `prepare.rs`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub mod audio;
pub mod take;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Status {
    /// Pasted (inserted) into the app the user was in.
    Pasted,
    /// Written out but not pasted (the user moved, or paste failed): ready to copy.
    Ready,
    /// Not written yet; the reason says what it is waiting for.
    Waiting(String),
    Failed(String),
}

impl Status {
    /// Still owed to the user: never pruned, whatever the keep setting says.
    pub fn is_unsent(&self) -> bool {
        matches!(self, Status::Waiting(_) | Status::Failed(_))
    }
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
}

/// Write `bytes` to `path` so a reader only ever sees the old file or the new one, whole: a
/// temporary file with a name no other writer uses (threads or processes), flushed to the disk
/// before it is renamed over `path` (a power cut right after the rename can't leave an empty
/// file). A failure removes the temporary file and is reported.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!("{name}.{}-{n}.tmp", std::process::id()));
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    // Windows refuses a rename over a file another writer is renaming at the same moment; a
    // few short tries settle it.
    let mut result = written.and_then(|()| std::fs::rename(&tmp, path));
    for _ in 0..20 {
        if result.is_ok() || !tmp.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        result = std::fs::rename(&tmp, path);
    }
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Keep a copy of a file that can't be read as expected next to it, as `<stem>.broken-<ms>.json`,
/// so nothing is lost when Voxa starts again from what it could read.
fn keep_broken_copy(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    // Read again while it stays damaged: one copy is enough.
    let bytes = std::fs::read(path).ok()?;
    let prefix = format!("{stem}.broken-");
    if let Ok(files) = std::fs::read_dir(path.parent()?) {
        for f in files.flatten() {
            let same_name = f.file_name().to_str().is_some_and(|n| n.starts_with(&prefix));
            if same_name && f.metadata().is_ok_and(|m| m.len() == bytes.len() as u64) && std::fs::read(f.path()).is_ok_and(|b| b == bytes) {
                return Some(f.path());
            }
        }
    }
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let copy = path.with_file_name(format!("{stem}.broken-{ms}.json"));
    std::fs::copy(path, &copy).ok()?;
    Some(copy)
}

/// What a damaged history file still holds: the entries before the point where it was cut off
/// (a torn write keeps the start of the array). Empty when nothing can be read.
fn salvage_entries(bytes: &[u8]) -> Vec<Entry> {
    let Ok(text) = std::str::from_utf8(bytes) else { return Vec::new() };
    // Try the longest prefix that ends after a whole entry, closing the array there.
    let ends: Vec<usize> = text.match_indices('}').map(|(i, _)| i + 1).collect();
    for &end in ends.iter().rev().take(4000) {
        if let Ok(v) = serde_json::from_str::<Vec<Entry>>(&format!("{}]", &text[..end])) {
            return v;
        }
    }
    Vec::new()
}

/// One lock for every read-modify-write of history.json in this process (Android's keyboard and
/// app screens share the file).
static HISTORY_LOCK: Mutex<()> = Mutex::new(());

/// A change to History queued with [`History::edit_later`]: returns whether it changed anything.
type LaterEdit = Box<dyn FnOnce(&mut History) -> bool + Send>;

/// Changes waiting for the writer thread, oldest first. Every [`History::edit`] applies them first
/// (under [`HISTORY_LOCK`]), so a reader always sees every change queued before it, in order.
static LATER: Mutex<std::collections::VecDeque<(PathBuf, LaterEdit)>> = Mutex::new(std::collections::VecDeque::new());
static LATER_READY: std::sync::Condvar = std::sync::Condvar::new();
static WRITER: std::sync::Once = std::sync::Once::new();

/// Apply every queued change. Call with [`HISTORY_LOCK`] held. Consecutive changes to one folder
/// share one load and one save.
fn apply_later_locked() {
    loop {
        let batch: Vec<(PathBuf, LaterEdit)> = LATER.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect();
        if batch.is_empty() {
            return;
        }
        let mut open: Option<(PathBuf, History, bool)> = None;
        for (dir, f) in batch {
            if open.as_ref().is_some_and(|(d, _, _)| *d != dir) {
                if let Some((_, h, true)) = open.take() {
                    let _ = h.save();
                }
            }
            let (_, h, changed) = open.get_or_insert_with(|| (dir.clone(), History::load(&dir), false));
            *changed |= f(h);
        }
        if let Some((_, h, true)) = open {
            let _ = h.save();
        }
    }
}

/// How history.json read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// Read whole, or there is no file yet.
    Clean,
    /// It didn't parse (a torn write, a newer app's format). A copy was kept aside as
    /// `history.broken-<ms>.json` and what could be read was salvaged. Saving is safe (the copy
    /// keeps the rest); deleting by what History lacks (orphan sweeps, pruning) is not.
    Damaged,
    /// It couldn't be read at all (locked, a read error). Nothing may be saved over it.
    Unreadable,
}

pub struct History {
    path: PathBuf,
    /// Oldest first.
    pub entries: Vec<Entry>,
    /// How the file read: anything but `Clean` turns off pruning, and `Unreadable` saving.
    pub loaded: Loaded,
}

impl History {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.json");
        let (entries, loaded) = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), Loaded::Clean),
            Err(_) => (Vec::new(), Loaded::Unreadable),
            Ok(b) => match serde_json::from_slice::<Vec<Entry>>(&b) {
                Ok(v) => (v, Loaded::Clean),
                Err(_) => match keep_broken_copy(&path) {
                    Some(_) => (salvage_entries(&b), Loaded::Damaged),
                    // No copy could be made: never write over the only one.
                    None => (salvage_entries(&b), Loaded::Unreadable),
                },
            },
        };
        Self { path, entries, loaded }
    }

    /// Save, unless the file couldn't be read (then the copy on disk is the only one).
    pub fn save(&self) -> std::io::Result<()> {
        if self.loaded == Loaded::Unreadable {
            return Err(std::io::Error::other("history.json could not be read; not saved over"));
        }
        let bytes = serde_json::to_vec_pretty(&self.entries).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes)
    }

    /// Load, change and save under the process-wide lock. `f` returns (changed, result). Changes
    /// queued with [`History::edit_later`] are applied first.
    pub fn edit<R>(dir: &Path, f: impl FnOnce(&mut History) -> (bool, R)) -> R {
        let _g = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        apply_later_locked();
        let mut h = Self::load(dir);
        let (changed, r) = f(&mut h);
        if changed {
            let _ = h.save();
        }
        r
    }

    /// Queue a change for a writer thread, so the caller (the keyboard's main thread) never waits
    /// for the disk. Changes apply in the order they were queued, and any [`History::edit`] in
    /// this process applies what is queued before it reads. `f` returns whether it changed
    /// anything.
    pub fn edit_later(dir: &Path, f: impl FnOnce(&mut History) -> bool + Send + 'static) {
        LATER.lock().unwrap_or_else(|e| e.into_inner()).push_back((dir.to_path_buf(), Box::new(f)));
        WRITER.call_once(|| {
            let _ = std::thread::Builder::new().name("voxa-history".into()).spawn(|| loop {
                {
                    let q = LATER.lock().unwrap_or_else(|e| e.into_inner());
                    let _q = LATER_READY.wait_while(q, |q| q.is_empty()).unwrap_or_else(|e| e.into_inner());
                }
                let _g = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                apply_later_locked();
            });
        });
        LATER_READY.notify_one();
    }

    /// Apply every change queued with [`History::edit_later`] now.
    pub fn flush() {
        let _g = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        apply_later_locked();
    }

    /// Whether a recording no entry lists may be deleted (`file_name` starts with the ms it was
    /// recorded at). Only when History read cleanly, and never for a recording made before
    /// History was found damaged: its entry may have been in the part that was lost.
    pub fn may_delete_unlisted_audio(&self, file_name: &str) -> bool {
        if self.loaded != Loaded::Clean {
            return false;
        }
        let Some(dir) = self.path.parent() else { return false };
        let Ok(files) = std::fs::read_dir(dir) else { return false };
        let recorded: Option<u128> = file_name.split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok());
        for f in files.flatten() {
            let n = f.file_name();
            let Some(damaged_at) = n.to_str().and_then(|n| n.strip_prefix("history.broken-")?.strip_suffix(".json")?.parse::<u128>().ok()) else { continue };
            if recorded.is_none_or(|r| r <= damaged_at) {
                return false;
            }
        }
        true
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

    /// Replace a written entry's text (the user edited it). Unsent entries have no text to edit.
    /// Returns whether anything changed.
    pub fn set_text(&mut self, id: u64, text: &str) -> bool {
        match self.entries.iter_mut().find(|e| e.id == id) {
            Some(e) if !e.status.is_unsent() && e.text != text => {
                e.text = text.to_string();
                true
            }
            _ => false,
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        before != self.entries.len()
    }

    /// Drop entries (and their audio) older than `keep_days`. 0 keeps everything. Dictations not
    /// written out yet (waiting or failed) are never dropped: they are still owed to the user.
    ///
    /// The wall clock is not trusted alone: age is counted up to the newest dictation at most, so
    /// a clock that jumps ahead (a dead clock battery, a wrong network time, a date set forward)
    /// never deletes recent History. Nothing is dropped from a History that didn't read cleanly.
    pub fn prune(&mut self, keep_days: u32, now_ms: u64) -> usize {
        if keep_days == 0 || self.loaded != Loaded::Clean {
            return 0;
        }
        let newest = self.entries.iter().map(|e| e.id).max().unwrap_or(0);
        let cutoff = now_ms.min(newest).saturating_sub(keep_days as u64 * 86_400_000);
        let (old, keep): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| e.id < cutoff && !e.status.is_unsent());
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
        let _ = self.save();
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
    /// Hold the shortcut (or the mic key) while talking; a quick tap still toggles.
    pub hold_to_talk: bool,
    /// Microphone chosen in Settings, by name; None follows the system default.
    pub mic_device: Option<String>,
    /// Leave the dictation on the clipboard after pasting (desktop only).
    pub keep_on_clipboard: bool,
    /// A short vibration on the keyboard's keys (Android).
    pub haptics: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remove_fillers: false,
            keep_days: 30,
            hotkey: "Ctrl+Alt+Space".into(),
            onboarded: false,
            imported_old_voxa: false,
            hold_to_talk: false,
            mic_device: None,
            keep_on_clipboard: true,
            haptics: true,
        }
    }
}

impl Settings {
    /// The saved settings, or the defaults when there are none or they can't be read.
    pub fn load(dir: &Path) -> Self {
        Self::load_checked(dir).unwrap_or_default()
    }

    /// The saved settings (the defaults when there is no file yet), or None when the file exists
    /// but can't be read or parsed: then nothing may be decided from them (pruning by
    /// `keep_days` would use the default 30 days for someone who keeps History forever). A copy
    /// of a damaged file is kept aside.
    pub fn load_checked(dir: &Path) -> Option<Self> {
        let path = dir.join("settings.json");
        match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Self::default()),
            Err(_) => None,
            Ok(b) => match serde_json::from_slice(&b) {
                Ok(s) => Some(s),
                Err(_) => {
                    keep_broken_copy(&path);
                    None
                }
            },
        }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(&dir.join("settings.json"), &bytes)
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
/// spoken formatting, and per-app style. Applied on the device after every dictation.
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
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(&dir.join("words.json"), &bytes)
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

/// This week's words, as the desktop's Insights counts them (written dictations only).
#[derive(Clone, Debug, PartialEq)]
pub struct Week {
    pub words: usize,
    /// Words per day, Monday first.
    pub per_day: [usize; 7],
    /// Index of today in `per_day`.
    pub today: usize,
    /// Seconds spoken for those words.
    pub secs: f32,
}

impl Week {
    /// Seconds saved versus typing the same words (the desktop's "faster than typing").
    pub fn saved_secs(&self) -> u32 {
        typing_saved_secs(self.words, self.secs)
    }

    /// Speaking speed in words a minute, 0 when there is too little to tell.
    pub fn wpm(&self) -> u32 {
        if self.secs > 1.0 { (self.words as f32 / (self.secs / 60.0)).round() as u32 } else { 0 }
    }
}

/// Typing speed "faster than typing" compares with: a typical 40 words a minute (as the desktop).
pub const TYPING_WPM: f32 = 40.0;

/// Seconds saved by saying `words` in `secs` instead of typing them at [`TYPING_WPM`]; 0 when
/// speaking was not faster.
pub fn typing_saved_secs(words: usize, secs: f32) -> u32 {
    (words as f32 / TYPING_WPM * 60.0 - secs).max(0.0).round() as u32
}

/// Words in a text, as History counts them.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// The local calendar day (days since 1970-01-01) of `ms`, for a UTC offset in milliseconds.
fn local_day(ms: u64, offset_ms: i64) -> i64 {
    (ms as i64 + offset_ms).div_euclid(86_400_000)
}

/// Words dictated this week (Monday to Sunday, local time), per day. `offset_ms` is the phone's
/// current UTC offset; no date library needed. 1970-01-01 was a Thursday.
pub fn week(entries: &[Entry], now_ms: u64, offset_ms: i64) -> Week {
    let today = local_day(now_ms, offset_ms);
    let monday = today - (today + 3).rem_euclid(7);
    let mut per_day = [0usize; 7];
    let mut secs = 0.0f32;
    for e in entries.iter().filter(|e| matches!(e.status, Status::Pasted | Status::Ready) && !e.text.trim().is_empty()) {
        let d = local_day(e.id, offset_ms) - monday;
        if (0..7).contains(&d) {
            per_day[d as usize] += e.words();
            secs += e.secs;
        }
    }
    Week { words: per_day.iter().sum(), per_day, today: (today - monday) as usize, secs }
}

/// How many dictations each app has, most first (History's "Your apps"). Ties by name.
pub fn app_counts(entries: &[Entry]) -> Vec<(String, usize)> {
    let mut m: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for e in entries.iter().filter(|e| !e.app.is_empty()) {
        *m.entry(e.app.as_str()).or_default() += 1;
    }
    let mut v: Vec<(String, usize)> = m.into_iter().map(|(a, n)| (a.to_string(), n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

/// After transcription: filler removal (if on), then the user's word list for `app`.
pub fn finish_text(text: String, dir: &Path, app: &str) -> String {
    let text = if Settings::load(dir).remove_fillers { voxa_core::text::remove_fillers(&text) } else { text };
    Words::load(dir).apply(&text, app)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voxa-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn entry(id: u64, text: &str, status: Status) -> Entry {
        Entry { id, app: "A".into(), text: text.into(), secs: 1.0, status, audio: None, started_in: None, via: None }
    }

    #[test]
    fn history_upsert_keeps_order_and_persists() {
        let dir = tmp("history");
        let mut h = History::load(&dir);
        assert!(h.upsert(entry(20, "b", Status::Pasted)));
        assert!(h.upsert(entry(10, "a", Status::Pasted)));
        assert!(!h.upsert(entry(10, "a", Status::Pasted)), "no change");
        h.save().unwrap();
        let back = History::load(&dir);
        assert_eq!(back.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [10, 20]);
        assert_eq!(back.search("b").len(), 1);
        assert_eq!(back.search("").first().unwrap().id, 20, "newest first");
    }

    #[test]
    fn prune_drops_old_written_entries_but_never_unsent_ones() {
        let dir = tmp("prune");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("old.wav");
        std::fs::write(&wav, b"x").unwrap();
        let day = 86_400_000u64;
        let now = 100 * day;
        let mut h = History::load(&dir);
        let mut old = entry(now - 40 * day, "old", Status::Pasted);
        old.audio = Some(wav.clone());
        h.upsert(old);
        h.upsert(entry(now - 41 * day, "", Status::Waiting("Sign in to send".into())));
        h.upsert(entry(now - 42 * day, "", Status::Failed("Couldn't reach ChatGPT".into())));
        h.upsert(entry(now - day, "new", Status::Pasted));
        assert_eq!(h.prune(30, now), 1);
        assert_eq!(h.entries.len(), 3);
        assert!(!wav.exists());
        assert_eq!(h.prune(0, u64::MAX), 0, "forever keeps all");
    }

    #[test]
    fn week_counts_written_words_monday_first_in_local_time() {
        let day = 86_400_000u64;
        // 1970-01-12 (day 11) was a Monday; "now" is Wednesday 1970-01-14 at noon UTC.
        let monday = 11 * day;
        let now = monday + 2 * day + day / 2;
        let entries = vec![
            entry(monday - 1000, "last sunday not counted", Status::Pasted),
            entry(monday + 1000, "one two three", Status::Pasted),
            entry(monday + 2 * day + 5, "four five", Status::Ready),
            entry(monday + 2 * day + 6, "never sent", Status::Waiting("Sign in to send".into())),
            entry(monday + 2 * day + 7, "   ", Status::Pasted),
        ];
        let w = week(&entries, now, 0);
        assert_eq!(w.secs, 2.0, "one second each for the two counted entries");
        assert_eq!(w.today, 2);
        assert_eq!(w.per_day, [3, 0, 2, 0, 0, 0, 0]);
        assert_eq!(w.words, 5);
        // Two hours east of UTC, the Sunday entry 1 s before midnight UTC is already Monday.
        let w = week(&entries, now, 2 * 3_600_000);
        assert_eq!(w.per_day[0], 3 + 4);
    }

    #[test]
    fn typing_saved_and_speed_follow_the_desktop() {
        // 40 words take a minute to type; said in 15 s that saves 45 s.
        assert_eq!(typing_saved_secs(40, 15.0), 45);
        assert_eq!(typing_saved_secs(2, 30.0), 0, "slower than typing saves nothing");
        let w = Week { words: 9, per_day: [9, 0, 0, 0, 0, 0, 0], today: 0, secs: 15.0 };
        assert_eq!(w.wpm(), 36);
        assert_eq!(Week { secs: 0.5, ..w.clone() }.wpm(), 0);
        assert_eq!(word_count("  one two
three "), 3);
    }

    #[test]
    fn app_counts_most_first() {
        let mut a = entry(1, "x", Status::Pasted);
        a.app = "Gmail".into();
        let mut b = entry(2, "y", Status::Pasted);
        b.app = "WhatsApp".into();
        let mut c = entry(3, "z", Status::Ready);
        c.app = "WhatsApp".into();
        let mut d = entry(4, "", Status::Waiting("x".into()));
        d.app = String::new();
        assert_eq!(app_counts(&[a, b, c, d]), vec![("WhatsApp".to_string(), 2), ("Gmail".to_string(), 1)]);
    }

    #[test]
    fn set_text_edits_written_entries_only() {
        let mut h = History { path: PathBuf::new(), loaded: Loaded::Clean, entries: vec![entry(1, "a", Status::Pasted), entry(2, "", Status::Waiting("x".into()))] };
        assert!(h.set_text(1, "b"));
        assert!(!h.set_text(1, "b"), "no change");
        assert!(!h.set_text(2, "c"), "unsent entries are not edited");
        assert_eq!(h.get(1).unwrap().text, "b");
    }

    #[test]
    fn settings_default_and_roundtrip() {
        let dir = tmp("settings");
        let s = Settings::load(&dir);
        assert_eq!(s, Settings::default());
        let s2 = Settings { remove_fillers: true, keep_days: 7, ..s };
        s2.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s2);
        std::fs::write(dir.join("settings.json"), r#"{"remove_fillers": true}"#).unwrap();
        assert_eq!(Settings::load(&dir).keep_days, 30, "older files still load");
    }

    #[test]
    fn words_apply_in_order_and_finish_text_follows_settings() {
        let dir = tmp("words");
        let w = Words {
            fixes: vec![Pair { from: "clawed code".into(), to: "Claude Code".into() }],
            snippets: vec![Pair { from: "my email".into(), to: "sam@example.com".into() }],
            spoken_formatting: true,
            rules: vec![AppRule { app: "Claude Code".into(), no_final_period: true, lowercase: false }],
        };
        w.save(&dir).unwrap();
        assert_eq!(Words::load(&dir), w);
        assert_eq!(w.apply("Ask clawed code. New line. Mail my email.", "Claude Code"), "Ask Claude Code.\nMail sam@example.com");
        assert_eq!(finish_text("Um, ask clawed code.".into(), &dir, "Slack"), "Um, ask Claude Code.", "fillers off by default");
        Settings { remove_fillers: true, ..Settings::default() }.save(&dir).unwrap();
        assert_eq!(finish_text("Um, ask clawed code.".into(), &dir, "Slack"), "Ask Claude Code.");
    }
}
