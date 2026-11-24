//! Voxa's shared Rust core for the Android app, exposed to Kotlin through UniFFI.
//!
//! The Kotlin side is only UI and platform glue (the keyboard view, the microphone, the sign-in
//! WebView, the Keystore). Everything else is here, shared with the desktop through `voxa-core`,
//! `voxa-chatgpt` and `voxa-store`:
//!
//! - [`VoxaFlow`]: voxa-core's dictation state machine (events in, effects out), the status-line
//!   wording, and History kept in step with its jobs from the first moment of recording.
//! - [`VoxaCore`]: the ChatGPT session, the account check, transcription of a recording (silence
//!   rules, trimming, retries, filler removal and the word list), and the saved settings, words and
//!   history.
//! - Free functions: the text pipeline, the meter level, and which fields Voxa may listen in.
//!
//! Never logs or returns cookie or token values.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use voxa_chatgpt::{ChatGpt, Session as GptSession};
use voxa_core::TranscribeError;
use voxa_store::{History, Status, take};

mod diag;
mod flow;
mod records;

pub use diag::{LogSink, set_log_sink};
pub use flow::*;
pub use records::*;

uniffi::setup_scaffolding!();

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Why speech could not be turned into text. Mirrors `voxa_core::TranscribeError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum TranscribeFailure {
    #[error("signed out")]
    SignedOut,
    #[error("session expired")]
    SessionExpired,
    #[error("blocked by Cloudflare")]
    Blocked,
    #[error("offline")]
    Offline,
    #[error("timed out")]
    Timeout,
    #[error("server answered {status}")]
    Server { status: u16 },
}

impl From<TranscribeError> for TranscribeFailure {
    fn from(e: TranscribeError) -> Self {
        match e {
            TranscribeError::SignedOut => Self::SignedOut,
            TranscribeError::SessionExpired => Self::SessionExpired,
            TranscribeError::Blocked => Self::Blocked,
            TranscribeError::Offline => Self::Offline,
            TranscribeError::Timeout => Self::Timeout,
            TranscribeError::Server(status) => Self::Server { status },
        }
    }
}

impl From<TranscribeFailure> for TranscribeError {
    fn from(e: TranscribeFailure) -> Self {
        match e {
            TranscribeFailure::SignedOut => Self::SignedOut,
            TranscribeFailure::SessionExpired => Self::SessionExpired,
            TranscribeFailure::Blocked => Self::Blocked,
            TranscribeFailure::Offline => Self::Offline,
            TranscribeFailure::Timeout => Self::Timeout,
            TranscribeFailure::Server { status } => Self::Server(status),
        }
    }
}

/// The signed-in ChatGPT account. Never carries tokens.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct AccountInfo {
    pub name: String,
    pub email: String,
    pub image: String,
    /// "ChatGPT Plus", "ChatGPT Free".
    pub plan_label: String,
}

impl From<&voxa_chatgpt::Account> for AccountInfo {
    fn from(a: &voxa_chatgpt::Account) -> Self {
        Self { name: a.name.clone(), email: a.email.clone(), image: a.image.clone(), plan_label: a.plan_label() }
    }
}

/// The result of checking the session: what the flow's readiness should say.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SessionCheck {
    pub session: SessionState,
    pub network: NetworkState,
    pub account: Option<AccountInfo>,
    /// ChatGPT answered with Cloudflare's challenge or block (HTML or `cf-mitigated`).
    pub blocked: bool,
}

/// The session, settings, words and history in one data folder, plus the ChatGPT client.
///
/// The session lives only in memory here: the app stores it encrypted (Android Keystore) and
/// hands it over with [`VoxaCore::use_session`] at start.
#[derive(uniffi::Object)]
pub struct VoxaCore {
    dir: PathBuf,
    gpt: OnceLock<ChatGpt>,
    session: Mutex<Option<GptSession>>,
}

impl VoxaCore {
    fn gpt(&self) -> &ChatGpt {
        self.gpt.get_or_init(ChatGpt::new)
    }

    fn session(&self) -> Option<GptSession> {
        self.session.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn store_settings(&self) -> voxa_store::Settings {
        voxa_store::Settings::load(&self.dir)
    }
}

#[uniffi::export]
impl VoxaCore {
    /// `data_dir`: the app's private files folder.
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Arc<Self> {
        let dir = PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self { dir, gpt: OnceLock::new(), session: Mutex::new(None) })
    }

    // ── session ──────────────────────────────────────────────────────────────

    /// Use the session captured by the sign-in WebView: its full cookie header (including
    /// cf_clearance) and the exact User-Agent it was issued to (Cloudflare binds them).
    pub fn use_session(&self, cookie_header: String, user_agent: String) {
        let s = (!cookie_header.trim().is_empty())
            .then(|| GptSession { cookie_header, user_agent, device_id: voxa_chatgpt::device_id(&self.dir) });
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }

    pub fn has_session(&self) -> bool {
        self.session().is_some()
    }

    pub fn sign_out(&self) {
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = std::fs::remove_file(self.dir.join("account.json"));
    }

    /// Check the session with ChatGPT (network; call off the main thread). Same mapping as the
    /// desktop: only a real rejection means signed out; being offline or blocked does not.
    pub fn check_session(&self) -> SessionCheck {
        let check = |session, network, account, blocked| SessionCheck { session, network, account, blocked };
        let Some(s) = self.session() else {
            diag::log(|| "session check: no session saved".into());
            return check(SessionState::Expired, NetworkState::Up, None, false);
        };
        let t = std::time::Instant::now();
        let result = self.gpt().account(&s);
        diag::log(|| format!("session check: {} in {} ms", match &result { Ok(_) => "ok".to_string(), Err(e) => format!("{e:?}") }, t.elapsed().as_millis()));
        match result {
            Ok(a) => {
                if let Ok(b) = serde_json::to_vec_pretty(&a) {
                    let _ = voxa_store::write_atomic(&self.dir.join("account.json"), &b);
                }
                check(SessionState::Valid, NetworkState::Up, Some(AccountInfo::from(&a)), false)
            }
            Err(TranscribeError::SignedOut | TranscribeError::SessionExpired) => check(SessionState::Expired, NetworkState::Up, None, false),
            Err(TranscribeError::Offline) => check(SessionState::Unknown, NetworkState::Down, None, false),
            Err(TranscribeError::Blocked) => check(SessionState::Unknown, NetworkState::Up, None, true),
            Err(_) => check(SessionState::Unknown, NetworkState::Up, None, false),
        }
    }

    /// The account from the last successful check, without a network call.
    pub fn cached_account(&self) -> Option<AccountInfo> {
        let a: voxa_chatgpt::Account = serde_json::from_slice(&std::fs::read(self.dir.join("account.json")).ok()?).ok()?;
        Some(AccountInfo::from(&a))
    }

    // ── transcription ────────────────────────────────────────────────────────

    /// Turn a recording (16 kHz mono PCM16 file) into finished text for `app`: the silence check,
    /// long-pause trimming, upload with up to three tries, Whisper's silence inventions dropped,
    /// then filler removal and the word list. Blocking network call. Empty text means no speech.
    pub fn transcribe_recording(&self, pcm_path: String, app: String) -> Result<String, TranscribeFailure> {
        let Some(session) = self.session() else {
            diag::log(|| "transcribe: signed out, not sent".into());
            return Err(TranscribeFailure::SignedOut);
        };
        let pcm_bytes = std::fs::metadata(&pcm_path).map(|m| m.len()).unwrap_or(0);
        let Some(p) = voxa_store::audio::prepare_pcm(Path::new(&pcm_path)) else {
            diag::log(|| format!("transcribe: recording unreadable ({pcm_bytes} bytes)"));
            return Err(TranscribeFailure::Server { status: 0 });
        };
        diag::log(|| format!("transcribe: recording {pcm_bytes} bytes, {:.1} s, peak rms {:.4}, upload {} bytes", p.secs, p.peak, p.wav.len()));
        if p.is_silent() {
            diag::log(|| "transcribe: silent take, not sent".into());
            return Ok(String::new());
        }
        let mut tries = 0;
        let t = std::time::Instant::now();
        let result = take::with_retries(|| {
            tries += 1;
            self.gpt().transcribe(&session, p.wav.clone(), "recording.wav", "audio/wav")
        });
        diag::log(|| match &result {
            Ok(text) => format!("transcribe: ok (HTTP 2xx) in {} ms, {tries} tries, text {}", t.elapsed().as_millis(), diag::text(text)),
            Err(e) => format!("transcribe: failed {e:?} in {} ms, {tries} tries", t.elapsed().as_millis()),
        });
        let text = result?;
        if take::invented_for_silence(&text, p.peak) {
            diag::log(|| "transcribe: dropped Whisper's silence phrase".into());
            return Ok(String::new());
        }
        let out = voxa_store::finish_text(text, &self.dir, &app);
        diag::log(|| format!("transcribe: finished text {}", diag::text(&out)));
        Ok(out)
    }

    /// History's "Send now" (waiting or failed) or "Write it out again". The result stays in
    /// History as "Not pasted"; it is never inserted into whatever field is open now.
    pub fn resend(&self, entry_id: u64) -> Option<HistoryEntry> {
        let entry = History::edit(&self.dir, |h| (false, h.get(entry_id).cloned()))?;
        let audio = entry.audio.clone()?.to_string_lossy().to_string();
        let updated = match self.transcribe_recording(audio, entry.app.clone()) {
            Ok(t) if !t.trim().is_empty() => voxa_store::Entry { text: t, status: Status::Ready, ..entry },
            Ok(_) => voxa_store::Entry { status: Status::Failed("No speech in the recording".into()), ..entry },
            Err(e) if TranscribeError::from(e).needs_sign_in() => voxa_store::Entry { status: Status::Waiting("Sign in to send".into()), ..entry },
            Err(_) => voxa_store::Entry { status: Status::Failed("Couldn't reach ChatGPT".into()), ..entry },
        };
        let u = updated.clone();
        History::edit(&self.dir, move |h| (h.upsert(u), ()));
        Some(updated.into())
    }

    /// Filler removal (if on in Settings), then the word list for `app`.
    pub fn finish_text(&self, text: String, app: String) -> String {
        voxa_store::finish_text(text, &self.dir, &app)
    }

    // ── settings, words, history ─────────────────────────────────────────────

    pub fn settings(&self) -> Settings {
        let s = self.store_settings();
        Settings {
            remove_fillers: s.remove_fillers,
            keep_days: s.keep_days,
            onboarded: s.onboarded,
            hold_to_talk: s.hold_to_talk,
            haptics: s.haptics,
            spoken_formatting: voxa_store::Words::load(&self.dir).spoken_formatting,
        }
    }

    pub fn set_settings(&self, settings: Settings) {
        let s = voxa_store::Settings {
            remove_fillers: settings.remove_fillers,
            keep_days: settings.keep_days,
            onboarded: settings.onboarded,
            hold_to_talk: settings.hold_to_talk,
            haptics: settings.haptics,
            ..self.store_settings()
        };
        s.save(&self.dir);
        let mut w = voxa_store::Words::load(&self.dir);
        if w.spoken_formatting != settings.spoken_formatting {
            w.spoken_formatting = settings.spoken_formatting;
            w.save(&self.dir);
        }
        History::edit(&self.dir, |h| (h.prune(s.keep_days, now_ms()) > 0, ()));
    }

    pub fn words(&self) -> Words {
        voxa_store::Words::load(&self.dir).into()
    }

    pub fn set_words(&self, words: Words) {
        voxa_store::Words::from(words).save(&self.dir);
    }

    /// Saved dictations matching `query` (text or app; empty for all), newest first, after dropping
    /// written-out ones older than the keep setting.
    pub fn history(&self, query: String) -> Vec<HistoryEntry> {
        let keep = self.store_settings().keep_days;
        History::edit(&self.dir, |h| {
            let pruned = h.prune(keep, now_ms()) > 0;
            (pruned, h.search(&query).into_iter().cloned().map(HistoryEntry::from).collect())
        })
    }

    /// Bytes of recordings kept for History.
    pub fn history_audio_bytes(&self) -> u64 {
        History::edit(&self.dir, |h| (false, h.audio_bytes()))
    }

    pub fn delete_entry(&self, entry_id: u64) {
        History::edit(&self.dir, |h| {
            if let Some(a) = h.get(entry_id).and_then(|e| e.audio.clone()) {
                let _ = std::fs::remove_file(a);
            }
            (h.remove(entry_id), ())
        });
    }

    pub fn clear_history(&self) {
        History::edit(&self.dir, |h| {
            h.delete_all();
            (false, ())
        });
    }
}

// ── free functions ───────────────────────────────────────────────────────────

/// The core's version, also a cheap "the native library loaded" check.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[uniffi::export]
pub fn remove_fillers(text: String) -> String {
    voxa_core::text::remove_fillers(&text)
}

#[uniffi::export]
pub fn spoken_formatting(text: String) -> String {
    voxa_core::text::spoken_formatting(&text)
}

#[uniffi::export]
pub fn replace_phrases(text: String, pairs: Vec<WordPair>) -> String {
    let pairs: Vec<(String, String)> = pairs.into_iter().map(|p| (p.from, p.to)).collect();
    voxa_core::text::replace_phrases(&text, &pairs)
}

/// The whole after-transcription pipeline with explicit settings (no files): fillers if
/// `remove_fillers`, then fixes, snippets, spoken formatting and the rule for `app`.
#[uniffi::export]
pub fn clean_text(text: String, remove_fillers: bool, words: Words, app: String) -> String {
    let text = if remove_fillers { voxa_core::text::remove_fillers(&text) } else { text };
    voxa_store::Words::from(words).apply(&text, &app)
}

/// After the user edits a dictation, the one short phrase they changed, if that is all: offers
/// "Always change X to Y?".
#[uniffi::export]
pub fn changed_phrase(before: String, after: String) -> Option<WordPair> {
    voxa_core::text::changed_phrase(&before, &after).map(|(from, to)| WordPair { from, to })
}

/// Meter level 0..1 for a chunk of 16-bit little-endian PCM (the live ink line).
#[uniffi::export]
pub fn meter_level(pcm: Vec<u8>) -> f32 {
    voxa_store::audio::meter_level(&voxa_store::audio::samples_from_pcm(&pcm))
}

/// Whether Voxa may listen in a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    Normal,
    /// Password or PIN: the mic is off; nothing is ever recorded.
    Password,
    /// Incognito or "no personalised learning": dictation works but nothing is kept.
    Private,
}

/// Decide from the field's `EditorInfo.inputType` and `imeOptions` (Android's raw ints).
#[uniffi::export]
pub fn field_kind(input_type: i32, ime_options: i32) -> FieldKind {
    const TYPE_MASK_CLASS: i32 = 0x0000_000f;
    const TYPE_MASK_VARIATION: i32 = 0x0000_0ff0;
    const CLASS_TEXT: i32 = 1;
    const CLASS_NUMBER: i32 = 2;
    const TEXT_PASSWORD: i32 = 0x80;
    const TEXT_VISIBLE_PASSWORD: i32 = 0x90;
    const TEXT_WEB_PASSWORD: i32 = 0xe0;
    const NUMBER_PASSWORD: i32 = 0x10;
    const IME_FLAG_NO_PERSONALIZED_LEARNING: i32 = 0x0100_0000;
    let class = input_type & TYPE_MASK_CLASS;
    let variation = input_type & TYPE_MASK_VARIATION;
    let password = match class {
        CLASS_TEXT => matches!(variation, TEXT_PASSWORD | TEXT_VISIBLE_PASSWORD | TEXT_WEB_PASSWORD),
        CLASS_NUMBER => variation == NUMBER_PASSWORD,
        _ => false,
    };
    if password {
        FieldKind::Password
    } else if ime_options & IME_FLAG_NO_PERSONALIZED_LEARNING != 0 {
        FieldKind::Private
    } else {
        FieldKind::Normal
    }
}

/// What the keyboard says in a field where it won't listen.
#[uniffi::export]
pub fn password_field_text() -> StatusText {
    StatusText { title: "Voxa doesn't listen in password fields".into(), detail: Some("Switch keyboards to type it".into()), action: None, dismissable: false }
}
