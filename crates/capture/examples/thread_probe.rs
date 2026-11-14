//! Records three seconds of a window from a background thread, the way the app does, and prints what came out.
//! Usage: cargo run -p shotmaxxing-capture --example thread_probe -- <window id> <out dir>

fn main() {
    let mut args = std::env::args().skip(1);
    let id: u32 = args.next().and_then(|a| a.parse().ok()).expect("window id");
    let out = std::path::PathBuf::from(args.next().expect("out dir"));
    shotmaxxing_capture::prepare();
    let handle = std::thread::spawn(move || {
        let options = shotmaxxing_capture::ProjectOptions::new(shotmaxxing_capture::Target::Window(id));
        let rec = shotmaxxing_capture::ProjectRecording::start(&out, "probe", options)
            .unwrap_or_else(|error| panic!("start failed: {error}"));
        std::thread::sleep(std::time::Duration::from_secs(3));
        rec.stop()
    });
    match handle.join().expect("thread") {
        Ok(done) => println!("frames {} dropped {} at {}", done.frames, done.dropped, done.root.display()),
        Err(error) => println!("error: {error}"),
    }
}
