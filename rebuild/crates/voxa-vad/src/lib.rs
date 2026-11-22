//! Silero voice activity detection, used to cut silence before upload.
//!
//! A loudness detector cannot do this job: on a real 48-minute recording the room's background
//! (median -34 dB) was louder than soft speech elsewhere (-36 dB), and both a loudness detector and
//! WebRTC VAD kept 64-99% of it. Silero separates voice from background (spec: 2.3 of 48 min kept).

use std::path::Path;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use voxa_core::trim::{TrimParams, Trimmed, trim_with};

const MODEL: &[u8] = include_bytes!("../model/silero_vad.onnx");
/// Silero takes 512-sample chunks at 16 kHz (32 ms).
pub const CHUNK: usize = 512;
pub const RATE: u32 = 16_000;
/// Speech probability threshold from the trimming spec: sensitive, so soft speech is kept.
pub const THRESHOLD: f32 = 0.25;
/// Once speech has started it continues until the probability falls below this (Silero's own
/// hysteresis, threshold - 0.15), so soft word endings are not cut.
pub const RELEASE: f32 = THRESHOLD - 0.15;

#[derive(Debug)]
pub struct VadError(pub String);

impl<E: std::fmt::Display> From<E> for VadError {
    fn from(e: E) -> Self {
        Self(e.to_string())
    }
}

/// Load onnxruntime.dll from `dll`. Call once per process before creating a [`Silero`].
pub fn init(dll: &Path) -> Result<(), VadError> {
    if !dll.exists() {
        return Err(VadError(format!("{} not found", dll.display())));
    }
    ort::init_from(dll.to_string_lossy().to_string()).commit()?;
    Ok(())
}

pub struct Silero {
    session: Session,
    state: ndarray::Array3<f32>,
}

impl Silero {
    pub fn new() -> Result<Self, VadError> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?
            .commit_from_memory(MODEL)?;
        Ok(Self { session, state: ndarray::Array3::zeros((2, 1, 128)) })
    }

    pub fn reset(&mut self) {
        self.state = ndarray::Array3::zeros((2, 1, 128));
    }

    /// Speech probability for one 512-sample chunk at 16 kHz.
    pub fn probability(&mut self, chunk: &[i16]) -> Result<f32, VadError> {
        let mut input = ndarray::Array2::<f32>::zeros((1, CHUNK));
        for (i, s) in chunk.iter().take(CHUNK).enumerate() {
            input[[0, i]] = *s as f32 / 32768.0;
        }
        let inputs = ort::inputs![
            Tensor::from_array(input)?,
            Tensor::from_array(std::mem::take(&mut self.state))?,
            Tensor::from_array(ndarray::arr0::<i64>(RATE as i64))?,
        ];
        let outputs = self.session.run(inputs)?;
        let state = outputs.get("stateN").ok_or_else(|| VadError("no stateN".into()))?.try_extract_array::<f32>()?;
        self.state = state.to_owned().into_dimensionality::<ndarray::Ix3>()?;
        let p = outputs.get("output").ok_or_else(|| VadError("no output".into()))?.try_extract_array::<f32>()?;
        Ok(p[[0, 0]])
    }
}

/// Trim silence from 16 kHz mono audio with Silero. Falls back to the original audio on any error.
pub fn trim_speech(samples: &[i16]) -> Result<Trimmed, VadError> {
    let mut vad = Silero::new()?;
    let p = TrimParams { frame_ms: 32, ..TrimParams::default() };
    let mut failed = None;
    let mut in_speech = false;
    let out = trim_with(samples, RATE, &p, |f| {
        if f.len() < CHUNK || failed.is_some() {
            return false;
        }
        match vad.probability(f) {
            Ok(prob) => {
                in_speech = if in_speech { prob >= RELEASE } else { prob >= THRESHOLD };
                in_speech
            }
            Err(e) => {
                failed = Some(e);
                false
            }
        }
    });
    match failed {
        Some(e) => Err(e),
        None => Ok(out),
    }
}
