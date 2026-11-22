//! Microphone capture: default input device -> mono 16 kHz 16-bit WAV on disk, written as it
//! records so a crash never loses a dictation. Also publishes a live level for the ink line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};

pub const RATE: u32 = 16_000;

/// Latest loudness, 0..1, as f32 bits. Read by the UI every frame.
pub static LEVEL: AtomicU32 = AtomicU32::new(0);

pub fn level() -> f32 {
    f32::from_bits(LEVEL.load(Ordering::Relaxed))
}

#[derive(Debug)]
pub enum MicError {
    NoDevice,
    /// The device refused to open (most often: Windows microphone privacy is off).
    Blocked(String),
}

type Writer = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

pub struct Recording {
    stream: Stream,
    writer: Arc<Mutex<Option<Writer>>>,
    pub path: PathBuf,
    peak: Arc<AtomicU32>,
}

pub fn has_input_device() -> bool {
    cpal::default_host().default_input_device().is_some()
}

pub fn start(path: &Path) -> Result<Recording, MicError> {
    let device = cpal::default_host().default_input_device().ok_or(MicError::NoDevice)?;
    let config = device.default_input_config().map_err(|e| MicError::Blocked(e.to_string()))?;
    let in_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let writer = hound::WavWriter::create(path, spec).map_err(|e| MicError::Blocked(e.to_string()))?;
    let writer = Arc::new(Mutex::new(Some(writer)));
    let peak = Arc::new(AtomicU32::new(0));

    // Linear resampler state: position in the input stream, in output-sample units.
    let step = in_rate as f64 / RATE as f64;
    let mut pos = 0.0f64;
    let mut prev = 0.0f32;
    let w = writer.clone();
    let pk = peak.clone();
    let mut on_samples = move |mono: &mut dyn Iterator<Item = f32>| {
        let mut sum = 0.0f32;
        let mut n = 0usize;
        let mut out: Vec<i16> = Vec::new();
        for s in mono {
            sum += s * s;
            n += 1;
            // emit output samples that fall between prev and s
            while pos < 1.0 {
                let v = prev + (s - prev) * pos as f32;
                out.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                pos += step;
            }
            pos -= 1.0;
            prev = s;
        }
        if n > 0 {
            let rms = (sum / n as f32).sqrt();
            let lvl = (rms * 6.0).min(1.0); // speech RMS ~0.02-0.2
            LEVEL.store(lvl.to_bits(), Ordering::Relaxed);
            let p = f32::from_bits(pk.load(Ordering::Relaxed)).max(rms);
            pk.store(p.to_bits(), Ordering::Relaxed);
        }
        if let Ok(mut g) = w.lock() {
            if let Some(wr) = g.as_mut() {
                for v in out {
                    let _ = wr.write_sample(v);
                }
            }
        }
    };

    let err = |e| eprintln!("mic stream error: {e}");
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32)),
            err,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _| {
                on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x as f32 / i16::MAX as f32).sum::<f32>() / channels as f32))
            },
            err,
            None,
        ),
        other => return Err(MicError::Blocked(format!("unsupported sample format {other:?}"))),
    }
    .map_err(|e| MicError::Blocked(e.to_string()))?;
    stream.play().map_err(|e| MicError::Blocked(e.to_string()))?;
    Ok(Recording { stream, writer, path: path.to_path_buf(), peak })
}

impl Recording {
    /// Stop and finalise the WAV. Returns the loudest RMS seen (to tell silence from speech).
    pub fn finish(self) -> f32 {
        drop(self.stream);
        if let Ok(mut g) = self.writer.lock() {
            if let Some(w) = g.take() {
                let _ = w.finalize();
            }
        }
        LEVEL.store(0f32.to_bits(), Ordering::Relaxed);
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }

    pub fn discard(self) {
        let path = self.path.clone();
        self.finish();
        let _ = std::fs::remove_file(path);
    }
}
