//! Prints what the record dialog offers: the displays, and the windows it lists, with whether each is in full screen
//! and whether it's on another Space or desktop. Reads only; nothing moves.
//! Usage: cargo run -p shotmaxxing-capture --example source_probe

fn main() {
    shotmaxxing_capture::prepare();
    let sources = shotmaxxing_capture::sources().unwrap_or_else(|error| panic!("listing failed: {error}"));
    for display in &sources.displays {
        println!(
            "display {:>6}  {:>6.0} × {:<6.0} {}",
            display.id, display.width, display.height, display.name
        );
    }
    for window in &sources.windows {
        let mut notes = Vec::new();
        if window.full_screen {
            notes.push("full screen");
        }
        if window.away {
            notes.push(if window.full_screen { "on another Space" } else { "on another desktop" });
        }
        println!(
            "window  {:>6}  {:>6.0} × {:<6.0} {} — {}{}",
            window.id,
            window.width,
            window.height,
            window.app,
            window.title.chars().take(48).collect::<String>(),
            if notes.is_empty() { String::new() } else { format!("  ({})", notes.join(", ")) }
        );
    }
}
