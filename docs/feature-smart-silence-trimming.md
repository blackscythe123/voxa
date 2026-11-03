# Smart Silence Trimming & VAD Audio Compression Pipeline

## Overview
When users leave Voxa's recording active during long thought pauses, meetings, or brainstorming sessions, the resulting audio files can stretch to 30–60+ minutes with up to 95%+ dead air (silence and room noise). 

Sending these raw, silence-heavy audio files directly to upstream ASR backends (ChatGPT Whisper / OpenAI ASR API) triggers:
1. **HTTP 500 / 413 Failures:** `{"detail":"Error in ASR API"}` caused by exceeding the 25 MB payload limit or gateway timeouts.
2. **Hallucinations / Degraded Transcripts:** Dense Whisper models hallucinate loops or degrade when fed long stretches of silent background hum.
3. **Bandwidth & Latency Waste:** Uploading 40–50 MB WebM files over consumer connections takes significant time before transcription even begins.

This document records the empirical findings, mathematical tuning parameters, and integration blueprint for adding native **Smart Silence Trimming (VAD)** into Voxa.

---

## Empirical Benchmark & Proof-of-Concept

Tested on real-world dictation session `f7844c3d-5b5c-4db7-8465-77edcd46b182.webm`:

| Metric | Raw Voxa Capture | Post-VAD Cleaned Audio | Optimization |
| :--- | :--- | :--- | :--- |
| **Duration** | **48.43 minutes** (2,905.7s) | **2.34 minutes** (140.6s) | **-46.08 mins (-95.2%)** |
| **File Size** | **44.77 MB** (46,946,036 bytes) | **2.44 MB** (2,559,661 bytes) | **-42.33 MB (-94.5%)** |
| **ASR API Status** | **Failed (HTTP 500)** | **Transcribes in < 4s** | Unblocked |
| **Format** | WebM (Opus 48kHz mono) | WebM (Opus 48kHz mono) | 100% Native Voxa Parity |

---

## Architectural Design & Parameters

### 1. Dual-Rate Acoustic Processing
- **Native Capture:** 48,000 Hz, 1-channel mono float32 (Opus).
- **VAD Evaluation:** Decimated by factor 3 to 16,000 Hz for neural Voice Activity Detection.
- **Slicing & Concatenation:** Applied directly to the original 48 kHz waveform to preserve full audio fidelity without lossy resampling artifacts.

### 2. Tuned VAD Hyperparameters
- **Detection Sensitivity (`threshold = 0.25`):** High sensitivity ensures soft whispers, trailing consonants, and quiet vocalizations are preserved.
- **Speech Padding / Hangover (`speech_pad_ms = 200`):** 
  - Adds 200 ms lead-in before speech onset to protect unvoiced plosives (`p`, `t`, `k`).
  - Adds 200 ms decay after speech offset to protect unvoiced fricatives and breath tails (`s`, `th`, `f`).
- **Minimum Silence Cut Threshold (`min_silence_duration_ms = 300`):**
  - Pauses shorter than 300 ms are part of natural human prosody and must **not** be cut.
  - Pauses longer than 300 ms (dead air between sentences and dictation bursts) are safely excised.
- **Inter-Utterance Breathing Spacer (`100 ms`):** 
  - Splicing distant phrases directly can cause unnatural jarring jumps. Inserting 100 ms of neutral room tone creates smooth, natural conversational cadence.
- **Boundary De-Clicking (`5 ms`):**
  - 5 ms linear fade-in and fade-out applied at cut edges to prevent high-frequency DC offset click artifacts.

---

## Proposed Voxa Native Integration Blueprint

### Option A: Node.js Preprocessing Worker (Recommended)
Add a lightweight VAD utility inside `src/services/audioPreprocessor.js`:
- Triggered inside `finalize-recording` before passing buffer to `chatgptAuth.transcribeAudio()`.
- Condition: Run only if `durationMs > 45000` (45s) or `audioBuffer.length > 5 * 1024 * 1024` (5 MB).
- Implemented via ONNX Runtime / `@ricky0123/vad-node` or WebRTC VAD native addon.
- **Fallback Guard:** If VAD produces less than 1.0 second of audio, bypass trimming and send original audio buffer.

### Option B: Renderer Web Audio Worklet
- Use an `AudioWorklet` in `src/renderer/overlay.html` to track speech energy during recording.
- Flags silent blocks in real-time so silence is dropped before writing to disk.

### User Configuration (Preferences)
Add a setting in `configStore.js`:
```json
{
  "smartSilenceRemoval": "auto", // "auto" | "always" | "off"
  "silenceThresholdMs": 500
}
```

---

## Standalone Recovery CLI Tool
A companion CLI script is maintained at `scripts/trim-silence.py` for manual batch processing and recovery of failed dictation files in `%APPDATA%\Voxa\audio`.
