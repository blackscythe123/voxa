//! One-time import from the old (Electron) Voxa, so updating keeps everything the user had:
//! history with recordings, and their shortcut. Their ChatGPT sign-in is carried over separately
//! (voxa_chatgpt::Session::from_electron_dir).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::store::{Entry, History, Settings, Status};

#[derive(Deserialize, Default)]
#[serde(default)]
struct OldEntry {
    created_at: u64,
    duration_ms: u64,
    source_app_label: Option<String>,
    source_app: Option<String>,
    transcript: Option<String>,
    status: String,
    error_message: Option<String>,
    audio_filename: Option<String>,
    audio_purged_at: Option<u64>,
}

/// Name used for dictations whose app the old Voxa never recorded.
pub const UNKNOWN_APP: &str = "Unknown app";

/// Import the old app's history into `history` (skipping ones already there), linking kept
/// recordings into `new_dir\audio`. Returns how many were added.
pub fn import_history(old_dir: &Path, new_dir: &Path, history: &mut History) -> usize {
    let Ok(bytes) = std::fs::read(old_dir.join("history.json")) else { return 0 };
    let Ok(old) = serde_json::from_slice::<Vec<OldEntry>>(&bytes) else { return 0 };
    let audio_dir = new_dir.join("audio");
    let _ = std::fs::create_dir_all(&audio_dir);
    let mut added = 0;
    for o in old {
        let text = o.transcript.clone().unwrap_or_default();
        let status = match o.status.as_str() {
            "ok" if !text.trim().is_empty() => Status::Pasted,
            "failed" => Status::Failed(o.error_message.clone().filter(|m| !m.is_empty()).map(|_| "Couldn't reach ChatGPT".to_string()).unwrap_or_else(|| "Couldn't reach ChatGPT".into())),
            // "empty": nothing was said; nothing to keep.
            _ => continue,
        };
        if o.created_at == 0 || history.get(o.created_at).is_some() {
            continue;
        }
        let audio = o
            .audio_filename
            .as_ref()
            .filter(|_| o.audio_purged_at.is_none())
            .map(|f| old_dir.join("audio").join(f))
            .filter(|p| p.exists())
            .and_then(|src| link_or_copy(&src, &audio_dir));
        let app = o.source_app_label.or(o.source_app).filter(|a| !a.trim().is_empty()).unwrap_or_else(|| UNKNOWN_APP.into());
        history.upsert(Entry { id: o.created_at, app, text, secs: o.duration_ms as f32 / 1000.0, status, audio, started_in: None, via: None });
        added += 1;
    }
    added
}

/// Hard link (instant, no extra space) or copy, so uninstalling the old app keeps the recording.
fn link_or_copy(src: &Path, dir: &Path) -> Option<PathBuf> {
    let dst = dir.join(src.file_name()?);
    if dst.exists() || std::fs::hard_link(src, &dst).is_ok() || std::fs::copy(src, &dst).is_ok() {
        Some(dst)
    } else {
        None
    }
}

/// The old app's start shortcut ("CommandOrControl+Space" -> "Ctrl+Space"), if it parses.
pub fn old_hotkey(old_dir: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(old_dir.join("preferences.json")).ok()?).ok()?;
    let raw = v.get("startHotkey")?.as_str()?;
    let s = raw.replace("CommandOrControl", "Ctrl").replace("CmdOrCtrl", "Ctrl").replace("Control", "Ctrl").replace("Option", "Alt");
    s.parse::<global_hotkey::hotkey::HotKey>().ok().map(|_| s)
}

/// Everything, once. Marks the settings so it never runs again.
pub fn run_once(old_dir: &Path, new_dir: &Path, history: &mut History, settings: &mut Settings) -> Option<usize> {
    if settings.imported_old_voxa || !old_dir.join("history.json").exists() {
        return None;
    }
    let added = import_history(old_dir, new_dir, history);
    if let Some(k) = old_hotkey(old_dir) {
        settings.hotkey = k;
    }
    // The old app kept history indefinitely. Don't let the default 30 days delete what was
    // just brought over: keep everything until the user chooses otherwise.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let oldest = history.entries.first().map(|e| e.id).unwrap_or(now);
    if settings.keep_days != 0 && now.saturating_sub(oldest) > settings.keep_days as u64 * 86_400_000 {
        settings.keep_days = 0;
    }
    settings.imported_old_voxa = true;
    history.save();
    settings.save(new_dir);
    Some(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_old_history_keeping_text_recordings_and_shortcut() {
        let root = std::env::temp_dir().join(format!("voxa-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (old, new) = (root.join("old"), root.join("new"));
        std::fs::create_dir_all(old.join("audio")).unwrap();
        std::fs::write(old.join("audio").join("a.webm"), b"webm").unwrap();
        std::fs::write(
            old.join("history.json"),
            r#"[
              {"id":"1","created_at":1781467168398,"duration_ms":3268,"source_app":"","transcript":"hello there","status":"ok","audio_filename":"a.webm","audio_purged_at":null},
              {"id":"2","created_at":1781467169000,"duration_ms":2000,"transcript":"","status":"empty"},
              {"id":"3","created_at":1781467170000,"duration_ms":9000,"transcript":null,"status":"failed","error_message":"ASR 500","audio_filename":"gone.webm","audio_purged_at":123}
            ]"#,
        )
        .unwrap();
        std::fs::write(old.join("preferences.json"), r#"{"startHotkey":"CommandOrControl+Space"}"#).unwrap();
        let mut h = History::load(&new);
        let mut s = Settings::default();
        assert_eq!(run_once(&old, &new, &mut h, &mut s), Some(2), "ok + failed; the empty take is skipped");
        let ok = h.get(1781467168398).unwrap();
        assert_eq!((ok.app.as_str(), ok.text.as_str(), ok.status.clone()), (UNKNOWN_APP, "hello there", Status::Pasted));
        assert!(ok.audio.as_ref().unwrap().starts_with(new.join("audio")), "recording now lives with the new app");
        assert!(matches!(h.get(1781467170000).unwrap().status, Status::Failed(_)));
        assert!(h.get(1781467170000).unwrap().audio.is_none(), "purged recording is not linked");
        assert_eq!(s.hotkey, "Ctrl+Space");
        assert_eq!(s.keep_days, 0, "old dictations (2026-06) are older than 30 days: keep everything");
        assert_eq!(run_once(&old, &new, &mut h, &mut s), None, "only once");
    }
}
