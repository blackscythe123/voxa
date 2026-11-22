//! Smart silence trimming before upload (docs/feature-smart-silence-trimming.md).
//!
//! Long dictations are mostly dead air; sending them raw can fail (a 48-minute file with ~2 minutes
//! of speech got HTTP 500) and makes Whisper hallucinate. This cuts pauses longer than
//! `min_silence_ms`, keeps `pad_ms` around speech so soft consonants survive, joins phrases with a
//! short spacer, and fades each cut so there are no clicks.
//!
//! Detection is pluggable ([`trim_with`]). The desktop uses Silero (crate `voxa-vad`): on a real
//! 48-minute recording the built-in loudness detector ([`trim`]) kept 99.7%, because the room's
//! background was as loud as soft speech. [`trim`] is only a fallback when Silero can't load.

#[derive(Clone, Copy, Debug)]
pub struct TrimParams {
    pub frame_ms: u32,
    /// Kept before and after speech.
    pub pad_ms: u32,
    /// Pauses shorter than this are natural rhythm and are kept.
    pub min_silence_ms: u32,
    /// Quiet gap inserted between joined phrases.
    pub spacer_ms: u32,
    /// Fade at each cut edge.
    pub fade_ms: u32,
    /// If less speech than this is found, send the original instead.
    pub min_keep_ms: u32,
    /// A frame is speech when this many dB above the room's noise floor...
    pub above_floor_db: f32,
    /// ...and never below this absolute level.
    pub min_db: f32,
}

impl Default for TrimParams {
    fn default() -> Self {
        Self { frame_ms: 30, pad_ms: 200, min_silence_ms: 300, spacer_ms: 100, fade_ms: 5, min_keep_ms: 1000, above_floor_db: 9.0, min_db: -52.0 }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Trimmed {
    pub samples: Vec<i16>,
    /// Speech spans kept, in sample offsets of the input.
    pub spans: Vec<(usize, usize)>,
    /// False when trimming was skipped (too little speech found): `samples` is the original.
    pub trimmed: bool,
}

fn frame_db(frame: &[i16]) -> f32 {
    let sum: f64 = frame.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    let rms = (sum / frame.len().max(1) as f64).sqrt();
    20.0 * (rms.max(1e-9) as f32).log10()
}

/// Speech spans using the built-in loudness detector.
pub fn speech_spans(samples: &[i16], rate: u32, p: &TrimParams) -> Vec<(usize, usize)> {
    let flen = (rate * p.frame_ms / 1000) as usize;
    if flen == 0 || samples.len() < flen {
        return Vec::new();
    }
    let dbs: Vec<f32> = samples.chunks(flen).map(frame_db).collect();
    // Noise floor: the quietest 10% of frames is the room. In a recording that is nearly all
    // speech those frames are speech too, so the estimate is capped at a real room's level.
    let mut sorted = dbs.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[(sorted.len() as f32 * 0.10) as usize].min(-50.0);
    let threshold = (floor + p.above_floor_db).max(p.min_db);

    let speech: Vec<bool> = dbs.iter().map(|db| *db >= threshold).collect();
    spans_from_frames(&speech, flen, samples.len(), rate, p)
}

/// Speech spans from a per-frame speech decision, padded, with short pauses merged.
pub fn spans_from_frames(speech: &[bool], flen: usize, total: usize, rate: u32, p: &TrimParams) -> Vec<(usize, usize)> {
    let ms = |m: u32| (rate as usize * m as usize) / 1000;
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (i, is_speech) in speech.iter().enumerate() {
        if !is_speech {
            continue;
        }
        let start = (i * flen).saturating_sub(ms(p.pad_ms));
        let end = ((i + 1) * flen + ms(p.pad_ms)).min(total);
        match spans.last_mut() {
            // Merge when the gap is a natural pause.
            Some(last) if start <= last.1 + ms(p.min_silence_ms) => last.1 = last.1.max(end),
            _ => spans.push((start, end)),
        }
    }
    spans
}

/// Trim silence with the built-in loudness detector.
pub fn trim(samples: &[i16], rate: u32, p: &TrimParams) -> Trimmed {
    let spans = speech_spans(samples, rate, p);
    cut(samples, rate, p, spans)
}

/// Trim silence with a caller-supplied voice detector, called once per `frame_ms` frame.
pub fn trim_with(samples: &[i16], rate: u32, p: &TrimParams, mut is_voice: impl FnMut(&[i16]) -> bool) -> Trimmed {
    let flen = (rate * p.frame_ms / 1000) as usize;
    let speech: Vec<bool> = samples.chunks(flen).map(|f| is_voice(f)).collect();
    let spans = spans_from_frames(&speech, flen, samples.len(), rate, p);
    cut(samples, rate, p, spans)
}

fn cut(samples: &[i16], rate: u32, p: &TrimParams, spans: Vec<(usize, usize)>) -> Trimmed {
    let kept: usize = spans.iter().map(|(a, b)| b - a).sum();
    if kept < (rate as usize * p.min_keep_ms as usize) / 1000 {
        return Trimmed { samples: samples.to_vec(), spans, trimmed: false };
    }
    let fade = (rate as usize * p.fade_ms as usize) / 1000;
    let spacer = (rate as usize * p.spacer_ms as usize) / 1000;
    let mut out = Vec::with_capacity(kept + spacer * spans.len());
    for (n, &(a, b)) in spans.iter().enumerate() {
        if n > 0 {
            out.extend(std::iter::repeat_n(0i16, spacer));
        }
        let seg = &samples[a..b];
        let len = seg.len();
        for (i, &s) in seg.iter().enumerate() {
            let g = if fade == 0 {
                1.0
            } else {
                let fin = (i as f32 / fade as f32).min(1.0);
                let fout = ((len - 1 - i) as f32 / fade as f32).min(1.0);
                fin.min(fout)
            };
            out.push((s as f32 * g) as i16);
        }
    }
    Trimmed { samples: out, spans, trimmed: true }
}
