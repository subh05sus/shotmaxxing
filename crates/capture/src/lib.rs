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
//! macOS uses ScreenCaptureKit and AVAssetWriter. Windows support (Windows Graphics Capture and Media Foundation)
//! is in progress; until it lands, [`Recorder::start`] returns [`CaptureError::Unsupported`] there.

#![deny(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;

use serde::Serialize;
use shotmaxxing_core::telemetry::{CursorSample, InputEvent};

mod project;

pub use project::{
    CURSOR_FILE, EVENTS_FILE, FinishedProject, ProjectOptions, ProjectRecording, SCREEN_TRACK,
};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
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
    #[error("The capture system reported a problem: {0}")]
    System(String),
    #[error("Couldn't write the recording: {0}")]
    Io(#[from] std::io::Error),
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
#[derive(Clone, Debug, Serialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app: String,
    pub bundle_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
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
}

impl RecordOptions {
    pub fn new(target: Target, output: impl Into<PathBuf>) -> Self {
        Self { target, output: output.into(), max_fps: 60, telemetry: true, hide_desktop_icons: false }
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
}

/// Lists displays and on-screen windows.
pub fn sources() -> Result<Sources, CaptureError> {
    imp::sources()
}

/// A JPEG of each target, at most `max_width` pixels wide, in the same order; `None` where no picture could be
/// taken. Used for the source picker.
pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<Vec<u8>>>, CaptureError> {
    imp::thumbnails(targets, max_width)
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

    pub fn stop(self) -> Result<Recording, CaptureError> {
        self.inner.stop()
    }
}
