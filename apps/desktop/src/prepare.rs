//! Before upload: cut long silences (Silero); after transcription: optional filler removal.

use std::path::Path;
use std::sync::OnceLock;

use crate::os;

/// Takes shorter than this are sent as recorded.
const TRIM_FROM_SECS: f32 = 30.0;

fn silero_ready() -> bool {
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        let mut dll = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("onnxruntime.dll")));
        // Test binaries live in cargo's build dir, not beside the app.
        if cfg!(test) && !dll.as_ref().is_some_and(|d| d.exists()) {
            dll = Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/onnxruntime/onnxruntime.dll"));
        }
        match dll.map(|d| voxa_vad::init(&d)) {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                os::log(&format!("  trim: Silero unavailable ({}), long takes go untrimmed", e.0));
                false
            }
            None => false,
        }
    })
}

/// The bytes to upload: the WAV as recorded, or with long silences cut.
pub fn audio_for_upload(path: &Path) -> Option<Vec<u8>> {
    let original = std::fs::read(path).ok()?;
    let Ok(mut r) = hound::WavReader::new(std::io::Cursor::new(&original)) else { return Some(original) };
    let spec = r.spec();
    let secs = r.duration() as f32 / spec.sample_rate as f32;
    if secs < TRIM_FROM_SECS || spec.sample_rate != voxa_vad::RATE || spec.channels != 1 || !silero_ready() {
        return Some(original);
    }
    let samples: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
    let t = std::time::Instant::now();
    match voxa_vad::trim_speech(&samples) {
        Ok(out) if out.trimmed => {
            let kept = out.samples.len() as f32 / spec.sample_rate as f32;
            os::log(&format!("  trim: {secs:.1}s -> {kept:.1}s in {} phrases ({:?})", out.spans.len(), t.elapsed()));
            let mut buf = std::io::Cursor::new(Vec::new());
            let mut w = hound::WavWriter::new(&mut buf, spec).ok()?;
            for s in &out.samples {
                w.write_sample(*s).ok()?;
            }
            w.finalize().ok()?;
            Some(buf.into_inner())
        }
        Ok(_) => {
            os::log(&format!("  trim: {secs:.1}s, too little speech found to trim safely, sending as recorded"));
            Some(original)
        }
        Err(e) => {
            os::log(&format!("  trim failed ({}), sending as recorded", e.0));
            Some(original)
        }
    }
}

pub fn finish_text(text: String, dir: &Path) -> String {
    if crate::store::Settings::load(dir).remove_fillers { voxa_core::text::remove_fillers(&text) } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("voxa-prepare-{}.wav", samples.len()));
        let spec = hound::WavSpec { channels: 1, sample_rate: voxa_vad::RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for s in samples {
            w.write_sample(*s).unwrap();
        }
        w.finalize().unwrap();
        p
    }

    #[test]
    fn short_takes_are_sent_as_recorded() {
        let p = wav(&vec![100i16; voxa_vad::RATE as usize * 5]);
        assert_eq!(audio_for_upload(&p).unwrap(), std::fs::read(&p).unwrap());
    }

    /// The real 48-minute take (decoded to 16 kHz WAV at %TEMP%\long48.wav), through the app's path.
    #[test]
    #[ignore]
    fn real_long_take_is_trimmed_through_the_app_path() {
        let p = std::env::temp_dir().join("long48.wav");
        let before = std::fs::metadata(&p).unwrap().len();
        let after = audio_for_upload(&p).unwrap().len() as u64;
        eprintln!("upload: {before} -> {after} bytes");
        assert!(silero_ready(), "onnxruntime.dll next to the test exe");
        assert!(after * 20 < before, "expected >95% cut");
    }

    #[test]
    fn fillers_follow_the_setting() {
        let dir = std::env::temp_dir().join("voxa-prepare-settings");
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join("settings.json"));
        assert_eq!(finish_text("Um, hello.".into(), &dir), "Um, hello.", "off by default");
        std::fs::write(dir.join("settings.json"), r#"{"remove_fillers": true}"#).unwrap();
        assert_eq!(finish_text("Um, hello.".into(), &dir), "Hello.");
    }
}
