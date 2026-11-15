//! Draws a project into a finished video.
//!
//! The editor's preview is HTML and CSS; this crate draws the same picture natively, from the same settings and the
//! same maths: wallpaper, padding, rounded corners and shadow, the zoom camera, the cursor with its clicks, blurs,
//! arrows, titles and the keystroke overlay. On macOS each frame is decoded, composited and encoded on the GPU
//! (AVFoundation and Core Image), so an export runs faster than real time without heating the Mac.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// The GIF writer and the CPU-drawn sprites serve the exporters, which only macOS has so far.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod animated;
pub mod click;
mod model;
pub mod scene;
// Systems without an exporter of their own yet don't draw.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod soft;
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod sprites;
pub mod voice;
pub mod wallpaper;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod win;

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
        // Without a voice there are no pauses to find, on any system.
        let (_, project) = shotmaxxing_core::bundle::ProjectBundle::open(root)
            .map_err(|e| RenderError::Project(e.to_string()))?;
        if project.tracks.iter().any(|t| t.kind == shotmaxxing_core::project::TrackKind::Microphone) {
            Err(RenderError::Unsupported)
        } else {
            Ok(Vec::new())
        }
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
    #[error("this isn't available on this system yet")]
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
    /// One frame as a PNG image, styled like the video: a screenshot with the wallpaper, zoom and cursor.
    Png,
}

impl Format {
    /// The file extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            Self::H264 | Self::Hevc => "mp4",
            Self::Gif => "gif",
            Self::Png => "png",
        }
    }
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
    /// For a PNG: the moment of the recording to show, in seconds. A moment that was cut shows the next one kept.
    #[serde(default)]
    pub at: Option<f64>,
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

/// How long the sound file at `path` plays, in seconds; `None` when it holds no sound this system can play.
pub fn audio_seconds(path: &Path) -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        macos::audio_seconds(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// Music copied into a project.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ImportedMusic {
    /// The copy, relative to the project folder, e.g. `media/music.mp3`.
    pub path: String,
    /// The file's own name, to show.
    pub name: String,
    pub seconds: f64,
}

/// The kinds of music file the mix can play.
pub const MUSIC_TYPES: [&str; 8] = ["mp3", "m4a", "aac", "wav", "aif", "aiff", "caf", "flac"];

/// Copies a music file into the project's media folder. It never overwrites earlier music, which undo may still
/// need. Adding the track to the project is up to the caller.
pub fn import_music(root: &Path, file: &Path) -> Result<ImportedMusic, RenderError> {
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| MUSIC_TYPES.contains(&e.as_str()))
        .ok_or_else(|| RenderError::Project("music can be MP3, M4A, AAC, WAV, AIFF, CAF or FLAC".into()))?;
    if !root.join("project.json").exists() {
        return Err(RenderError::Project(format!("{} isn't a Shotmaxxing project", root.display())));
    }
    let seconds = audio_seconds(file)
        .ok_or_else(|| RenderError::Project(format!("{} has no sound that can be played", file.display())))?;
    let media = root.join("media");
    std::fs::create_dir_all(&media)?;
    let copy = unique_path(&media, "music", &ext);
    std::fs::copy(file, &copy)?;
    let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(ImportedMusic { path: format!("media/{}", name_of(&copy)), name: name_of(file), seconds })
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
    if matches!(options.format, Format::H264 | Format::Hevc) {
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
    #[cfg(windows)]
    {
        win::export(&project, &options, progress)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (project, progress);
        Err(RenderError::Unsupported)
    }
}

/// Where an export is written until it's complete: hidden, next to the final file, with the same extension.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
fn partial(output: &Path) -> PathBuf {
    let name = output.file_stem().and_then(|s| s.to_str()).unwrap_or("export");
    let ext = output.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    output.with_file_name(format!(".{name}.partial.{ext}"))
}

/// Draws the moment `options.at` of the project at `root` with Core Image and with the portable compositor, as
/// RGBA frames, so tests can hold one to the other.
#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn draw_both_ways(root: &Path, options: &ExportOptions) -> Result<(Vec<u8>, Vec<u8>), RenderError> {
    let project = RenderProject::load(root)?;
    macos::draw_both(&project, options)
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
