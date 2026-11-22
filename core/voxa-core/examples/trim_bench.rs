//! Trim a real recording and report what was kept. Usage: trim_bench <in.wav> <out.wav>

use voxa_core::trim::{TrimParams, trim};

fn main() {
    let mut args = std::env::args().skip(1);
    let (input, output) = (args.next().expect("in.wav"), args.next().expect("out.wav"));
    let mut r = hound::WavReader::open(&input).expect("open");
    let spec = r.spec();
    assert_eq!(spec.channels, 1, "mono expected");
    let samples: Vec<i16> = r.samples::<i16>().map(|s| s.unwrap()).collect();
    let rate = spec.sample_rate;
    let t = std::time::Instant::now();
    let out = trim(&samples, rate, &TrimParams::default());
    let took = t.elapsed();
    let secs = |n: usize| n as f64 / rate as f64;
    println!("input:   {:.1} s ({:.1} min)", secs(samples.len()), secs(samples.len()) / 60.0);
    println!("kept:    {:.1} s in {} phrases (trimmed: {})", secs(out.samples.len()), out.spans.len(), out.trimmed);
    println!("removed: {:.1}%  | trimming took {took:?}", 100.0 * (1.0 - out.samples.len() as f64 / samples.len() as f64));
    let mut w = hound::WavWriter::create(&output, hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }).unwrap();
    for s in &out.samples {
        w.write_sample(*s).unwrap();
    }
    w.finalize().unwrap();
    println!("wrote {output} ({:.2} MB)", std::fs::metadata(&output).unwrap().len() as f64 / 1e6);
}
