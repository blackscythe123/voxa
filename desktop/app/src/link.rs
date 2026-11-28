//! How the tray process and the window process talk, through small files in the data folder.
//!
//! The tray process (hotkey, strip, mic, transcription) is the only writer of history.json. It
//! publishes its live state to live.json. The window process (`--window`) reads history,
//! settings and live state, and asks the tray process to act by dropping command files in cmd\.
//! The window runs as its own process so its memory goes back to Windows when it closes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use voxa_core::{Mic, Network, Readiness, Session};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Live {
    /// "valid", "expired" or "unknown".
    pub session: String,
    pub online: bool,
    /// "granted", "denied" or "missing".
    pub mic: String,
    pub mic_name: String,
    pub signing_in: bool,
    /// History entries the tray process is still working on (not yet pasted or given up).
    pub live_ids: Vec<u64>,
    /// Answer to the last shortcut change: (shortcut asked for, error if refused).
    pub hotkey_result: Option<(String, Option<String>)>,
    /// What the strip shows: idle, recording, paused, writing, pasted, needs, blocked.
    pub state: String,
    /// The updater's state for Settings and the Home bars (crate::update).
    pub update: UpdateView,
}

/// What the window shows about updates. Written by the tray process (the only one that updates).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateView {
    /// The status line under "Voxa 1.0.0" in Settings (copy deck, docs/updates.md).
    pub status: String,
    pub checking: bool,
    /// The last check was asked for by the user (only then is a refusal or failure announced).
    pub manual: bool,
    /// Auto off and an update is ready: Settings offers "Restart and update".
    pub can_restart: bool,
    /// Auto install gave up (2 failed attempts): Settings offers "Download".
    pub gave_up: bool,
    /// The newest version on offer, if any.
    pub version: Option<String>,
    /// The versioned installer or dmg, for "Download".
    pub download_url: Option<String>,
    /// The release page, for "What's new".
    pub notes_url: Option<String>,
    /// Windows: "Voxa was updated to X" on Home until dismissed.
    pub updated_to: Option<String>,
    /// macOS: "Voxa X is out" on Home until dismissed.
    pub mac_available: Option<String>,
    /// macOS: the Accessibility grant must be given again (the app's signature changed).
    pub mac_accessibility_again: bool,
}

impl Live {
    pub fn from_readiness(r: Readiness) -> (String, bool, String) {
        let session = match r.session {
            Session::Valid => "valid",
            Session::Expired => "expired",
            Session::Unknown => "unknown",
        };
        let mic = match r.mic {
            Mic::Granted => "granted",
            Mic::Denied => "denied",
            Mic::Missing => "missing",
            Mic::Stopped => "stopped",
            Mic::CantSave => "cantsave",
        };
        (session.into(), r.network == Network::Up, mic.into())
    }

    pub fn readiness(&self) -> Readiness {
        Readiness {
            session: match self.session.as_str() {
                "valid" => Session::Valid,
                "expired" => Session::Expired,
                _ => Session::Unknown,
            },
            network: if self.online || self.session.is_empty() { Network::Up } else { Network::Down },
            mic: match self.mic.as_str() {
                "denied" => Mic::Denied,
                "missing" => Mic::Missing,
                "stopped" => Mic::Stopped,
                "cantsave" => Mic::CantSave,
                _ => Mic::Granted,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    SignIn,
    SignOut,
    SetHotkey(String),
    PauseHotkey(bool),
    /// Settings changed on disk: reload them (keep-days pruning, filler removal).
    ReloadSettings,
    Resend(u64),
    ResendAll,
    DeleteEntry(u64),
    /// The user corrected a dictation's text.
    EditEntry(u64, String),
    DeleteAll,
    /// Delete recordings older than this many days, keeping their text.
    DeleteOldAudio(u32),
    /// Settings, Help: "Check for updates".
    CheckForUpdates,
    /// Auto off and an update is ready: the user pressed "Restart and update".
    InstallUpdateNow,
    /// "What's new": the release page of the update (or of this version).
    OpenReleaseNotes,
    /// The Home bar after an update (or the macOS "is out" bar) was closed.
    DismissUpdateNote,
}

fn dir() -> PathBuf {
    crate::data_dir()
}

pub fn live_path() -> PathBuf {
    dir().join("live.json")
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    voxa_core::fsx::replace_file(tmp.as_ref(), path.as_ref())
}

pub fn write_live(live: &Live) {
    if let Ok(b) = serde_json::to_vec(live) {
        let _ = write_atomic(&live_path(), &b);
    }
}

pub fn read_live() -> Live {
    std::fs::read(live_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Window process: ask the tray process to do something.
pub fn send(cmd: &Command) {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    if let Ok(b) = serde_json::to_vec(cmd) {
        let _ = write_atomic(&dir().join("cmd").join(format!("{n}.json")), &b);
        crate::os::commands_signal();
    }
}

/// Ticks (of 40 ms) between reads of the commands folder when no signal came: a missed signal
/// costs at most this. Windows signals every command; macOS has no signal yet.
const FALLBACK_TICKS: u32 = if cfg!(windows) { 25 } else { 6 };

/// Realworld 19: the commands folder is read when the window process signalled a command, not
/// 25 times a second (a network-redirected profile made that block the hotkey).
pub fn due(signalled: bool, tick: u32) -> bool {
    signalled || tick % FALLBACK_TICKS == 0
}

/// Tray process: whether to read the commands folder on this tick.
pub fn commands_due(tick: u32) -> bool {
    due(crate::os::commands_signalled(), tick)
}

/// Tray process: commands waiting, oldest first. Each is removed once read.
pub fn take_commands() -> Vec<Command> {
    let Ok(rd) = std::fs::read_dir(dir().join("cmd")) else { return Vec::new() };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let cmd = std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok());
            let _ = std::fs::remove_file(&p);
            cmd
        })
        .collect()
}

/// Last-modified time of a file, to notice changes cheaply.
pub fn stamp(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_round_trips_through_live() {
        for r in [
            Readiness { session: Session::Valid, network: Network::Up, mic: Mic::Granted },
            Readiness { session: Session::Expired, network: Network::Down, mic: Mic::Denied },
            Readiness { session: Session::Unknown, network: Network::Up, mic: Mic::Missing },
        ] {
            let (session, online, mic) = Live::from_readiness(r);
            assert_eq!(Live { session, online, mic, ..Default::default() }.readiness(), r);
        }
    }

    #[test]
    fn the_commands_folder_is_not_read_every_tick() {
        let reads = (1..=250u32).filter(|t| due(false, *t)).count();
        assert!(reads <= 42, "{reads} folder reads in 10 s with nothing to do");
        assert!(due(true, 1), "a signalled command is read at once");
    }

    #[test]
    fn check_for_updates_command_survives_the_file_trip() {
        for c in [Command::CheckForUpdates, Command::InstallUpdateNow, Command::OpenReleaseNotes, Command::DismissUpdateNote] {
            let b = serde_json::to_vec(&c).unwrap();
            assert_eq!(serde_json::from_slice::<Command>(&b).unwrap(), c);
        }
    }

    #[test]
    fn old_live_json_without_update_still_reads() {
        let old = br#"{"session":"valid","online":true,"mic":"granted","mic_name":"Mic","signing_in":false,"live_ids":[3],"hotkey_result":null,"state":"idle"}"#;
        let live: Live = serde_json::from_slice(old).unwrap();
        assert_eq!(live.session, "valid");
        assert_eq!(live.live_ids, vec![3]);
        assert_eq!(live.update, UpdateView::default());
        // And a newer view round trips.
        let mut l = live.clone();
        l.update.updated_to = Some("1.0.1".into());
        l.update.status = "You have the latest version.".into();
        let b = serde_json::to_vec(&l).unwrap();
        assert_eq!(serde_json::from_slice::<Live>(&b).unwrap(), l);
    }

    #[test]
    fn commands_survive_the_file_trip() {
        let c = Command::SetHotkey("Ctrl+Shift+K".into());
        let b = serde_json::to_vec(&c).unwrap();
        assert_eq!(serde_json::from_slice::<Command>(&b).unwrap(), c);
    }
}
