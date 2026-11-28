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
use cpal::{Sample, SampleFormat, Stream};

pub const RATE: u32 = 16_000;

/// Latest loudness, 0..1, as f32 bits. Read by the UI every frame.
pub static LEVEL: AtomicU32 = AtomicU32::new(0);

pub fn level() -> f32 {
    f32::from_bits(LEVEL.load(Ordering::Relaxed))
}

#[derive(Debug)]
pub enum MicError {
    NoDevice,
    /// Access was refused: Windows microphone privacy is off (or macOS permission not given).
    Blocked(#[allow(dead_code)] String),
    /// Another app holds the microphone for itself (exclusive mode: DAWs, some call apps).
    InUse(#[allow(dead_code)] String),
    /// It wouldn't open for another reason (driver, unsupported format).
    Failed(#[allow(dead_code)] String),
    /// The recording file couldn't be created (disk full, data folder locked or unavailable).
    Disk(#[allow(dead_code)] String),
}

impl MicError {
    /// Sort an audio backend's error by what the user can do about it. Only a refusal of access
    /// sends the user to the privacy settings.
    pub fn from_backend(text: String) -> Self {
        let t = text.to_ascii_lowercase();
        // AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED aside: in use by another app.
        if t.contains("8889000a") || t.contains("in use") || t.contains("exclusive") {
            MicError::InUse(text)
        // E_ACCESSDENIED (privacy off), or the macOS permission.
        } else if t.contains("80070005") || t.contains("access is denied") || t.contains("access denied") || t.contains("not permitted") || t.contains("permission") {
            MicError::Blocked(text)
        } else if t.contains("not available") || t.contains("no longer available") || t.contains("88890004") {
            MicError::NoDevice
        } else {
            MicError::Failed(text)
        }
    }
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
    /// The take is paused: whatever still arrives is dropped before it touches anything.
    paused: AtomicBool,
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
            paused: AtomicBool::new(false),
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
    /// Resumes in this take (silence is only inserted for the first [`MAX_GAPS`]).
    resumes: u32,
    /// When the mic last went live in this take (start or resume); None while paused.
    live_since: Option<std::time::Instant>,
    /// How long the last live stretch lasted, once it ended (pause or finish).
    last_stretch: std::time::Duration,
    /// The take was paused at least once.
    was_paused: bool,
}

/// Silence written at each resume so words either side of a pause don't run together (~300 ms).
const GAP_SAMPLES: usize = (RATE as usize * 3) / 10;
/// After this many pauses in one take, resume without adding more silence.
const MAX_GAPS: u32 = 50;

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
    let config = device.default_input_config().map_err(|e| MicError::from_backend(e.to_string()))?;
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
        // Paused: drop late buffers first, so they never reach the level, the peak or the file.
        if shared.paused.load(Ordering::Relaxed) {
            return;
        }
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
        // Interfaces that only offer 32-bit, 24-in-32, unsigned or 64-bit float samples.
        SampleFormat::I32 => device.build_input_stream(
            &config.into(),
            move |data: &[i32], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x.to_sample::<f32>()).sum::<f32>() / channels as f32)),
            make_err(),
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x.to_sample::<f32>()).sum::<f32>() / channels as f32)),
            make_err(),
            None,
        ),
        SampleFormat::U8 => device.build_input_stream(
            &config.into(),
            move |data: &[u8], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x.to_sample::<f32>()).sum::<f32>() / channels as f32)),
            make_err(),
            None,
        ),
        SampleFormat::F64 => device.build_input_stream(
            &config.into(),
            move |data: &[f64], _| on_samples(&mut data.chunks(channels).map(|f| f.iter().map(|&x| x as f32).sum::<f32>() / channels as f32)),
            make_err(),
            None,
        ),
        other => return Err(MicError::Failed(format!("unsupported sample format {other:?}"))),
    }
    .map_err(|e| MicError::from_backend(e.to_string()))?;
    // Built but not capturing: nothing is recorded until start().
    stream.pause().map_err(|e| MicError::from_backend(e.to_string()))?;
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
        Ok(Self { stream, device_name, preferred, shared, path: None, resumes: 0, live_since: None, last_stretch: std::time::Duration::ZERO, was_paused: false })
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
        self.shared.paused.store(false, Ordering::Relaxed);
        self.resumes = 0;
        self.live_since = Some(std::time::Instant::now());
        self.was_paused = false;
        self.path = Some(path.to_path_buf());
        if let Err(e) = self.stream.play() {
            // Stale stream (device reset): rebuild once and retry.
            self.stream = build(&device, self.shared.clone())?;
            self.stream.play().map_err(|_| MicError::from_backend(e.to_string()))?;
        }
        Ok(())
    }

    /// Turn the mic off mid-take and keep the file open. The OS mic indicator goes out, as when
    /// idle; nothing is recorded until [`Mic::resume`].
    pub fn pause(&mut self) {
        self.shared.paused.store(true, Ordering::Relaxed);
        if let Err(e) = self.stream.pause() {
            crate::os::log(&format!("  mic pause: {e}"));
        }
        LEVEL.store(0f32.to_bits(), Ordering::Relaxed);
        if let Some(t) = self.live_since.take() {
            self.last_stretch = t.elapsed();
        }
        self.was_paused = true;
    }

    /// Turn the mic back on and keep writing to the same file, after a short silence. Rebuilds
    /// first if the microphone changed or the stream died during the pause, as [`Mic::start`] does.
    pub fn resume(&mut self) -> Result<(), MicError> {
        let (device, name) = pick_device(self.preferred.as_deref())?;
        let dead = self.shared.dead.swap(false, Ordering::Relaxed);
        if name != self.device_name || dead {
            crate::os::log(&format!("  resume: rebuilding the mic stream ({})", if dead { "it had stopped" } else { "the microphone changed" }));
            self.stream = build(&device, self.shared.clone())?;
            self.device_name = name;
        }
        self.resumes += 1;
        if self.resumes <= MAX_GAPS {
            if let Ok(mut g) = self.shared.writer.lock() {
                if let Some(wr) = g.as_mut() {
                    for _ in 0..GAP_SAMPLES {
                        if wr.write_sample(0i16).is_err() {
                            self.shared.disk_error.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        }
        // The stall watchdog judges the stretch since this resume.
        self.shared.callbacks.store(0, Ordering::Relaxed);
        self.shared.reset.store(true, Ordering::Relaxed);
        self.shared.paused.store(false, Ordering::Relaxed);
        self.live_since = Some(std::time::Instant::now());
        if let Err(e) = self.stream.play() {
            self.stream = build(&device, self.shared.clone())?;
            self.stream.play().map_err(|_| MicError::from_backend(e.to_string()))?;
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
        self.stream.play().map_err(|e| MicError::from_backend(e.to_string()))
    }

    /// Seconds of audio in the take's file so far (talking time plus the silence at each resume).
    pub fn written_secs(&self) -> f32 {
        self.shared.writer.lock().ok().and_then(|g| g.as_ref().map(|w| w.len() as f32 / RATE as f32)).unwrap_or(0.0)
    }

    /// What went wrong with the take that just finished, if anything.
    pub fn trouble(&self) -> Option<Trouble> {
        if self.shared.disk_error.load(Ordering::Relaxed) {
            Some(Trouble::Disk)
        } else if self.shared.dead.load(Ordering::Relaxed) || (self.shared.callbacks.load(Ordering::Relaxed) == 0 && !self.too_short_to_judge()) {
            Some(Trouble::MicStopped)
        } else {
            None
        }
    }

    /// A paused take whose last live stretch was too short for any sound to arrive (paused right
    /// after starting or resuming): no callbacks then is not a dead mic.
    fn too_short_to_judge(&self) -> bool {
        self.was_paused && self.last_stretch < std::time::Duration::from_millis(300)
    }

    /// Stop and finalise the WAV. Returns the loudest RMS seen.
    pub fn finish(&mut self) -> f32 {
        let _ = self.stream.pause();
        self.shared.paused.store(false, Ordering::Relaxed);
        if let Some(t) = self.live_since.take() {
            self.last_stretch = t.elapsed();
        }
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
    use super::MicError;

    /// Realworld 18: only a refusal of access is "microphone access is off". A mic held by
    /// another app, or one that won't open for another reason, says something else.
    #[test]
    fn only_refused_access_is_reported_as_privacy() {
        let in_use = "A backend-specific error has occurred: Device in use (0x8889000A)";
        assert!(matches!(MicError::from_backend(in_use.into()), MicError::InUse(_)));
        let denied = "A backend-specific error has occurred: Access is denied. (0x80070005)";
        assert!(matches!(MicError::from_backend(denied.into()), MicError::Blocked(_)));
        let gone = "The requested device is no longer available. For example, it has been unplugged.";
        assert!(matches!(MicError::from_backend(gone.into()), MicError::NoDevice));
        let other = "A backend-specific error has occurred: The audio engine failed (0x88890008)";
        assert!(matches!(MicError::from_backend(other.into()), MicError::Failed(_)));
    }

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
