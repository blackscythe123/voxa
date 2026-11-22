//! The window process's view of Voxa (`voxa-desktop --window`). Same calls the window used to
//! make on the tray app, now answered from the data folder and forwarded as commands.

use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use gpui::Context;
use voxa_core::Readiness;

use crate::link::{self, Command, Live};
use crate::store::{History, Settings, Status};
use crate::{audio, data_dir, win};

pub(crate) struct Backend {
    pub(crate) history: History,
    pub(crate) settings: Settings,
    pub(crate) start_with_windows: bool,
    pub(crate) mic_name: String,
    pub(crate) levels: VecDeque<f32>,
    pub(crate) signing_in: bool,
    live: Live,
    stamps: [Option<SystemTime>; 3],
    /// The setup's mic check: this process opens the mic itself, only while that step shows.
    mic: Option<audio::Mic>,
}

impl Backend {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let live = link::read_live();
        let mut me = Self {
            history: History::load(&data_dir()),
            settings: Settings::load(&data_dir()),
            start_with_windows: win::starts_with_windows(),
            mic_name: live.mic_name.clone(),
            levels: VecDeque::new(),
            signing_in: live.signing_in,
            live,
            stamps: [None; 3],
            mic: None,
        };
        me.stamps = me.current_stamps();
        // Follow the tray process: history, settings and live state, checked 4 times a second.
        // The mic check needs 25 updates a second, only while it runs.
        cx.spawn(async move |this, cx| {
            let mut tick = 0u32;
            loop {
                cx.background_executor().timer(Duration::from_millis(40)).await;
                tick += 1;
                let alive = this
                    .update(cx, |b, cx| {
                        if b.mic.is_some() {
                            let lvl = audio::level().max(b.levels.back().copied().unwrap_or(0.0) * 0.88);
                            b.levels.push_back(lvl);
                            while b.levels.len() > crate::QUILL_CYCLE {
                                b.levels.pop_front();
                            }
                            cx.notify();
                        }
                        if tick % 6 == 0 && b.reload_if_changed() {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        me
    }

    fn current_stamps(&self) -> [Option<SystemTime>; 3] {
        let d = data_dir();
        [link::stamp(&d.join("history.json")), link::stamp(&d.join("settings.json")), link::stamp(&link::live_path())]
    }

    fn reload_if_changed(&mut self) -> bool {
        let now = self.current_stamps();
        if now == self.stamps {
            return false;
        }
        if now[0] != self.stamps[0] {
            self.history = History::load(&data_dir());
        }
        if now[1] != self.stamps[1] {
            self.settings = Settings::load(&data_dir());
        }
        if now[2] != self.stamps[2] {
            self.live = link::read_live();
            self.signing_in = self.live.signing_in;
            if !self.live.mic_name.is_empty() {
                self.mic_name = self.live.mic_name.clone();
            }
        }
        self.stamps = now;
        true
    }

    /// Who is signed in (saved by the tray app at each sign-in check).
    pub fn account(&self) -> Option<voxa_chatgpt::Account> {
        std::fs::read(data_dir().join("account.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn readiness(&self) -> Readiness {
        self.live.readiness()
    }

    pub fn is_live(&self, id: u64) -> bool {
        self.live.live_ids.contains(&id)
    }

    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        link::send(&Command::SignIn);
        self.signing_in = true;
        cx.notify();
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        link::send(&Command::SignOut);
        cx.notify();
    }

    pub fn open_mic_settings(&mut self, _: &mut Context<Self>) {
        let _ = std::process::Command::new("explorer").arg("ms-settings:privacy-microphone").spawn();
    }

    pub fn pause_hotkey(&mut self, pause: bool) {
        link::send(&Command::PauseHotkey(pause));
    }

    /// Ask the tray process to register the shortcut and wait briefly for its answer.
    pub fn set_hotkey(&mut self, s: &str, cx: &mut Context<Self>) -> Result<(), String> {
        s.parse::<global_hotkey::hotkey::HotKey>().map_err(|_| format!("{s} can't be used as a shortcut"))?;
        link::send(&Command::SetHotkey(s.to_string()));
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(25));
            let live = link::read_live();
            if let Some((asked, err)) = live.hotkey_result.clone() {
                if asked == s {
                    self.live = live;
                    self.settings = Settings::load(&data_dir());
                    cx.notify();
                    return match err {
                        Some(e) => Err(e),
                        None => Ok(()),
                    };
                }
            }
        }
        Err("Voxa didn't answer. Is it running in the tray?".into())
    }

    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let mut s = Settings::load(&data_dir());
        f(&mut s);
        s.save(&data_dir());
        self.settings = s;
        link::send(&Command::ReloadSettings);
        cx.notify();
    }

    pub fn delete_history(&mut self, cx: &mut Context<Self>) {
        link::send(&Command::DeleteAll);
        self.history.entries.clear();
        cx.notify();
    }

    pub fn delete_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        link::send(&Command::DeleteEntry(id));
        self.history.entries.retain(|e| e.id != id);
        cx.notify();
    }

    pub fn set_autostart(&mut self, on: bool, cx: &mut Context<Self>) {
        // The tray app's exe, which is this same exe.
        if let Err(e) = win::set_start_with_windows(on) {
            win::log(&format!("FAIL start with Windows: {e}"));
        }
        self.start_with_windows = win::starts_with_windows();
        cx.notify();
    }

    pub fn resend(&mut self, id: u64, cx: &mut Context<Self>) {
        link::send(&Command::Resend(id));
        if let Some(e) = self.history.entries.iter_mut().find(|e| e.id == id) {
            e.status = Status::Waiting("Writing it out".into());
        }
        cx.notify();
    }

    pub fn resend_all(&mut self, cx: &mut Context<Self>) {
        link::send(&Command::ResendAll);
        cx.notify();
    }

    pub fn play_entry(&self, id: u64) -> Option<f32> {
        let path = self.history.get(id)?.audio.clone().filter(|a| a.exists())?;
        // Recordings from the old Voxa are .webm: open them in the default player.
        if path.extension().is_some_and(|e| e != "wav") {
            let _ = std::process::Command::new("explorer").arg(&path).spawn();
            return None;
        }
        let secs = hound::WavReader::open(&path).ok().map(|r| r.duration() as f32 / r.spec().sample_rate as f32)?;
        win::play_audio(&path).then_some(secs)
    }

    pub fn show_in_folder(&self, id: u64) {
        if let Some(a) = self.history.get(id).and_then(|e| e.audio.clone()) {
            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", a.display())).spawn();
        }
    }

    pub fn mic_test(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.mic.is_some() {
            return;
        }
        if on {
            let path = std::env::temp_dir().join("voxa-mic-test.wav");
            match audio::Mic::new().and_then(|mut m| m.start(&path).map(|_| m)) {
                Ok(m) => {
                    self.mic_name = m.device_name().to_string();
                    self.mic = Some(m);
                    self.levels.clear();
                }
                Err(e) => win::log(&format!("mic test failed: {e:?}")),
            }
        } else if let Some(mut m) = self.mic.take() {
            m.discard();
            self.levels.clear();
            let _ = std::fs::remove_file(std::env::temp_dir().join("voxa-mic-test.wav"));
        }
        cx.notify();
    }
}
