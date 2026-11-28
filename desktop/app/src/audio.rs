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
    /// The recording file couldn't be created (disk full, data folder locked or unavailable).
    Disk(#[allow(dead_code)] String),
}

type Writer = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

/// State shared with the audio callback.
struct Shared {
    writer: Mutex<Option<Writer>>,
    peak: AtomicU32,
    reset: AtomicBool,
    /// Audio callbacks since the current take started (0 = the stream delivers nothing).
    callbacks: AtomicU32,
    /// The stream reported an error (device reset, unplugged): rebuild before the next take.
    dead: AtomicBool,
    /// Writing the WAV failed during the take (disk full).
    disk_error: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            writer: Mutex::new(None),
            peak: AtomicU32::new(0),
            reset: AtomicBool::new(true),
            callbacks: AtomicU32::new(0),
            dead: AtomicBool::new(false),
            disk_error: AtomicBool::new(false),
        }
    }
}

/// What went wrong with a take's audio, for the log and the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trouble {
    /// No sound arrived at all, or the stream died mid-take.
    MicStopped,
    /// The WAV couldn't be written.
    Disk,
}

pub struct Mic {
    stream: Stream,
    device_name: String,
    /// The user's choice in Settings; None follows the system default.
    preferred: Option<String>,
    shared: Arc<Shared>,
    path: Option<PathBuf>,
}

fn default_device() -> Result<(cpal::Device, String), MicError> {
    let d = cpal::default_host().default_input_device().ok_or(MicError::NoDevice)?;
    let name = d.name().unwrap_or_default();
    Ok((d, name))
}

/// The preferred microphone if it is plugged in, else the system default (which follows the OS:
/// a new headset, or a change in sound settings, is picked up at the next recording).
fn pick_device(preferred: Option<&str>) -> Result<(cpal::Device, String), MicError> {
    if let Some(want) = preferred {
        if let Ok(devices) = cpal::default_host().input_devices() {
            for d in devices {
                if d.name().is_ok_and(|n| n == want) {
                    return Ok((d, want.to_string()));
                }
            }
        }
    }
    default_device()
}

/// Names of the microphones the OS reports, for the picker in Settings.
pub fn input_devices() -> Vec<String> {
    let mut names: Vec<String> = cpal::default_host().input_devices().map(|ds| ds.filter_map(|d| d.name().ok()).collect()).unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

/// The system default microphone's name.
pub fn default_device_name() -> Option<String> {
    cpal::default_host().default_input_device().and_then(|d| d.name().ok())
}

fn build(device: &cpal::Device, shared: Arc<Shared>) -> Result<Stream, MicError> {
    let config = device.default_input_config().map_err(|e| MicError::Blocked(e.to_string()))?;
    let in_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let step = in_rate as f64 / RATE as f64;
    let mut pos = 0.0f64;
    let mut prev = 0.0f32;

    let errs = shared.clone();
    // The stream's own errors used to go to eprintln (invisible): now they mark it dead and are logged.
    let make_err = move || {
        let s = errs.clone();
        move |e: cpal::StreamError| {
            crate::os::log(&format!("mic stream error: {e}"));
            s.dead.store(true, Ordering::Relaxed);
        }
    };
    let mut on_samples = move |mono: &mut dyn Iterator<Item = f32>| {
        shared.callbacks.fetch_add(1, Ordering::Relaxed);
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
            // Decibel scale, like an audio meter: -52 dB (room) .. -24 dB (normal speech up close) -> 0..1.
            // Was -15 dB at the top, which only shouting reached.
            // Measured speech here: soft 0.016 (-36 dB), normal 0.037 (-29 dB), loud 0.07 (-23 dB).
            let db = 20.0 * rms.max(1e-6).log10();
            LEVEL.store(((db + 52.0) / 28.0).clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
            let p = f32::from_bits(shared.peak.load(Ordering::Relaxed)).max(rms);
            shared.peak.store(p.to_bits(), Ordering::Relaxed);
        }
        if let Ok(mut g) = shared.writer.lock() {
            if let Some(wr) = g.as_mut() {
                for v in out {
                    if wr.write_sample(v).is_err() {
                        shared.disk_error.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    };

    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32)),
            make_err(),
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _| {
                on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x as f32 / i16::MAX as f32).sum::<f32>() / channels as f32))
            },
            make_err(),
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
        Self::with_preferred(None)
    }

    pub fn with_preferred(preferred: Option<String>) -> Result<Self, MicError> {
        let (device, device_name) = pick_device(preferred.as_deref())?;
        let shared = Arc::new(Shared::new());
        let stream = build(&device, shared.clone())?;
        Ok(Self { stream, device_name, preferred, shared, path: None })
    }

    /// Change the preferred microphone; takes effect at the next recording.
    pub fn set_preferred(&mut self, preferred: Option<String>) {
        self.preferred = preferred;
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Start writing to `path`. Rebuilds first if the default microphone changed.
    pub fn start(&mut self, path: &Path) -> Result<(), MicError> {
        let (device, name) = pick_device(self.preferred.as_deref())?;
        // A different mic, or the old stream died (device reset, Bluetooth reconnect): rebuild.
        let dead = self.shared.dead.swap(false, Ordering::Relaxed);
        if name != self.device_name || dead {
            if dead {
                crate::os::log("  mic stream had stopped: rebuilding it");
            }
            self.stream = build(&device, self.shared.clone())?;
            self.device_name = name;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let writer = hound::WavWriter::create(path, spec).map_err(|e| MicError::Disk(e.to_string()))?;
        *self.shared.writer.lock().unwrap() = Some(writer);
        self.shared.peak.store(0f32.to_bits(), Ordering::Relaxed);
        self.shared.callbacks.store(0, Ordering::Relaxed);
        self.shared.disk_error.store(false, Ordering::Relaxed);
        self.shared.reset.store(true, Ordering::Relaxed);
        self.path = Some(path.to_path_buf());
        if let Err(e) = self.stream.play() {
            // Stale stream (device reset): rebuild once and retry.
            self.stream = build(&device, self.shared.clone())?;
            self.stream.play().map_err(|_| MicError::Blocked(e.to_string()))?;
        }
        Ok(())
    }

    /// True when, well into a take, the stream has delivered nothing or reported an error.
    pub fn stalled(&self) -> bool {
        self.shared.dead.load(Ordering::Relaxed) || self.shared.callbacks.load(Ordering::Relaxed) == 0
    }

    /// Rebuild the stream and keep recording into the same file (nothing is shown on screen).
    pub fn revive(&mut self) -> Result<(), MicError> {
        let (device, name) = pick_device(self.preferred.as_deref())?;
        self.stream = build(&device, self.shared.clone())?;
        self.device_name = name;
        self.shared.dead.store(false, Ordering::Relaxed);
        self.stream.play().map_err(|e| MicError::Blocked(e.to_string()))
    }

    /// What went wrong with the take that just finished, if anything.
    pub fn trouble(&self) -> Option<Trouble> {
        if self.shared.disk_error.load(Ordering::Relaxed) {
            Some(Trouble::Disk)
        } else if self.shared.callbacks.load(Ordering::Relaxed) == 0 || self.shared.dead.load(Ordering::Relaxed) {
            Some(Trouble::MicStopped)
        } else {
            None
        }
    }

    /// Stop and finalise the WAV. Returns the loudest RMS seen.
    pub fn finish(&mut self) -> f32 {
        let _ = self.stream.pause();
        if let Some(w) = self.shared.writer.lock().unwrap().take() {
            if let Err(e) = w.finalize() {
                crate::os::log(&format!("  couldn't finish the recording file: {e}"));
                self.shared.disk_error.store(true, Ordering::Relaxed);
            }
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

#[cfg(test)]
mod tests {
    /// Manual: cargo test -p voxa-desktop list_microphones -- --ignored --nocapture
    #[test]
    #[ignore]
    fn list_microphones() {
        eprintln!("default: {:?}", super::default_device_name());
        for d in super::input_devices() {
            eprintln!("  {d}");
        }
    }
}
