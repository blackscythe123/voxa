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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use voxa_chatgpt::{ChatGpt, Session as GptSession};
use voxa_core::TranscribeError;
use voxa_store::{History, Status, take};

mod claims;
mod diag;
mod flow;
mod greeting;
mod nudge;
mod notices;
mod records;

pub use diag::{LogSink, set_log_sink};
pub use flow::*;
pub use greeting::*;
pub use nudge::*;
pub use notices::*;
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
    /// ChatGPT's session call answered 2xx with no token for a session we had: the only answer
    /// that may lead to a "signed out" notification (a 401 or 403 may be a rate limit).
    pub definite: bool,
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
    /// Goes up on every new session and sign-out: work that started with an older session
    /// never writes anything back.
    session_gen: AtomicU64,
}

/// Holds an entry's send claim (see [`claims`]) until dropped.
pub(crate) struct SendClaim<'a> {
    core: &'a VoxaCore,
    id: u64,
}

impl Drop for SendClaim<'_> {
    fn drop(&mut self) {
        claims::release(&self.core.dir, self.id);
    }
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

    /// Claim an entry for sending; None when it is already being sent (by the keyboard, History's
    /// "Send now" or the background send).
    pub(crate) fn claim(&self, id: u64) -> Option<SendClaim<'_>> {
        // Built only when claimed: a dropped claim releases the entry.
        if claims::try_claim(&self.dir, id) { Some(SendClaim { core: self, id }) } else { None }
    }

    /// Upload a prepared recording: in parts when it is longer than one upload may carry, the
    /// text joined. Each part is uploaded once (see `ChatGpt::transcribe`).
    fn upload(&self, session: &GptSession, p: &voxa_store::audio::Prepared) -> Result<String, TranscribeError> {
        let mut texts: Vec<String> = Vec::new();
        for part in p.parts() {
            let wav = p.wav(part).ok_or(TranscribeError::Server(0))?;
            let t = self.gpt().transcribe(session, wav, "recording.wav", "audio/wav")?;
            if !t.trim().is_empty() {
                texts.push(t.trim().to_string());
            }
        }
        Ok(texts.join(" "))
    }

    /// Write how an upload ended into an entry still waiting for it. An entry deleted, or sent
    /// another way meanwhile, is left as it is.
    fn record_result(&self, entry_id: u64, result: &Result<String, TranscribeFailure>) {
        let (status, text) = match result {
            Ok(t) if t.trim().is_empty() => (Status::Failed(TranscribeError::Server(204).reason().into()), String::new()),
            Ok(t) => (Status::Ready, t.clone()),
            Err(e) => {
                let e = TranscribeError::from(*e);
                if e.needs_sign_in() {
                    (Status::Waiting(notices::WAITING_FOR_SIGN_IN.into()), String::new())
                } else if e == TranscribeError::Offline {
                    (Status::Waiting(notices::WAITING_FOR_NETWORK.into()), String::new())
                } else {
                    (Status::Failed(e.reason().into()), String::new())
                }
            }
        };
        History::edit(&self.dir, move |h| {
            let Some(now) = h.get(entry_id).filter(|e| e.status.is_unsent()).cloned() else { return (false, ()) };
            (h.upsert(voxa_store::Entry { status, text, ..now }), ())
        });
    }
}

#[uniffi::export]
impl VoxaCore {
    /// `data_dir`: the app's private files folder.
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Arc<Self> {
        let dir = PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self { dir, gpt: OnceLock::new(), session: Mutex::new(None), session_gen: AtomicU64::new(0) })
    }

    // ── session ──────────────────────────────────────────────────────────────

    /// Use the session captured by the sign-in WebView: its full cookie header (including
    /// cf_clearance) and the exact User-Agent it was issued to (Cloudflare binds them).
    pub fn use_session(&self, cookie_header: String, user_agent: String) {
        let s = (!cookie_header.trim().is_empty())
            .then(|| GptSession { cookie_header, user_agent, device_id: voxa_chatgpt::device_id(&self.dir) });
        let mut g = self.session.lock().unwrap_or_else(|e| e.into_inner());
        self.session_gen.fetch_add(1, Ordering::SeqCst);
        // Cookies refreshed for the session before this one must never be merged into it.
        self.gpt().clear_refreshed();
        *g = s;
    }

    /// Goes up on every `use_session` and `sign_out`. The app compares it before and after slow
    /// work, and drops the result when the session changed meanwhile.
    pub fn session_generation(&self) -> u64 {
        self.session_gen.load(Ordering::SeqCst)
    }

    /// The sign-in cookies after ChatGPT re-issued them (it does on use, 90 days from then), once,
    /// and only if they were refreshed from the live session: the app saves them in its
    /// encrypted vault. The live session already uses them.
    pub fn take_refreshed_cookies(&self) -> Option<String> {
        let mut g = self.session.lock().unwrap_or_else(|e| e.into_inner());
        let Some(s) = g.as_mut() else {
            self.gpt().clear_refreshed();
            return None;
        };
        let h = self.gpt().take_refreshed(&s.cookie_header)?;
        s.cookie_header = h.clone();
        diag::log(|| "sign-in refreshed by ChatGPT".into());
        Some(h)
    }

    pub fn has_session(&self) -> bool {
        self.session().is_some()
    }

    pub fn sign_out(&self) {
        let mut g = self.session.lock().unwrap_or_else(|e| e.into_inner());
        self.session_gen.fetch_add(1, Ordering::SeqCst);
        self.gpt().clear_refreshed();
        *g = None;
        drop(g);
        let _ = std::fs::remove_file(self.dir.join("account.json"));
    }

    /// Check the session with ChatGPT (network; call off the main thread). Same mapping as the
    /// desktop: only a real rejection means signed out; being offline or blocked does not.
    pub fn check_session(&self) -> SessionCheck {
        let check = |session, network, account, blocked, definite| SessionCheck { session, network, account, blocked, definite };
        let generation = self.session_generation();
        let Some(s) = self.session() else {
            diag::log(|| "session check: no session saved".into());
            return check(SessionState::Expired, NetworkState::Up, None, false, false);
        };
        let t = std::time::Instant::now();
        let answer = self.gpt().check(&s);
        diag::log(|| format!("session check: {} in {} ms", match &answer {
            voxa_chatgpt::SessionAnswer::Valid(_) => "ok".to_string(),
            voxa_chatgpt::SessionAnswer::NoToken => "no token".to_string(),
            voxa_chatgpt::SessionAnswer::Failed(e) => format!("{e:?}"),
        }, t.elapsed().as_millis()));
        match answer {
            voxa_chatgpt::SessionAnswer::Valid(a) => {
                // Signed out or in again while this check ran: its account is not this session's.
                if self.session_generation() == generation {
                    if let Ok(b) = serde_json::to_vec_pretty(&a) {
                        let _ = voxa_store::write_atomic(&self.dir.join("account.json"), &b);
                    }
                }
                check(SessionState::Valid, NetworkState::Up, Some(AccountInfo::from(&a)), false, false)
            }
            voxa_chatgpt::SessionAnswer::NoToken => check(SessionState::Expired, NetworkState::Up, None, false, true),
            voxa_chatgpt::SessionAnswer::Failed(TranscribeError::SignedOut | TranscribeError::SessionExpired) => {
                check(SessionState::Expired, NetworkState::Up, None, false, false)
            }
            voxa_chatgpt::SessionAnswer::Failed(TranscribeError::Offline) => check(SessionState::Unknown, NetworkState::Down, None, false, false),
            voxa_chatgpt::SessionAnswer::Failed(TranscribeError::Blocked) => check(SessionState::Unknown, NetworkState::Up, None, true, false),
            voxa_chatgpt::SessionAnswer::Failed(_) => check(SessionState::Unknown, NetworkState::Up, None, false, false),
        }
    }

    /// The account from the last successful check, without a network call.
    pub fn cached_account(&self) -> Option<AccountInfo> {
        let a: voxa_chatgpt::Account = serde_json::from_slice(&std::fs::read(self.dir.join("account.json")).ok()?).ok()?;
        Some(AccountInfo::from(&a))
    }

    // ── transcription ────────────────────────────────────────────────────────

    /// Turn a recording (16 kHz mono PCM16 file) into finished text for `app`: the silence check,
    /// long-pause trimming, upload (in parts for a take over 10 minutes; a connection that never
    /// opened is tried again, audio is never sent twice), Whisper's silence inventions dropped,
    /// then filler removal and the word list. Blocking network call. Empty text means the device
    /// found no speech; a reply without a transcript is an error, never empty text.
    pub fn transcribe_recording(&self, pcm_path: String, app: String) -> Result<String, TranscribeFailure> {
        let Some(session) = self.session() else {
            diag::log(|| "transcribe: signed out, not sent".into());
            return Err(TranscribeFailure::SignedOut);
        };
        let _reading = claims::reading(Path::new(&pcm_path));
        let pcm_bytes = std::fs::metadata(&pcm_path).map(|m| m.len()).unwrap_or(0);
        let Some(p) = voxa_store::audio::prepare_pcm(Path::new(&pcm_path)) else {
            diag::log(|| format!("transcribe: recording unreadable ({pcm_bytes} bytes)"));
            return Err(TranscribeFailure::Server { status: 0 });
        };
        let parts = p.parts().len();
        let peak = p.peak;
        diag::log(|| format!("transcribe: recording {pcm_bytes} bytes, {:.1} s, peak rms {:.4}, upload {} bytes in {parts} part(s)", p.secs, p.peak, p.upload_bytes()));
        if p.is_silent() {
            diag::log(|| "transcribe: silent take, not sent".into());
            return Ok(String::new());
        }
        let t = std::time::Instant::now();
        let result = self.upload(&session, &p);
        drop(p);
        diag::log(|| match &result {
            Ok(text) => format!("transcribe: ok (HTTP 2xx) in {} ms, text {}", t.elapsed().as_millis(), diag::text(text)),
            Err(e) => format!("transcribe: failed {e:?} in {} ms", t.elapsed().as_millis()),
        });
        let text = result?;
        if take::invented_for_silence(&text, peak) {
            diag::log(|| "transcribe: dropped Whisper's silence phrase".into());
            return Ok(String::new());
        }
        let out = voxa_store::finish_text(text, &self.dir, &app);
        diag::log(|| format!("transcribe: finished text {}", diag::text(&out)));
        Ok(out)
    }

    /// The keyboard's upload of a take ([`FlowEffect::Transcribe`]): [`Self::transcribe_recording`],
    /// then the result is written into the take's History entry (`entry_id`, 0 for an incognito
    /// take) here, so it is kept even if the keyboard went away meanwhile (a keyboard switch
    /// mid-upload). The flow claimed the entry when it asked for this upload; the claim ends
    /// here. `canned`: debug builds only, the text to use instead of calling ChatGPT.
    pub fn transcribe_take(&self, entry_id: u64, pcm_path: String, app: String, canned: Option<String>) -> Result<String, TranscribeFailure> {
        let result = match canned {
            Some(t) => Ok(self.finish_text(t, app)),
            None => self.transcribe_recording(pcm_path, app),
        };
        if entry_id != 0 {
            self.record_result(entry_id, &result);
            claims::release(&self.dir, entry_id);
        }
        result
    }

    /// History's "Send now" (waiting or failed) or "Write it out again". The result stays in
    /// History as "Not pasted"; it is never inserted into whatever field is open now.
    /// Returns None when the entry is gone, has no recording, or is already being sent (by the
    /// background send, or a second tap).
    pub fn resend(&self, entry_id: u64) -> Option<HistoryEntry> {
        let Some(_claim) = self.claim(entry_id) else {
            diag::log(|| format!("resend {entry_id}: already being sent"));
            return None;
        };
        let entry = History::edit(&self.dir, |h| (false, h.get(entry_id).cloned()))?;
        let audio = entry.audio.clone()?.to_string_lossy().to_string();
        let updated = match self.transcribe_recording(audio, entry.app.clone()).map_err(TranscribeError::from) {
            Ok(t) if !t.trim().is_empty() => voxa_store::Entry { text: t, status: Status::Ready, ..entry },
            Ok(_) => voxa_store::Entry { status: Status::Failed(TranscribeError::Server(204).reason().into()), ..entry },
            // A written entry ("Write it out again") keeps its words when the new try fails.
            Err(_) if !entry.status.is_unsent() => entry,
            Err(e) if e.needs_sign_in() => voxa_store::Entry { status: Status::Waiting(notices::WAITING_FOR_SIGN_IN.into()), ..entry },
            // Still offline: it keeps waiting, and the background send picks it up.
            Err(TranscribeError::Offline) => voxa_store::Entry { status: Status::Waiting(notices::WAITING_FOR_NETWORK.into()), ..entry },
            Err(e) => voxa_store::Entry { status: Status::Failed(e.reason().into()), ..entry },
        };
        let u = updated.clone();
        // Deleted while it was being sent: leave it deleted.
        History::edit(&self.dir, move |h| if h.get(entry_id).is_some() { (h.upsert(u), ()) } else { (false, ()) });
        Some(updated.into())
    }

    /// Whether an entry is being sent right now (the keyboard, History's "Send now" or the
    /// background send).
    pub fn is_sending(&self, entry_id: u64) -> bool {
        claims::is_claimed(&self.dir, entry_id)
    }

    /// One saved dictation, without pruning History (a read only; for notification buttons).
    pub fn entry(&self, entry_id: u64) -> Option<HistoryEntry> {
        History::edit(&self.dir, |h| (false, h.get(entry_id).cloned().map(HistoryEntry::from)))
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
        if let Err(e) = s.save(&self.dir) {
            diag::log(|| format!("settings not saved: {:?}", e.kind()));
        }
        let mut w = voxa_store::Words::load(&self.dir);
        if w.spoken_formatting != settings.spoken_formatting {
            w.spoken_formatting = settings.spoken_formatting;
            if let Err(e) = w.save(&self.dir) {
                diag::log(|| format!("words not saved: {:?}", e.kind()));
            }
        }
        History::edit(&self.dir, |h| (h.prune(s.keep_days, now_ms()) > 0, ()));
    }

    pub fn words(&self) -> Words {
        voxa_store::Words::load(&self.dir).into()
    }

    pub fn set_words(&self, words: Words) {
        if let Err(e) = voxa_store::Words::from(words).save(&self.dir) {
            diag::log(|| format!("words not saved: {:?}", e.kind()));
        }
    }

    /// Saved dictations matching `query` (text or app; empty for all), newest first, after dropping
    /// written-out ones older than the keep setting.
    pub fn history(&self, query: String) -> Vec<HistoryEntry> {
        // Settings that can't be read decide nothing: no pruning by a default keep setting.
        let keep = voxa_store::Settings::load_checked(&self.dir).map(|s| s.keep_days);
        History::edit(&self.dir, |h| {
            let pruned = keep.is_some_and(|keep| h.prune(keep, now_ms()) > 0);
            (pruned, h.search(&query).into_iter().cloned().map(HistoryEntry::from).collect())
        })
    }

    /// Bytes of recordings kept for History.
    pub fn history_audio_bytes(&self) -> u64 {
        History::edit(&self.dir, |h| (false, h.audio_bytes()))
    }

    /// Words this week, per day. `utc_offset_minutes`: the phone's current offset from UTC.
    pub fn week_stats(&self, utc_offset_minutes: i32) -> WeekStats {
        History::edit(&self.dir, |h| (false, voxa_store::week(&h.entries, now_ms(), utc_offset_minutes as i64 * 60_000).into()))
    }

    /// Apps in History with their number of dictations, most first ("Your apps").
    pub fn app_counts(&self) -> Vec<AppCount> {
        History::edit(&self.dir, |h| {
            (false, voxa_store::app_counts(&h.entries).into_iter().map(|(app, n)| AppCount { app, count: n as u32 }).collect())
        })
    }

    /// The user edited a written dictation's text in History. Returns the entry as saved, or None
    /// when there is no such written entry.
    pub fn set_entry_text(&self, entry_id: u64, text: String) -> Option<HistoryEntry> {
        History::edit(&self.dir, |h| {
            let changed = h.set_text(entry_id, &text);
            let e = h.get(entry_id).filter(|e| !e.status.is_unsent()).cloned().map(HistoryEntry::from);
            (changed, e)
        })
    }

    pub fn delete_entry(&self, entry_id: u64) {
        History::edit(&self.dir, |h| {
            if let Some(a) = h.get(entry_id).and_then(|e| e.audio.clone()) {
                let _ = std::fs::remove_file(a);
            }
            let removed = h.remove(entry_id);
            if removed && h.save().is_ok() {
                // The copy of the version before would still hold the deleted words.
                h.drop_copies(false);
            }
            (false, ())
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

/// A recording's loudness as `bars` levels 0..1 (History's waveform). Empty if unreadable.
#[uniffi::export]
pub fn recording_levels(pcm_path: String, bars: u32) -> Vec<f32> {
    voxa_store::audio::waveform(Path::new(&pcm_path), bars as usize)
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

#[cfg(test)]
mod session_tests {
    use super::*;

    fn core() -> (Arc<VoxaCore>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("voxa-ffi-session-{}-{}", std::process::id(), now_ms()));
        (VoxaCore::new(dir.to_string_lossy().to_string()), dir)
    }

    #[test]
    fn sign_in_and_out_bump_the_generation_and_drop_waiting_cookies() {
        let (c, dir) = core();
        let g0 = c.session_generation();
        c.use_session("__Secure-next-auth.session-token=A".into(), "ua".into());
        assert!(c.session_generation() > g0);
        assert_eq!(c.take_refreshed_cookies(), None);
        let g1 = c.session_generation();
        c.sign_out();
        assert!(c.session_generation() > g1);
        assert!(!c.has_session());
        assert_eq!(c.take_refreshed_cookies(), None, "nothing to adopt once signed out");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_entry_can_only_be_claimed_once_at_a_time() {
        let (c, dir) = core();
        let first = c.claim(7).expect("free");
        assert!(c.is_sending(7));
        assert!(c.claim(7).is_none(), "a second sender must back off");
        assert!(c.resend(7).is_none(), "Send now while the background send runs does nothing");
        drop(first);
        assert!(!c.is_sending(7));
        assert!(c.claim(7).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
