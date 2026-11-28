//! UniFFI mirrors of voxa-store's types (the store itself has no FFI dependency).

use voxa_store as vs;

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum EntryStatus {
    /// Inserted into the field the user was typing in.
    Pasted,
    /// Written out but not inserted ("Not pasted"): ready to copy or insert.
    Ready,
    /// Not written yet; the reason says what it is waiting for ("Not sent").
    Waiting(String),
    Failed(String),
}

impl From<vs::Status> for EntryStatus {
    fn from(s: vs::Status) -> Self {
        match s {
            vs::Status::Pasted => Self::Pasted,
            vs::Status::Ready => Self::Ready,
            vs::Status::Waiting(r) => Self::Waiting(r),
            vs::Status::Failed(r) => Self::Failed(r),
        }
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct HistoryEntry {
    /// Milliseconds since the epoch when recording started. Also the entry's identity.
    pub id: u64,
    pub app: String,
    pub text: String,
    pub secs: f32,
    pub status: EntryStatus,
    /// Whether the recording is still on the phone (so it can be sent again).
    pub has_audio: bool,
    /// The recording (16 kHz mono PCM16, raw), when it is still on the phone. For playback.
    pub audio_path: Option<String>,
    pub started_in: Option<String>,
}

impl From<vs::Entry> for HistoryEntry {
    fn from(e: vs::Entry) -> Self {
        Self {
            id: e.id,
            app: e.app,
            text: e.text,
            secs: e.secs,
            status: e.status.into(),
            has_audio: e.audio.as_ref().is_some_and(|a| a.exists()),
            audio_path: e.audio.as_ref().filter(|a| a.exists()).map(|a| a.to_string_lossy().to_string()),
            started_in: e.started_in,
        }
    }
}

/// Home's "words this week": Monday first, local time (the desktop's Insights rules).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WeekStats {
    pub words: u64,
    /// Words per day, Monday first (7 values).
    pub per_day: Vec<u32>,
    /// Index of today in `per_day`.
    pub today: u8,
}

impl From<vs::Week> for WeekStats {
    fn from(w: vs::Week) -> Self {
        Self { words: w.words as u64, per_day: w.per_day.iter().map(|&n| n as u32).collect(), today: w.today as u8 }
    }
}

/// The settings Android shows. Saved into the shared settings.json (other fields untouched).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Settings {
    pub remove_fillers: bool,
    /// Days of history to keep; 0 = forever.
    pub keep_days: u32,
    pub onboarded: bool,
    pub hold_to_talk: bool,
    pub haptics: bool,
    /// Spoken "new line", "new paragraph", "bullet point" (stored in the word list).
    pub spoken_formatting: bool,
}

#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct WordPair {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct AppRule {
    /// The app's label as Android shows it ("WhatsApp").
    pub app: String,
    pub no_final_period: bool,
    pub lowercase: bool,
}

#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct Words {
    pub fixes: Vec<WordPair>,
    pub snippets: Vec<WordPair>,
    pub spoken_formatting: bool,
    pub rules: Vec<AppRule>,
}

impl From<vs::Words> for Words {
    fn from(w: vs::Words) -> Self {
        let pairs = |v: Vec<vs::Pair>| v.into_iter().map(|p| WordPair { from: p.from, to: p.to }).collect();
        Self {
            fixes: pairs(w.fixes),
            snippets: pairs(w.snippets),
            spoken_formatting: w.spoken_formatting,
            rules: w.rules.into_iter().map(|r| AppRule { app: r.app, no_final_period: r.no_final_period, lowercase: r.lowercase }).collect(),
        }
    }
}

impl From<Words> for vs::Words {
    fn from(w: Words) -> Self {
        let pairs = |v: Vec<WordPair>| v.into_iter().map(|p| vs::Pair { from: p.from, to: p.to }).collect();
        Self {
            fixes: pairs(w.fixes),
            snippets: pairs(w.snippets),
            spoken_formatting: w.spoken_formatting,
            rules: w.rules.into_iter().map(|r| vs::AppRule { app: r.app, no_final_period: r.no_final_period, lowercase: r.lowercase }).collect(),
        }
    }
}
