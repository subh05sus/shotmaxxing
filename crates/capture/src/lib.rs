//! Screen capture for Shotmaxxing.
//!
//! A [`Recorder`] films one display or one window into a hardware-encoded video file and, alongside it, records
//! what happened: where the cursor was, clicks, shortcuts and which control was clicked. Auto zoom and the editor
//! read that telemetry later; nothing is drawn into the video itself, so every choice stays editable.
//!
//! Performance rules (see docs/architecture.md):
//! - frames go straight from the capture system to the hardware encoder, never through the CPU;
//! - the frame rate is variable: a still screen produces no frames;
//! - the real cursor is hidden in the capture and recorded as data instead.
//!
//! macOS uses ScreenCaptureKit and AVAssetWriter. Windows uses Windows Graphics Capture and Media Foundation for the
//! picture, WASAPI for the computer's sound and microphones, Media Foundation for the camera, GDI for the source
//! picker's thumbnails and low-level hooks for the input.

#![deny(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shotmaxxing_core::telemetry::{CursorSample, InputEvent};

mod clipboard;
mod project;

pub use clipboard::copy_image;
pub use project::{
    CAMERA_TRACK, CURSOR_FILE, EVENTS_FILE, FinishedProject, MIC_TRACK, ProjectOptions, ProjectRecording,
    SCREEN_TRACK, SCREENSHOT_FILE, SYSTEM_AUDIO_TRACK, save_screenshot,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "windows")]
use win as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use unsupported as imp;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error(
        "Screen Recording isn't allowed yet. Turn on Shotmaxxing in System Settings › Privacy & Security › Screen Recording, then try again."
    )]
    PermissionDenied,
    #[error("Couldn't find {0}. It may have closed; list sources again.")]
    SourceNotFound(String),
    #[error("Recording isn't available on this platform yet.")]
    Unsupported,
    /// A window Stage Manager has parked in its strip that couldn't be brought back to the stage.
    #[error("That window is parked in Stage Manager's strip. Click it to bring it back, then try again.")]
    Parked,
    /// A window that's open but not on the Space showing (minimised, hidden or on another desktop), and didn't come.
    #[error(
        "That window isn't on this desktop: it may be minimised or on another desktop. Show it, then try again."
    )]
    Away,
    #[error("The capture system reported a problem: {0}")]
    System(String),
    #[error("Couldn't write the recording: {0}")]
    Io(#[from] std::io::Error),
    /// A microphone or camera that can't be used, said as the person should read it.
    #[error("{0}")]
    Device(String),
}

/// A voice-over being recorded: a microphone on its own, for the editor to lay over a finished video.
pub struct VoiceRecorder {
    inner: imp::Voice,
    output: PathBuf,
    started: std::time::Instant,
}

/// A finished voice-over.
#[derive(Clone, Debug)]
pub struct VoiceTake {
    pub output: PathBuf,
    pub duration_us: u64,
}

impl VoiceRecorder {
    /// Starts recording `microphone` (the system's default when `None`) into `output`, an `.m4a`, and returns once
    /// the microphone is sending sound, so what starts playing next lines up with the file's beginning.
    pub fn start(microphone: Option<String>, output: impl Into<PathBuf>) -> Result<Self, CaptureError> {
        let output = output.into();
        if let Some(folder) = output.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let inner = imp::Voice::start(microphone, &output)?;
        Ok(Self { inner, output, started: std::time::Instant::now() })
    }

    /// Seconds since it started.
    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Stops and finishes the file. Fails, leaving no file behind, when the microphone sent nothing.
    pub fn stop(self) -> Result<VoiceTake, CaptureError> {
        let seconds = self.inner.stop()?;
        Ok(VoiceTake { output: self.output, duration_us: (seconds * 1e6).round() as u64 })
    }

    /// Stops and throws the recording away.
    pub fn discard(self) {
        let output = self.output.clone();
        let _ = self.inner.stop();
        let _ = std::fs::remove_file(output);
    }
}

/// A display that can be recorded. Sizes and positions are in points, in global screen coordinates.
#[derive(Clone, Debug, Serialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A window that can be recorded.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub bundle_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// A full-screen app's window, filling a Space of its own (macOS).
    #[serde(default)]
    pub full_screen: bool,
    /// On another Space, as a full-screen app is while another Space shows: it can't be pictured from here, and
    /// recording it, or a screenshot of it, goes there first.
    #[serde(default)]
    pub away: bool,
}

/// A microphone or camera.
#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// The one the system uses unless told otherwise.
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Sources {
    pub displays: Vec<DisplayInfo>,
    pub windows: Vec<WindowInfo>,
}

/// Which microphone or camera to record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DeviceChoice {
    #[default]
    Off,
    /// The one the system uses unless told otherwise.
    Default,
    /// A device id from [`microphones`] or [`cameras`].
    Id(String),
}

impl DeviceChoice {
    pub fn is_on(&self) -> bool {
        *self != Self::Off
    }
}

/// A microphone or camera to record, and the file it goes into.
#[derive(Clone, Debug)]
pub struct DeviceRecording {
    /// `None` is the system's default device.
    pub device: Option<String>,
    pub output: PathBuf,
}

/// A track recorded alongside the screen, placed on the screen recording's timeline.
#[derive(Clone, Debug)]
pub struct RecordedTrack {
    pub output: PathBuf,
    /// Where the track starts relative to the screen recording's first frame; negative if it started earlier.
    pub offset_us: i64,
    pub duration_us: u64,
}

/// What to record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A display by id, or `None` for the main display.
    Display(Option<u32>),
    Window(u32),
}

#[derive(Clone, Debug)]
pub struct RecordOptions {
    pub target: Target,
    /// The video file to write (`.mp4`).
    pub output: PathBuf,
    /// The most frames per second to capture. The real rate follows what changes on screen.
    pub max_fps: u32,
    /// Record clicks, shortcuts and the cursor path for auto zoom and the editor.
    pub telemetry: bool,
    /// Leave the desktop icons out of screen recordings. Windows are recorded on their own, so this only applies to
    /// screens.
    pub hide_desktop_icons: bool,
    /// Record what the computer plays into this file (`.m4a`). This app's own sounds are left out on macOS and on
    /// Windows 11 (or Server 2022) and later; older Windows records everything that plays.
    pub system_audio: Option<PathBuf>,
    /// Record a microphone into an `.m4a` file.
    pub microphone: Option<DeviceRecording>,
    /// Record a camera into an `.mp4` file.
    pub camera: Option<DeviceRecording>,
}

impl RecordOptions {
    pub fn new(target: Target, output: impl Into<PathBuf>) -> Self {
        Self {
            target,
            output: output.into(),
            max_fps: 60,
            telemetry: true,
            hide_desktop_icons: false,
            system_audio: None,
            microphone: None,
            camera: None,
        }
    }
}

/// What a finished recording produced.
#[derive(Clone, Debug)]
pub struct Recording {
    pub output: PathBuf,
    pub duration_us: u64,
    /// Frames written. With a variable frame rate this is often far below duration × fps.
    pub frames: u64,
    /// Frames the encoder couldn't take in time (should stay at zero).
    pub dropped: u64,
    /// Size of the captured area in points.
    pub width: f64,
    pub height: f64,
    /// Pixels per point.
    pub scale: f64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub cursor: Vec<CursorSample>,
    pub events: Vec<InputEvent>,
    pub system_audio: Option<RecordedTrack>,
    pub microphone: Option<RecordedTrack>,
    pub camera: Option<RecordedTrack>,
    /// What didn't work without stopping the recording, such as a microphone that wasn't allowed.
    pub warnings: Vec<String>,
}

/// Lists displays, the windows showing on this Space, and full-screen apps' windows on Spaces of their own.
pub fn sources() -> Result<Sources, CaptureError> {
    imp::sources()
}

/// A small picture of a source, for the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Thumbnail {
    /// A JPEG of it.
    Picture(Vec<u8>),
    /// A window that Stage Manager has parked in its strip, which can't be pictured as it is there (it would come out
    /// small and tilted). It comes forward when it's recorded or pictured. Its app's icon, as a PNG, when it has one.
    Parked { icon: Option<Vec<u8>> },
    /// A window on another Space (a full-screen app's), which can't be pictured from this one. The Mac goes there
    /// when it's recorded or pictured. Its app's icon, as a PNG, when it has one.
    Away { icon: Option<Vec<u8>> },
}

/// A picture of each target, at most `max_width` pixels wide, in the same order; `None` where none could be taken.
/// Used for the source picker.
pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<Thumbnail>>, CaptureError> {
    imp::thumbnails(targets, max_width)
}

/// A full-size picture of a screen or a window, without the pointer.
#[derive(Clone, Debug)]
pub struct Screenshot {
    /// The picture, as PNG, in sRGB.
    pub png: Vec<u8>,
    /// Its size in pixels.
    pub width: u32,
    pub height: u32,
    /// Pixels per point.
    pub scale: f64,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
}

/// Takes a screenshot of `target` at its full resolution, without the pointer. A window comes alone, so whatever
/// covers it doesn't show; a screen comes with everything on it, so hide your own windows first.
pub fn screenshot(target: Target) -> Result<Screenshot, CaptureError> {
    let mut shot = imp::screenshot(target)?;
    // The size is read back from the picture itself, so it always matches what's saved.
    (shot.width, shot.height) = png_size(&shot.png)
        .ok_or_else(|| CaptureError::System("the picture couldn't be read back".into()))?;
    // Name what was taken, as the source list does.
    if let Target::Window(id) = target
        && shot.app_name.is_none()
        && let Ok(sources) = imp::sources()
        && let Some(window) = sources.windows.iter().find(|w| w.id == id)
    {
        shot.app_name = Some(window.app.clone()).filter(|a| !a.is_empty());
        shot.window_title = Some(window.title.clone()).filter(|t| !t.is_empty());
    }
    Ok(shot)
}

/// A PNG's size in pixels, from its header.
pub(crate) fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if png.len() < 24 || png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return None;
    }
    let number = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    Some((number(16), number(20))).filter(|&(w, h)| w > 0 && h > 0)
}

/// Microphones, the system default first.
pub fn microphones() -> Vec<Device> {
    let mut found = imp::microphones();
    found.sort_by_key(|d| !d.is_default);
    found
}

/// Cameras, the system default first.
pub fn cameras() -> Vec<Device> {
    let mut found = imp::cameras();
    found.sort_by_key(|d| !d.is_default);
    found
}

/// Command-line tools call this once from their main thread before recording from another thread. Apps don't need
/// to: they are connected to the window server already.
pub fn prepare() {
    imp::prepare();
}

/// Whether the app may capture the screen right now.
pub fn has_permission() -> bool {
    imp::has_permission()
}

/// A recording in progress. Dropping it without calling [`Recorder::stop`] stops capture and keeps what was
/// written so far (the file is fragmented, so it stays playable).
pub struct Recorder {
    inner: imp::Recorder,
}

impl Recorder {
    pub fn start(options: RecordOptions) -> Result<Self, CaptureError> {
        Ok(Self { inner: imp::Recorder::start(options)? })
    }

    /// Seconds recorded so far.
    pub fn elapsed(&self) -> f64 {
        self.inner.elapsed()
    }

    /// Frames written so far. The screen only delivers a frame when something on it changes, so this counts changes.
    pub fn frames(&self) -> u64 {
        self.inner.frames()
    }

    pub fn stop(self) -> Result<Recording, CaptureError> {
        self.inner.stop()
    }
}

#[cfg(test)]
mod tests {
    use super::{Screenshot, Target, png_size, save_screenshot};
    use shotmaxxing_core::bundle::ProjectBundle;
    use shotmaxxing_core::project::{ProjectKind, SourceKind, TrackKind};

    /// The first bytes of a PNG `width` × `height` pixels: its signature and header.
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&width.to_be_bytes());
        png.extend_from_slice(&height.to_be_bytes());
        png
    }

    #[test]
    fn png_sizes_come_from_the_header() {
        assert_eq!(png_size(&png_header(2880, 1800)), Some((2880, 1800)));
        assert_eq!(png_size(&png_header(2880, 1800)[..20]), None, "too short");
        assert_eq!(png_size(&png_header(0, 1800)), None, "empty");
        assert_eq!(png_size(b"GIF89a, which is no PNG at all"), None);
    }

    #[test]
    fn a_screenshot_is_saved_as_a_project_that_frames_it() {
        let dir = tempfile::tempdir().unwrap();
        let shot = Screenshot {
            png: png_header(2880, 1800),
            width: 2880,
            height: 1800,
            scale: 2.0,
            app_name: Some("Safari".into()),
            window_title: Some("Docs".into()),
        };
        let saved = save_screenshot(dir.path(), "Screenshot", Target::Window(42), &shot, None).unwrap();
        assert_eq!(saved.root.file_name().unwrap(), "Screenshot.smx");
        let (bundle, project) = ProjectBundle::open(&saved.root).unwrap();
        assert_eq!(project.kind, ProjectKind::Screenshot);
        assert_eq!(project.source.kind, SourceKind::Window);
        assert_eq!(project.source.app_name.as_deref(), Some("Safari"));
        assert_eq!((project.source.size.width, project.source.size.height), (1440.0, 900.0), "in points");
        assert_eq!(project.tracks.len(), 1);
        assert_eq!(project.tracks[0].kind, TrackKind::Screen);
        assert_eq!(std::fs::read(bundle.root().join(&project.tracks[0].path)).unwrap(), shot.png);
        assert!(!project.zoom.auto && project.zooms.is_empty());
        assert_eq!(project.canvas.aspect, shotmaxxing_core::project::AspectRatio::Original);

        // A second one is numbered.
        // The next one can start from the look of the last.
        let look = shotmaxxing_core::project::Canvas {
            background: shotmaxxing_core::project::Background::Wallpaper { id: "ocean".into() },
            padding: 0.12,
            corner_radius: 20.0,
            ..Default::default()
        };
        let again =
            save_screenshot(dir.path(), "Screenshot", Target::Display(None), &shot, Some(&look)).unwrap();
        assert_eq!(again.root.file_name().unwrap(), "Screenshot 2.smx");
        let (_, styled) = ProjectBundle::open(&again.root).unwrap();
        assert_eq!(styled.canvas.background, look.background);
        assert_eq!((styled.canvas.padding, styled.canvas.corner_radius), (0.12, 20.0));
        assert_eq!(styled.canvas.aspect, shotmaxxing_core::project::AspectRatio::Original);
    }
}
