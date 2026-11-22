//! Silero trimming on a real recording. Usage: bench <onnxruntime.dll> <in.wav> <out.wav>
fn main() {
    let mut a = std::env::args().skip(1);
    let (dll, input, output) = (a.next().unwrap(), a.next().unwrap(), a.next().unwrap());
    voxa_vad::init(std::path::Path::new(&dll)).expect("onnxruntime");
    let mut r = hound::WavReader::open(&input).unwrap();
    assert_eq!(r.spec().sample_rate, voxa_vad::RATE);
    let samples: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
    let t = std::time::Instant::now();
    let out = voxa_vad::trim_speech(&samples).expect("trim");
    let secs = |n: usize| n as f64 / voxa_vad::RATE as f64;
    println!("input {:.1} s -> kept {:.1} s ({:.1}%) in {} phrases, trimmed={}, took {:?}",
        secs(samples.len()), secs(out.samples.len()), 100.0 * out.samples.len() as f64 / samples.len() as f64, out.spans.len(), out.trimmed, t.elapsed());
    let mut w = hound::WavWriter::create(&output, hound::WavSpec { channels: 1, sample_rate: voxa_vad::RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
    for s in &out.samples { w.write_sample(*s).unwrap(); }
    w.finalize().unwrap();
    println!("wrote {output} ({:.2} MB)", std::fs::metadata(&output).unwrap().len() as f64 / 1e6);
}
