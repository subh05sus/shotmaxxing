//! Records a few seconds of a window with the computer's sound and the default microphone, and prints where each
//! track landed on the screen's timeline. Tracks are kept even when no frame arrives (a sleeping screen), so the
//! sound path can be checked on its own.
//! Usage: cargo run -p shotmaxxing-capture --example sound_probe -- <window id> <out dir> [--camera]

use shotmaxxing_capture::{DeviceRecording, RecordOptions, Recorder, Target};

fn main() {
    let mut args = std::env::args().skip(1);
    let id: u32 = args.next().and_then(|a| a.parse().ok()).expect("window id");
    let out = std::path::PathBuf::from(args.next().expect("out dir"));
    let camera = args.next().as_deref() == Some("--camera");
    std::fs::create_dir_all(&out).expect("out dir");
    shotmaxxing_capture::prepare();
    let mut options = RecordOptions::new(Target::Window(id), out.join("screen.mp4"));
    options.telemetry = false;
    options.system_audio = Some(out.join("system-audio.m4a"));
    options.microphone = Some(DeviceRecording { device: None, output: out.join("microphone.m4a") });
    if camera {
        options.camera = Some(DeviceRecording { device: None, output: out.join("camera.mp4") });
    }
    let recorder = Recorder::start(options).unwrap_or_else(|error| panic!("start failed: {error}"));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let done = recorder.stop().unwrap_or_else(|error| panic!("stop failed: {error}"));
    println!("screen: {} frames over {} µs", done.frames, done.duration_us);
    for (name, track) in
        [("system audio", &done.system_audio), ("microphone", &done.microphone), ("camera", &done.camera)]
    {
        match track {
            Some(t) => println!("{name}: offset {} µs, {} µs long", t.offset_us, t.duration_us),
            None => println!("{name}: nothing"),
        }
    }
    for warning in &done.warnings {
        println!("warning: {warning}");
    }
}
