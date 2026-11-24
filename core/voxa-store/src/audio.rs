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

pub struct Prepared {
    /// The WAV to upload.
    pub wav: Vec<u8>,
    /// Peak RMS of the take, for [`take::is_silent`] and [`take::invented_for_silence`].
    pub peak: f32,
    pub secs: f32,
}

impl Prepared {
    pub fn is_silent(&self) -> bool {
        take::is_silent(self.peak)
    }
}

/// Measure a recording and build the WAV to upload, with long silences cut from long takes.
pub fn prepare_samples(samples: Vec<i16>) -> Option<Prepared> {
    let secs = samples.len() as f32 / RATE as f32;
    let peak = peak_rms(&samples);
    let kept = if secs >= TRIM_FROM_SECS {
        let t = trim_long(&samples);
        if t.trimmed { t.samples } else { samples }
    } else {
        samples
    };
    Some(Prepared { wav: wav_bytes(&kept)?, peak, secs })
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
        assert_eq!(p.wav.len(), 44 + RATE as usize * 4);
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
        let kept_secs = (p.wav.len() - 44) as f32 / 2.0 / RATE as f32;
        assert!(kept_secs < 25.0, "kept {kept_secs} s");
        assert!(!p.is_silent());
    }

    #[test]
    fn meter_follows_loudness() {
        assert_eq!(meter_level(&[0; 512]), 0.0);
        assert!(meter_level(&[3000, -3000].repeat(256)) > 0.5);
    }
}
