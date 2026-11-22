#!/usr/bin/env python3
"""
trim-silence.py — Voice Activity Detection (VAD) Smart Silence Trimming Utility for Voxa.

Eliminates dead air, pauses, and background hum from Voxa WebM recordings while
preserving natural speech pacing and protecting consonant onsets/offsets.

Requirements:
    pip install av numpy torch scipy silero-vad
"""

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

try:
    import av
    import numpy as np
    import torch
    from scipy import signal
    from silero_vad import get_speech_timestamps, load_silero_vad
except ImportError as err:
    print(f"Error: Missing dependency: {err}")
    print("Please install requirements: pip install av numpy torch scipy silero-vad")
    sys.exit(1)


def trim_silence(
    input_path: Path,
    output_path: Path,
    vad_threshold: float = 0.25,
    speech_pad_ms: int = 200,
    min_silence_duration_ms: int = 300,
    spacer_ms: int = 100,
    bit_rate: int = 128000,
):
    print(f"[*] Processing: {input_path}")
    orig_size_bytes = input_path.stat().st_size

    # 1. Decode original audio (preserving native sample rate, typically 48kHz for Opus/WebM)
    container = av.open(str(input_path))
    if not container.streams.audio:
        raise ValueError("No audio streams found in input container.")
    
    stream = container.streams.audio[0]
    orig_rate = stream.rate
    chunks = []
    for frame in container.decode(stream):
        chunks.append(frame.to_ndarray()[0])
    container.close()

    if not chunks:
        raise ValueError("Audio stream contains no decodable frames.")

    audio_orig = np.concatenate(chunks).astype(np.float32)
    orig_duration_s = len(audio_orig) / orig_rate
    print(f"    - Input Duration: {orig_duration_s:.2f}s ({orig_duration_s / 60:.2f} min)")
    print(f"    - Input Size:     {orig_size_bytes:,} bytes ({orig_size_bytes / (1024 * 1024):.2f} MB)")

    # 2. Resample to 16 kHz for Silero VAD neural network
    vad_rate = 16000
    if orig_rate % vad_rate == 0:
        decimate_factor = orig_rate // vad_rate
        audio_16k = signal.decimate(audio_orig, decimate_factor).copy()
    else:
        num_target_samples = int(len(audio_orig) * vad_rate / orig_rate)
        audio_16k = signal.resample(audio_orig, num_target_samples).astype(np.float32)

    tensor_16k = torch.from_numpy(audio_16k).float()

    # 3. Detect speech intervals
    model = load_silero_vad()
    speech_ts = get_speech_timestamps(
        tensor_16k,
        model,
        sampling_rate=vad_rate,
        threshold=vad_threshold,
        min_speech_duration_ms=150,
        min_silence_duration_ms=min_silence_duration_ms,
        speech_pad_ms=speech_pad_ms,
        return_seconds=False,
    )

    if not speech_ts:
        print("[!] Warning: No speech detected with current threshold. Keeping original audio.")
        shutil.copy2(input_path, output_path)
        return {
            "orig_duration_s": orig_duration_s,
            "new_duration_s": orig_duration_s,
            "orig_size": orig_size_bytes,
            "new_size": orig_size_bytes,
            "speech_intervals": 0,
        }

    # 4. Map timestamps back to native rate and consolidate nearby bursts
    ratio = orig_rate / vad_rate
    min_gap_samples = int(0.150 * orig_rate)  # Merge pauses under 150ms
    merged_intervals = []

    for item in speech_ts:
        st = max(0, int(item["start"] * ratio))
        en = min(len(audio_orig), int(item["end"] * ratio))

        if not merged_intervals:
            merged_intervals.append([st, en])
        else:
            prev = merged_intervals[-1]
            if st <= prev[1] + min_gap_samples:
                prev[1] = max(prev[1], en)
            else:
                merged_intervals.append([st, en])

    # 5. Slice and de-click boundaries (5ms linear fades)
    fade_len = int(0.005 * orig_rate)
    fade_in = np.linspace(0.0, 1.0, fade_len, dtype=np.float32)
    fade_out = np.linspace(1.0, 0.0, fade_len, dtype=np.float32)
    pause_spacer = np.zeros(int((spacer_ms / 1000.0) * orig_rate), dtype=np.float32)

    speech_pieces = []
    for idx, (st, en) in enumerate(merged_intervals):
        piece = audio_orig[st:en].copy()
        if len(piece) > 2 * fade_len:
            piece[:fade_len] *= fade_in
            piece[-fade_len:] *= fade_out
        speech_pieces.append(piece)
        if idx < len(merged_intervals) - 1 and spacer_ms > 0:
            speech_pieces.append(pause_spacer)

    processed_audio = np.concatenate(speech_pieces)
    new_duration_s = len(processed_audio) / orig_rate

    # 6. Re-encode to clean Opus WebM
    temp_output = output_path.with_suffix(".tmp.webm")
    out_container = av.open(str(temp_output), mode="w", format="webm")
    out_stream = out_container.add_stream("opus", rate=orig_rate)
    out_stream.layout = "mono"
    out_stream.bit_rate = bit_rate

    frame_size = int(orig_rate * 0.020)  # 20ms frames (960 samples @ 48kHz)
    for i in range(0, len(processed_audio), frame_size):
        chunk = processed_audio[i : i + frame_size]
        if len(chunk) < frame_size:
            chunk = np.pad(chunk, (0, frame_size - len(chunk)))
        frame = av.AudioFrame.from_ndarray(chunk.reshape(1, -1), format="fltp", layout="mono")
        frame.rate = orig_rate
        for packet in out_stream.encode(frame):
            out_container.mux(packet)

    for packet in out_stream.encode(None):
        out_container.mux(packet)
    out_container.close()

    # Move temp output to final output
    if output_path.exists():
        os.remove(output_path)
    os.rename(temp_output, output_path)

    new_size_bytes = output_path.stat().st_size
    diff_s = orig_duration_s - new_duration_s
    pct_cut = (diff_s / orig_duration_s) * 100 if orig_duration_s > 0 else 0

    print(f"[+] Complete:")
    print(f"    - Clean Duration: {new_duration_s:.2f}s ({new_duration_s / 60:.2f} min)")
    print(f"    - Clean Size:     {new_size_bytes:,} bytes ({new_size_bytes / (1024 * 1024):.2f} MB)")
    print(f"    - Cut Silence:    {diff_s:.2f}s ({pct_cut:.1f}% reduction)")
    print(f"    - Speech Bursts:  {len(merged_intervals)}")

    return {
        "orig_duration_s": orig_duration_s,
        "new_duration_s": new_duration_s,
        "orig_size": orig_size_bytes,
        "new_size": new_size_bytes,
        "speech_intervals": len(merged_intervals),
    }


def update_voxa_history(audio_id: str, new_duration_ms: int, new_bytes: int):
    history_path = Path(os.path.expandvars(r"%APPDATA%\Voxa\history.json"))
    if not history_path.exists():
        return False

    try:
        with open(history_path, "r", encoding="utf-8") as f:
            data = json.load(f)

        updated = False
        for entry in data:
            if entry.get("id") == audio_id or entry.get("audio_file", "").startswith(audio_id):
                entry["duration_ms"] = new_duration_ms
                entry["audio_bytes"] = new_bytes
                updated = True
                break

        if updated:
            with open(history_path, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
            print(f"[+] Updated Voxa history metadata for record: {audio_id}")
            return True
    except Exception as ex:
        print(f"[!] Could not update history.json: {ex}")
    return False


def main():
    parser = argparse.ArgumentParser(
        description="Smart Silence Trimming & VAD Audio Compression for Voxa WebM files."
    )
    parser.add_argument("input", nargs="?", help="Path to input WebM audio file")
    parser.add_argument("-i", "--input-file", dest="input_opt", help="Input file path (flag alias)")
    parser.add_argument("-o", "--output", help="Output file path (default: <input>_trimmed.webm)")
    parser.add_argument("--in-place", action="store_true", help="Replace the original file in-place (creates .bak)")
    parser.add_argument("--threshold", type=float, default=0.25, help="VAD threshold 0.0-1.0 (default: 0.25)")
    parser.add_argument("--pad-ms", type=int, default=200, help="Speech padding lead-in/out in ms (default: 200)")
    parser.add_argument("--spacer-ms", type=int, default=100, help="Pause spacer between phrases in ms (default: 100)")
    parser.add_argument("--update-history", action="store_true", help="Sync duration/size into AppData/Voxa/history.json")

    args = parser.parse_args()
    input_str = args.input or args.input_opt

    if not input_str:
        parser.print_help()
        sys.exit(1)

    input_path = Path(input_str).resolve()
    if not input_path.exists():
        print(f"Error: File not found: {input_path}")
        sys.exit(1)

    if args.in_place:
        backup_path = input_path.with_suffix(input_path.suffix + ".bak")
        if not backup_path.exists():
            shutil.copy2(input_path, backup_path)
            print(f"[+] Backup saved: {backup_path}")
        output_path = input_path
    elif args.output:
        output_path = Path(args.output).resolve()
    else:
        output_path = input_path.with_name(f"{input_path.stem}_trimmed{input_path.suffix}")

    stats = trim_silence(
        input_path=backup_path if args.in_place else input_path,
        output_path=output_path,
        vad_threshold=args.threshold,
        speech_pad_ms=args.pad_ms,
        spacer_ms=args.spacer_ms,
    )

    if args.update_history:
        audio_id = input_path.stem
        update_voxa_history(audio_id, int(stats["new_duration_s"] * 1000), stats["new_size"])


if __name__ == "__main__":
    main()
