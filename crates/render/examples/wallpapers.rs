//! Draws every built-in wallpaper to a PNG, to compare against the editor's CSS.
//!
//! `cargo run -p shotmaxxing-render --example wallpapers -- <folder> [width] [height]`

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let width: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(640);
    let height: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(360);
    for w in shotmaxxing_render::wallpaper::wallpapers() {
        let pixels =
            shotmaxxing_render::wallpaper::rasterize(&w.css, width, height).expect("a built-in wallpaper");
        let size = tiny_skia::IntSize::from_wh(width, height).expect("a valid size");
        let pixmap = tiny_skia::Pixmap::from_vec(pixels, size).expect("pixels of that size");
        let path = dir.join(format!("{}.png", w.id));
        pixmap.save_png(&path).expect("a writable folder");
        println!("{}", path.display());
    }
}
