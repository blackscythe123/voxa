//! A take is in History from its first moment (status "Recording", then "Writing it out"), so a
//! forced end (installer update, shutdown, power cut, crash, Task Manager) never loses it. At
//! the next start those takes become "Not sent", ready for Send now, and their recording files
//! get a correct WAV header (the header's lengths are only written when a take finishes).
//! Recordings with no History entry at all (from builds before this) are brought back the same way.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::store::{Entry, History, Status, NOT_SENT, RECORDING, WRITING};

/// Bytes per second of Voxa's recordings (16 kHz, mono, 16-bit).
const BYTES_PER_SEC: f32 = 32_000.0;
/// App name for a recording found with no History entry.
pub const RECOVERED_APP: &str = "Recovered recording";

/// Fix the RIFF and data lengths of a WAV whose writer never finished. Returns the bytes of
/// audio in it, or None when it isn't a WAV with a data chunk.
pub fn repair_wav(path: &Path) -> Option<u64> {
    let mut f = std::fs::OpenOptions::new().read(true).write(true).open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head).ok()?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return None;
    }
    // Walk the chunks to the data chunk.
    let mut at = 12u64;
    loop {
        let mut ch = [0u8; 8];
        f.seek(SeekFrom::Start(at)).ok()?;
        f.read_exact(&mut ch).ok()?;
        let size = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) as u64;
        if &ch[0..4] == b"data" {
            let data = len.saturating_sub(at + 8);
            // Whole 16-bit samples only.
            let data = data - data % 2;
            if size as u64 != data {
                f.seek(SeekFrom::Start(at + 4)).ok()?;
                f.write_all(&(data.min(u32::MAX as u64) as u32).to_le_bytes()).ok()?;
                f.seek(SeekFrom::Start(4)).ok()?;
                f.write_all(&((at + 8 + data - 8).min(u32::MAX as u64) as u32).to_le_bytes()).ok()?;
                let _ = f.set_len(at + 8 + data);
            }
            return Some(data);
        }
        at += 8 + size + size % 2;
        if at >= len {
            return None;
        }
    }
}

/// History ids are recording start times in ms; recordings are named `<ms>-<job>.wav`.
fn started_ms(path: &Path) -> Option<u64> {
    path.file_stem()?.to_str()?.split('-').next()?.parse().ok()
}

/// Turn takes that were still recording or being written out when Voxa last stopped into "Not
/// sent", and bring back recordings with no entry. Only call when History was read cleanly.
/// Returns how many takes were recovered.
pub fn recover(history: &mut History, audio_dir: &Path) -> usize {
    let mut n = 0;
    let mut gone = Vec::new();
    for e in history.entries.iter_mut() {
        let Status::Waiting(why) = &e.status else { continue };
        if why != RECORDING && why != WRITING {
            continue;
        }
        let audio = e.audio.as_deref().and_then(repair_wav);
        if !e.text.trim().is_empty() {
            // Being written out again: it still has its text.
            e.status = Status::Ready;
        } else if audio.is_some_and(|b| b > 0) {
            if e.secs <= 0.0 {
                e.secs = audio.unwrap_or(0) as f32 / BYTES_PER_SEC;
            }
            e.status = Status::Waiting(NOT_SENT.into());
        } else {
            // Nothing was recorded.
            gone.push(e.id);
            continue;
        }
        n += 1;
    }
    for id in gone {
        if let Some(a) = history.get(id).and_then(|e| e.audio.clone()) {
            let _ = std::fs::remove_file(a);
        }
        history.remove(id);
    }
    let known: std::collections::HashSet<PathBuf> = history.entries.iter().filter_map(|e| e.audio.clone()).collect();
    let Ok(rd) = std::fs::read_dir(audio_dir) else { return n };
    let mut orphans: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "wav") && !known.contains(p)).collect();
    orphans.sort();
    for path in orphans {
        let Some(id) = started_ms(&path) else { continue };
        match repair_wav(&path) {
            Some(bytes) if bytes > 0 => {
                if history.get(id).is_some() {
                    continue;
                }
                history.upsert(Entry { id, app: RECOVERED_APP.into(), text: String::new(), secs: bytes as f32 / BYTES_PER_SEC, status: Status::Waiting(NOT_SENT.into()), audio: Some(path), started_in: None, via: None });
                n += 1;
            }
            // A header with no sound after it: nothing to keep.
            Some(_) => {
                let _ = std::fs::remove_file(&path);
            }
            None => {}
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voxa-recover-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A WAV as hound leaves it when the process is killed mid-take: lengths still 0.
    fn unfinished_wav(path: &Path, samples: usize) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..samples {
            w.write_sample((i % 100) as i16).unwrap();
        }
        w.finalize().unwrap();
        let mut b = std::fs::read(path).unwrap();
        // The header lengths as they are when the writer never finished.
        let data_at = b.windows(4).position(|w| w == b"data").unwrap();
        b[data_at + 4..data_at + 8].copy_from_slice(&0u32.to_le_bytes());
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn a_take_cut_off_by_a_forced_kill_is_kept_as_not_sent_and_plays() {
        let d = tmp("kill");
        let wav = d.join("1795000000000-3.wav");
        unfinished_wav(&wav, 32_000);
        let mut h = History::load(&d);
        h.upsert(Entry { id: 1795000000000, app: "Slack".into(), text: String::new(), secs: 0.0, status: Status::Waiting(RECORDING.into()), audio: Some(wav.clone()), started_in: None, via: None });
        assert_eq!(recover(&mut h, &d), 1);
        let e = h.get(1795000000000).unwrap();
        assert_eq!(e.status, Status::Waiting(NOT_SENT.into()));
        assert!((e.secs - 2.0).abs() < 0.01, "length from the file: {}", e.secs);
        let r = hound::WavReader::open(&wav).expect("the repaired file opens");
        assert_eq!(r.duration(), 32_000, "every sample is readable");
    }

    #[test]
    fn a_take_killed_while_being_written_out_again_keeps_its_text() {
        let d = tmp("rewrite");
        let mut h = History::load(&d);
        h.upsert(Entry { id: 5, app: "A".into(), text: "kept words".into(), secs: 1.0, status: Status::Waiting(WRITING.into()), audio: None, started_in: None, via: None });
        recover(&mut h, &d);
        assert_eq!(h.get(5).unwrap().status, Status::Ready);
    }

    #[test]
    fn a_take_with_nothing_recorded_is_dropped_with_its_empty_file() {
        let d = tmp("empty");
        let wav = d.join("1795000000001-4.wav");
        unfinished_wav(&wav, 0);
        let mut h = History::load(&d);
        h.upsert(Entry { id: 1795000000001, app: "A".into(), text: String::new(), secs: 0.0, status: Status::Waiting(RECORDING.into()), audio: Some(wav.clone()), started_in: None, via: None });
        assert_eq!(recover(&mut h, &d), 0);
        assert!(h.entries.is_empty());
        assert!(!wav.exists());
    }

    #[test]
    fn a_recording_with_no_history_entry_is_brought_back() {
        let d = tmp("orphan");
        let orphan = d.join("1795353726091-2.wav");
        unfinished_wav(&orphan, 16_000);
        let known = d.join("1795353726200-3.wav");
        unfinished_wav(&known, 16_000);
        let mut h = History::load(&d);
        h.upsert(Entry { id: 1795353726200, app: "A".into(), text: "t".into(), secs: 1.0, status: Status::Pasted, audio: Some(known), started_in: None, via: None });
        assert_eq!(recover(&mut h, &d), 1);
        let e = h.get(1795353726091).expect("the orphan is in History");
        assert_eq!((e.app.as_str(), &e.status), (RECOVERED_APP, &Status::Waiting(NOT_SENT.into())));
        assert_eq!(recover(&mut h, &d), 0, "only once");
    }

    #[test]
    fn finished_takes_are_left_alone() {
        let d = tmp("finished");
        let mut h = History::load(&d);
        h.upsert(Entry { id: 9, app: "A".into(), text: String::new(), secs: 1.0, status: Status::Waiting("Sends when you're back online".into()), audio: None, started_in: None, via: None });
        assert_eq!(recover(&mut h, &d), 0);
        assert_eq!(h.get(9).unwrap().status, Status::Waiting("Sends when you're back online".into()));
    }
}
