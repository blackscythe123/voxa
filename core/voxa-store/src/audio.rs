//! A recording, from raw samples to the bytes uploaded: measuring it (peak for the silence rule,
//! meter level for the live line) and cutting long silences (the desktop's `prepare.rs`).
//!
//! Android records 16 kHz mono PCM16 (little-endian) straight to a raw `.pcm` file, so a take cut
//! short by a process death needs no header repair: the file is valid up to its last whole sample.

use std::path::Path;

use crate::take;

pub const RATE: u32 = 16_000;
/// Takes shorter than this are sent as recorded.
pub const TRIM_FROM_SECS: f32 = 30.0;

pub fn samples_from_pcm(bytes: &[u8]) -> Vec<i16> {
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

fn rms(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f64 = frame.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

/// The loudest 30 ms frame's RMS: the "peak rms" the silence rules use.
pub fn peak_rms(samples: &[i16]) -> f32 {
    let flen = (RATE as usize * 30) / 1000;
    samples.chunks(flen).map(rms).fold(0.0, f32::max)
}

/// Meter level 0..1 on the desktop's decibel scale: -52 dB (room) to -24 dB (speech up close).
pub fn meter_level(samples: &[i16]) -> f32 {
    let db = 20.0 * rms(samples).max(1e-6).log10();
    ((db + 52.0) / 28.0).clamp(0.0, 1.0)
}

/// A recording's loudness as `bars` meter levels (0..1), for History's waveform. Each bar is
/// the loudest 30 ms frame in its slice of the take. Empty when the file can't be read.
pub fn waveform(path: &Path, bars: usize) -> Vec<f32> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let samples = samples_from_pcm(&bytes);
    if samples.is_empty() || bars == 0 {
        return Vec::new();
    }
    let per = samples.len().div_ceil(bars).max(1);
    let flen = (RATE as usize * 30) / 1000;
    samples
        .chunks(per)
        .map(|slice| slice.chunks(flen).map(meter_level).fold(0.0, f32::max))
        .collect()
}

pub fn wav_bytes(samples: &[i16]) -> Option<Vec<u8>> {
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut buf, spec).ok()?;
    for s in samples {
        w.write_sample(*s).ok()?;
    }
    w.finalize().ok()?;
    Some(buf.into_inner())
}

#[cfg(feature = "vad")]
fn trim_long(samples: &[i16]) -> voxa_core::trim::Trimmed {
    match voxa_vad::trim_speech(samples) {
        Ok(t) => t,
        Err(_) => voxa_core::trim::trim(samples, RATE, &voxa_core::trim::TrimParams::default()),
    }
}

/// Without Silero (the Android build for now), long takes use voxa-core's loudness detector.
#[cfg(not(feature = "vad"))]
fn trim_long(samples: &[i16]) -> voxa_core::trim::Trimmed {
    voxa_core::trim::trim(samples, RATE, &voxa_core::trim::TrimParams::default())
}

/// The most speech one upload carries: 10 minutes (19.2 MB of WAV), safely under ChatGPT's
/// ~25 MB limit for one file. Longer takes are sent in parts and the text joined.
pub const MAX_PART_SECS: usize = 600;

pub struct Prepared {
    /// What is uploaded: the take with long silences cut. Turned into WAV one part at a time
    /// (see [`Prepared::parts`]), so a long take is never held as several full copies at once.
    pub samples: Vec<i16>,
    /// Peak RMS of the take, for [`take::is_silent`] and [`take::invented_for_silence`].
    pub peak: f32,
    pub secs: f32,
}

impl Prepared {
    pub fn is_silent(&self) -> bool {
        take::is_silent(self.peak)
    }

    /// The pieces sent one request each: one for takes up to [`MAX_PART_SECS`].
    pub fn parts(&self) -> Vec<std::ops::Range<usize>> {
        upload_parts(&self.samples, MAX_PART_SECS * RATE as usize)
    }

    /// The WAV for one part.
    pub fn wav(&self, part: std::ops::Range<usize>) -> Option<Vec<u8>> {
        wav_bytes(self.samples.get(part)?)
    }

    /// Bytes of WAV all parts add up to.
    pub fn upload_bytes(&self) -> usize {
        self.parts().len() * 44 + self.samples.len() * 2
    }
}

/// Cut `samples` into parts of at most `max` samples. Each cut is made at the quietest half
/// second in the last 30 s before the limit, so a word is rarely split.
pub fn upload_parts(samples: &[i16], max: usize) -> Vec<std::ops::Range<usize>> {
    let max = max.max(RATE as usize);
    let window = RATE as usize / 2;
    let look = (RATE as usize * 30).min(max / 2);
    let mut parts = Vec::new();
    let mut start = 0;
    while samples.len() - start > max {
        let limit = start + max;
        let from = limit - look;
        let mut best = (f32::MAX, limit);
        let mut at = from;
        while at + window <= limit {
            let level = rms(&samples[at..at + window]);
            if level < best.0 {
                best = (level, at + window / 2);
            }
            at += window / 2;
        }
        parts.push(start..best.1);
        start = best.1;
    }
    parts.push(start..samples.len());
    parts
}

/// Measure a recording and keep what is uploaded, with long silences cut from long takes.
pub fn prepare_samples(samples: Vec<i16>) -> Option<Prepared> {
    let secs = samples.len() as f32 / RATE as f32;
    let peak = peak_rms(&samples);
    let kept = if secs >= TRIM_FROM_SECS {
        let t = trim_long(&samples);
        if t.trimmed { t.samples } else { samples }
    } else {
        samples
    };
    Some(Prepared { samples: kept, peak, secs })
}

/// [`prepare_samples`] for a raw PCM file.
pub fn prepare_pcm(pcm_path: &Path) -> Option<Prepared> {
    prepare_samples(samples_from_pcm(&std::fs::read(pcm_path).ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_takes_are_sent_whole_and_silence_is_seen() {
        let p = prepare_samples(vec![0i16; RATE as usize * 2]).unwrap();
        assert!(p.is_silent());
        assert_eq!(p.parts().len(), 1);
        assert_eq!(p.wav(p.parts()[0].clone()).unwrap().len(), 44 + RATE as usize * 4);
        assert!((p.secs - 2.0).abs() < 0.01);
    }

    #[test]
    fn long_takes_lose_their_long_pauses() {
        // 10 s of tone, 40 s of silence, 10 s of tone.
        let tone = |n: usize| (0..n).map(|i| if i % 16 < 8 { 4000i16 } else { -4000 }).collect::<Vec<_>>();
        let mut s = tone(RATE as usize * 10);
        s.extend(std::iter::repeat_n(0i16, RATE as usize * 40));
        s.extend(tone(RATE as usize * 10));
        let p = prepare_samples(s).unwrap();
        let kept_secs = p.samples.len() as f32 / RATE as f32;
        assert!(kept_secs < 25.0, "kept {kept_secs} s");
        assert!(!p.is_silent());
    }

    /// ChatGPT takes about 25 MB per file: a long take is sent in parts under that, cut in a
    /// pause, and every sample is sent exactly once.
    #[test]
    fn a_long_take_is_sent_in_parts_under_the_upload_limit() {
        let tone = |n: usize| (0..n).map(|i| if i % 16 < 8 { 4000i16 } else { -4000 }).collect::<Vec<_>>();
        // 25 minutes of talk with a 1 s pause every minute.
        let mut s = Vec::new();
        for _ in 0..25 {
            s.extend(tone(RATE as usize * 59));
            s.extend(std::iter::repeat_n(0i16, RATE as usize));
        }
        let max = MAX_PART_SECS * RATE as usize;
        let parts = upload_parts(&s, max);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.first().unwrap().start, 0);
        assert_eq!(parts.last().unwrap().end, s.len());
        for w in parts.windows(2) {
            assert_eq!(w[0].end, w[1].start, "no gap and no overlap");
        }
        for p in &parts {
            assert!(p.len() <= max);
            assert!(44 + p.len() * 2 < 25_000_000, "each part is under ChatGPT's limit");
        }
        // The cut lands in a pause, not mid-word.
        assert_eq!(s[parts[0].end], 0);
        assert_eq!(upload_parts(&s[..RATE as usize * 60], max), vec![0..RATE as usize * 60]);
    }

    #[test]
    fn meter_follows_loudness() {
        assert_eq!(meter_level(&[0; 512]), 0.0);
        assert!(meter_level(&[3000, -3000].repeat(256)) > 0.5);
    }

    #[test]
    fn waveform_has_one_level_per_bar_loud_where_the_voice_is() {
        let dir = std::env::temp_dir().join(format!("voxa-wave-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("take.pcm");
        // 1 s of silence, then 1 s of tone.
        let mut s = vec![0i16; RATE as usize];
        s.extend((0..RATE as usize).map(|i| if i % 16 < 8 { 4000i16 } else { -4000 }));
        let bytes: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
        std::fs::write(&path, bytes).unwrap();
        let w = waveform(&path, 8);
        assert_eq!(w.len(), 8);
        assert_eq!(w[0], 0.0);
        assert!(w[7] > 0.5);
        assert!(waveform(&dir.join("missing.pcm"), 8).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
