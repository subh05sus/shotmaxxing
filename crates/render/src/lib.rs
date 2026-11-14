//! Draws a project into a finished video.
//!
//! The editor's preview is HTML and CSS; this crate draws the same picture natively, from the same settings and the
//! same maths: wallpaper, padding, rounded corners and shadow, the zoom camera, the cursor with its clicks, blurs,
//! arrows, titles and the keystroke overlay. On macOS each frame is decoded, composited and encoded on the GPU
//! (AVFoundation and Core Image), so an export runs faster than real time without heating the Mac.

use std::path::{Path, PathBuf};

use serde::Deserialize;

mod animated;
pub mod click;
mod model;
pub mod scene;
mod sprites;
pub mod voice;
pub mod wallpaper;

#[cfg(target_os = "macos")]
mod macos;

pub use model::RenderProject;

/// Where a project's cleaned-up voice goes, next to the original.
pub const CLEAN_MIC: &str = "media/microphone-clean.m4a";

/// The long pauses in a project's voice where nothing happens on screen, as (start, end) seconds of the screen
/// recording: what "Cut silences" takes out. Each keeps a short breath at both ends.
pub fn find_pauses(root: &Path) -> Result<Vec<(f64, f64)>, RenderError> {
    #[cfg(target_os = "macos")]
    {
        macos::voice::find_pauses(root)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Err(RenderError::Unsupported)
    }
}

/// Writes a cleaned-up copy of a project's voice (less hum, fan noise and hiss, an even level) to [`CLEAN_MIC`]
/// and says where it is. `progress` gets 0 to 1 and returns `false` to stop.
pub fn clean_voice(root: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<PathBuf, RenderError> {
    #[cfg(target_os = "macos")]
    {
        macos::voice::clean_voice(root, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (root, progress);
        Err(RenderError::Unsupported)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("{0}")]
    Project(String),
    #[error("{0}")]
    System(String),
    #[error("exporting isn't available on this system yet")]
    Unsupported,
    #[error("the export was cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// MP4 with H.264: plays everywhere.
    H264,
    /// MP4 with HEVC: smaller, for 4K.
    Hevc,
    /// A looping GIF.
    Gif,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ExportOptions {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: Format,
    /// Bits per second, for video formats.
    pub bitrate: f64,
    pub output: PathBuf,
}

/// The size of a video whose shorter side is `short_side` pixels, in the canvas's shape (width ÷ height). Both
/// sides are even, as video encoders need.
pub fn output_size(aspect: f64, short_side: u32) -> (u32, u32) {
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(16);
    let short = f64::from(short_side);
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 16.0 / 9.0 };
    if aspect >= 1.0 { (even(short * aspect), even(short)) } else { (even(short), even(short / aspect)) }
}

/// A bitrate that keeps text crisp at this size and frame rate, in bits per second.
pub fn default_bitrate(width: u32, height: u32, fps: u32, format: Format) -> f64 {
    let bits_per_pixel = if format == Format::Hevc { 0.065 } else { 0.1 };
    (f64::from(width) * f64::from(height) * f64::from(fps) * bits_per_pixel).clamp(2e6, 120e6)
}

/// `<dir>/<stem>.<ext>`, or `<stem> 2.<ext>` and so on when that file exists.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() && n < 10_000 {
        path = dir.join(format!("{stem} {n}.{ext}"));
        n += 1;
    }
    path
}

/// Exports the project at `root` (an `.smx` folder). `progress` receives 0 to 1 and returns `false` to cancel.
/// Nothing is left behind if the export fails or is cancelled.
pub fn export(
    root: &Path,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<(), RenderError> {
    if options.width < 16 || options.height < 16 || options.width > 7680 || options.height > 7680 {
        return Err(RenderError::Project(format!(
            "{} × {} isn't a size that can be exported",
            options.width, options.height
        )));
    }
    let mut options = options.clone();
    if options.format != Format::Gif {
        // 4:2:0 video needs even sides.
        options.width &= !1;
        options.height &= !1;
    }
    let project = RenderProject::load(root)?;
    if project.screen.is_none() {
        return Err(RenderError::Project("this project has no recording to export".into()));
    }
    if project.output_duration() <= 0.0 {
        return Err(RenderError::Project("everything is cut, so there is nothing to export".into()));
    }
    #[cfg(target_os = "macos")]
    {
        macos::export(&project, &options, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (project, progress);
        Err(RenderError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_the_canvas_shape() {
        assert_eq!(output_size(16.0 / 9.0, 1080), (1920, 1080));
        assert_eq!(output_size(9.0 / 16.0, 1080), (1080, 1920));
        assert_eq!(output_size(1.6, 1080), (1728, 1080));
        assert_eq!(output_size(1.0, 720), (720, 720));
        // Odd results round to even.
        assert_eq!(output_size(4.0 / 3.0, 721).1 % 2, 0);
    }

    #[test]
    fn unique_paths_count_up() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "Demo", "mp4");
        assert_eq!(first.file_name().unwrap(), "Demo.mp4");
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(dir.path(), "Demo", "mp4").file_name().unwrap(), "Demo 2.mp4");
    }
}
