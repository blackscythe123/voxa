//! Microphone capture: default input device -> mono 16 kHz 16-bit WAV on disk, written as it
//! records so a crash never loses a dictation. Also publishes a live level for the ink line.
//!
//! Building a WASAPI stream takes 300-550 ms, so the stream is built once and kept paused;
//! the hotkey only calls play(), which delivers audio in ~6-17 ms (measured, examples/mic_latency.rs).
//! It is rebuilt when the default input device changes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    Blocked(#[allow(dead_code)] String),
}

type Writer = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

/// State shared with the audio callback.
struct Shared {
    writer: Mutex<Option<Writer>>,
    peak: AtomicU32,
    reset: AtomicBool,
}

pub struct Mic {
    stream: Stream,
    device_name: String,
    shared: Arc<Shared>,
    path: Option<PathBuf>,
}

fn default_device() -> Result<(cpal::Device, String), MicError> {
    let d = cpal::default_host().default_input_device().ok_or(MicError::NoDevice)?;
    let name = d.name().unwrap_or_default();
    Ok((d, name))
}

fn build(device: &cpal::Device, shared: Arc<Shared>) -> Result<Stream, MicError> {
    let config = device.default_input_config().map_err(|e| MicError::Blocked(e.to_string()))?;
    let in_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let step = in_rate as f64 / RATE as f64;
    let mut pos = 0.0f64;
    let mut prev = 0.0f32;

    let mut on_samples = move |mono: &mut dyn Iterator<Item = f32>| {
        if shared.reset.swap(false, Ordering::Relaxed) {
            pos = 0.0;
            prev = 0.0;
        }
        let mut sum = 0.0f32;
        let mut n = 0usize;
        let mut out: Vec<i16> = Vec::new();
        for s in mono {
            sum += s * s;
            n += 1;
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
            // Decibel scale, like an audio meter: -50 dB (room) .. -15 dB (loud) -> 0..1.
            // Measured speech here: soft 0.016 (-36 dB), normal 0.037 (-29 dB), loud 0.07 (-23 dB).
            let db = 20.0 * rms.max(1e-6).log10();
            LEVEL.store(((db + 50.0) / 35.0).clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
            let p = f32::from_bits(shared.peak.load(Ordering::Relaxed)).max(rms);
            shared.peak.store(p.to_bits(), Ordering::Relaxed);
        }
        if let Ok(mut g) = shared.writer.lock() {
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
    // Built but not capturing: nothing is recorded until start().
    stream.pause().map_err(|e| MicError::Blocked(e.to_string()))?;
    Ok(stream)
}

impl Mic {
    /// Build the stream now (paused) so starting later is instant.
    pub fn new() -> Result<Self, MicError> {
        let (device, device_name) = default_device()?;
        let shared = Arc::new(Shared { writer: Mutex::new(None), peak: AtomicU32::new(0), reset: AtomicBool::new(true) });
        let stream = build(&device, shared.clone())?;
        Ok(Self { stream, device_name, shared, path: None })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Start writing to `path`. Rebuilds first if the default microphone changed.
    pub fn start(&mut self, path: &Path) -> Result<(), MicError> {
        let (device, name) = default_device()?;
        if name != self.device_name {
            self.stream = build(&device, self.shared.clone())?;
            self.device_name = name;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let writer = hound::WavWriter::create(path, spec).map_err(|e| MicError::Blocked(e.to_string()))?;
        *self.shared.writer.lock().unwrap() = Some(writer);
        self.shared.peak.store(0f32.to_bits(), Ordering::Relaxed);
        self.shared.reset.store(true, Ordering::Relaxed);
        self.path = Some(path.to_path_buf());
        if let Err(e) = self.stream.play() {
            // Stale stream (device reset): rebuild once and retry.
            self.stream = build(&device, self.shared.clone())?;
            self.stream.play().map_err(|_| MicError::Blocked(e.to_string()))?;
        }
        Ok(())
    }

    /// Stop and finalise the WAV. Returns the loudest RMS seen.
    pub fn finish(&mut self) -> f32 {
        let _ = self.stream.pause();
        if let Some(w) = self.shared.writer.lock().unwrap().take() {
            let _ = w.finalize();
        }
        LEVEL.store(0f32.to_bits(), Ordering::Relaxed);
        self.path = None;
        f32::from_bits(self.shared.peak.load(Ordering::Relaxed))
    }

    pub fn discard(&mut self) {
        let path = self.path.clone();
        self.finish();
        if let Some(p) = path {
            let _ = std::fs::remove_file(p);
        }
    }
}
