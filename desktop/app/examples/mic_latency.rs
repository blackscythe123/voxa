//! Where does the ~0.5 s between hotkey and recording go? Times each step of opening the mic,
//! cold and then warm (device cached), and when the first audio samples arrive.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn open(device: &cpal::Device) {
    let t = Instant::now();
    let config = device.default_input_config().unwrap();
    let t_cfg = t.elapsed();
    let got = Arc::new(AtomicBool::new(false));
    let g = got.clone();
    let stream = device
        .build_input_stream(
            &config.into(),
            move |_d: &[f32], _| {
                g.store(true, Ordering::Relaxed);
            },
            |e| eprintln!("{e}"),
            None,
        )
        .unwrap();
    let t_build = t.elapsed();
    stream.play().unwrap();
    let t_play = t.elapsed();
    while !got.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    println!("  config {t_cfg:?} | build {t_build:?} | play {t_play:?} | first samples {:?}", t.elapsed());
}

fn main() {
    let t = Instant::now();
    let host = cpal::default_host();
    let device = host.default_input_device().expect("no input device");
    println!("host + default device: {:?} ({})", t.elapsed(), device.name().unwrap_or_default());
    println!("open 1 (cold):");
    open(&device);
    println!("open 2 (warm, same device):");
    open(&device);
    println!("open 3 (warm):");
    open(&device);

    // Pre-built, paused stream: how fast does play() deliver samples?
    let config = device.default_input_config().unwrap();
    let got = Arc::new(AtomicBool::new(false));
    let g = got.clone();
    let stream = device
        .build_input_stream(&config.into(), move |_d: &[f32], _| g.store(true, Ordering::Relaxed), |e| eprintln!("{e}"), None)
        .unwrap();
    stream.pause().unwrap();
    for round in 1..=3 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        got.store(false, Ordering::Relaxed);
        let t = Instant::now();
        stream.play().unwrap();
        while !got.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        println!("prebuilt play {round}: first samples after {:?}", t.elapsed());
        std::thread::sleep(std::time::Duration::from_millis(300));
        stream.pause().unwrap();
    }
}

