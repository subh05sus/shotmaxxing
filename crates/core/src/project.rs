//! The project model: everything in a `.smx` project's `project.json`.
//!
//! The model is forgiving on the way in (missing fields take defaults, unknown fields are ignored) so projects open
//! across versions, and it is always written back in full.

use crate::geometry::{Point, Size};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub source: SourceInfo,
    pub tracks: Vec<MediaTrack>,
    pub canvas: Canvas,
    pub cursor: CursorStyle,
    pub zoom: ZoomSettings,
    pub zooms: Vec<ZoomRegion>,
    pub edits: Edits,
    pub webcam: WebcamStyle,
    /// Voice cleanup and cut silences.
    pub voice: VoiceSettings,
    pub captions: Vec<Caption>,
    pub export: ExportSettings,
    /// Set when an agent filmed the project.
    pub agent: Option<AgentInfo>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            id: String::new(),
            name: "Untitled recording".into(),
            created_at_ms: 0,
            updated_at_ms: 0,
            source: SourceInfo::default(),
            tracks: Vec::new(),
            canvas: Canvas::default(),
            cursor: CursorStyle::default(),
            zoom: ZoomSettings::default(),
            zooms: Vec::new(),
            edits: Edits::default(),
            webcam: WebcamStyle::default(),
            voice: VoiceSettings::default(),
            captions: Vec::new(),
            export: ExportSettings::default(),
            agent: None,
        }
    }
}

impl Project {
    /// Duration of the screen track, in microseconds.
    pub fn duration_us(&self) -> u64 {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Screen)
            .map(|t| t.offset_us.max(0).unsigned_abs() + t.duration_us)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SourceInfo {
    pub kind: SourceKind,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    /// Capture size in points.
    pub size: Size,
    /// Pixels per point of the captured display (2.0 on Retina).
    pub scale: f64,
    pub platform: Option<crate::keys::Platform>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Display,
    Window,
    Area,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MediaTrack {
    pub id: String,
    pub kind: TrackKind,
    /// Path relative to the project folder, e.g. `media/screen.mp4`.
    pub path: String,
    pub duration_us: u64,
    /// Where the track starts on the screen recording's timeline. Negative when it started first: the microphone
    /// and camera start a moment before the screen so no words are lost.
    #[serde(default)]
    pub offset_us: i64,
    #[serde(default = "one")]
    pub volume: f64,
    #[serde(default)]
    pub muted: bool,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Screen,
    Camera,
    Microphone,
    SystemAudio,
    Music,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum AspectRatio {
    #[default]
    #[serde(rename = "16:9")]
    Widescreen,
    #[serde(rename = "4:3")]
    Classic,
    #[serde(rename = "9:16")]
    Vertical,
    #[serde(rename = "1:1")]
    Square,
    #[serde(rename = "4:5")]
    Portrait,
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "custom")]
    Custom { width: u32, height: u32 },
}

impl AspectRatio {
    /// Width / height, or `None` for the source's own shape.
    pub fn ratio(self) -> Option<f64> {
        match self {
            Self::Widescreen => Some(16.0 / 9.0),
            Self::Classic => Some(4.0 / 3.0),
            Self::Vertical => Some(9.0 / 16.0),
            Self::Square => Some(1.0),
            Self::Portrait => Some(4.0 / 5.0),
            Self::Original => None,
            Self::Custom { width, height } if width > 0 && height > 0 => {
                Some(f64::from(width) / f64::from(height))
            }
            Self::Custom { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Background {
    Wallpaper { id: String },
    Solid { color: String },
    Gradient { from: String, to: String, angle_deg: f64 },
    Image { path: String },
    None,
}

impl Default for Background {
    fn default() -> Self {
        Self::Wallpaper { id: "aurora-lime".into() }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShadowPreset {
    None,
    Subtle,
    /// Matches the shadow the operating system draws under windows.
    #[default]
    Native,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Shadow {
    pub preset: ShadowPreset,
    pub blur: f64,
    pub spread: f64,
    pub offset_y: f64,
    pub opacity: f64,
}

impl Default for Shadow {
    fn default() -> Self {
        Self { preset: ShadowPreset::Native, blur: 48.0, spread: 0.0, offset_y: 18.0, opacity: 0.35 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Canvas {
    pub aspect: AspectRatio,
    pub background: Background,
    /// Space around the capture as a fraction of the canvas's shorter side (0 to 0.3).
    pub padding: f64,
    /// Corner radius of the capture, in output pixels at 1080p.
    pub corner_radius: f64,
    pub shadow: Shadow,
    /// Remove the window's title bar.
    pub window_crop: bool,
    /// Whole-project playback speed.
    pub speed: f64,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            aspect: AspectRatio::Widescreen,
            background: Background::default(),
            padding: 0.08,
            corner_radius: 12.0,
            shadow: Shadow::default(),
            window_crop: false,
            speed: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CursorTheme {
    /// The recording platform's own cursors.
    #[default]
    Native,
    MacosClassic,
    Windows,
    Minimal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClickEffect {
    None,
    /// The cursor squashes briefly.
    Press,
    /// A ring expands from the click.
    Ripple {
        color: String,
        size: f64,
    },
}

impl Default for ClickEffect {
    fn default() -> Self {
        Self::Ripple { color: "accent".into(), size: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Spotlight {
    pub enabled: bool,
    /// Radius in capture points.
    pub radius: f64,
    /// How dark the rest of the frame gets (0 to 1).
    pub dim: f64,
}

impl Default for Spotlight {
    fn default() -> Self {
        Self { enabled: false, radius: 140.0, dim: 0.45 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CursorStyle {
    pub visible: bool,
    pub theme: CursorTheme,
    /// 1.0 is the system size; up to 4.0.
    pub size: f64,
    /// 0 keeps the raw path; 1 is the smoothest.
    pub smoothing: f64,
    pub snap_to_clicks: bool,
    pub click_effect: ClickEffect,
    pub click_sound: bool,
    /// 0 to 1.
    pub motion_blur: f64,
    /// Curve agent and smoothed paths instead of straight lines.
    pub arcs: bool,
    /// Return the cursor to its first position at the end, for looping GIFs.
    pub loop_to_start: bool,
    pub spotlight: Spotlight,
    pub halo: bool,
    /// Hide the cursor after it has been still for this long.
    pub auto_hide_ms: Option<u32>,
}

impl Default for CursorStyle {
    fn default() -> Self {
        Self {
            visible: true,
            theme: CursorTheme::Native,
            size: 1.6,
            smoothing: 0.5,
            snap_to_clicks: true,
            click_effect: ClickEffect::default(),
            click_sound: false,
            motion_blur: 0.3,
            arcs: true,
            loop_to_start: false,
            spotlight: Spotlight::default(),
            halo: false,
            auto_hide_ms: Some(2_500),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoomDepth {
    Subtle,
    #[default]
    Standard,
    Bold,
    Custom(f64),
}

impl ZoomDepth {
    /// The largest scale auto zoom may use.
    pub fn max_scale(self) -> f64 {
        match self {
            Self::Subtle => 1.45,
            Self::Standard => 1.9,
            Self::Bold => 2.6,
            Self::Custom(v) => v.clamp(1.05, 4.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoomMotion {
    #[default]
    Smooth,
    Instant,
    Custom(u32),
}

impl ZoomMotion {
    pub fn transition_ms(self) -> u32 {
        match self {
            Self::Smooth => 700,
            Self::Instant => 0,
            Self::Custom(ms) => ms.min(3_000),
        }
    }
}

/// The advanced auto zoom controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ZoomTuning {
    /// Start a zoom this long before the activity.
    pub lead_ms: u32,
    /// Keep the zoom this long after the activity.
    pub hold_ms: u32,
    /// Activity closer than this in time joins one zoom.
    pub merge_gap_ms: u32,
    pub min_duration_ms: u32,
    pub max_duration_ms: u32,
    /// The smallest scale worth zooming to; below this, stay wide.
    pub min_scale: f64,
    /// Breathing room around the framed control, as a fraction of its size.
    pub margin: f64,
    /// Zoom on a pause of the pointer over something, without a click.
    pub dwell: bool,
    pub dwell_ms: u32,
    /// Zoom on bursts of typing, framing the field being typed into.
    pub typing: bool,
    /// Typing with gaps shorter than this counts as one burst.
    pub typing_gap_ms: u32,
    /// Include screen changes that follow a click, so the result is framed too.
    pub frame_results: bool,
}

impl Default for ZoomTuning {
    fn default() -> Self {
        Self {
            lead_ms: 350,
            hold_ms: 1_100,
            merge_gap_ms: 1_400,
            min_duration_ms: 1_400,
            max_duration_ms: 9_000,
            min_scale: 1.2,
            margin: 0.6,
            dwell: true,
            dwell_ms: 1_000,
            typing: true,
            typing_gap_ms: 900,
            frame_results: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ZoomSettings {
    pub auto: bool,
    pub depth: ZoomDepth,
    pub motion: ZoomMotion,
    pub follow_mouse: bool,
    /// 0 to 1: how far the camera drifts toward the pointer inside a zoom.
    pub follow_strength: f64,
    pub keystrokes: bool,
    pub tuning: ZoomTuning,
}

impl Default for ZoomSettings {
    fn default() -> Self {
        Self {
            auto: true,
            depth: ZoomDepth::Standard,
            motion: ZoomMotion::Smooth,
            follow_mouse: true,
            follow_strength: 0.35,
            keystrokes: true,
            tuning: ZoomTuning::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoomSource {
    #[default]
    Auto,
    Manual,
}

/// One zoom block on the timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZoomRegion {
    pub id: String,
    pub start_us: u64,
    pub end_us: u64,
    pub scale: f64,
    /// Centre of the zoom in capture points.
    pub focus: Point,
    #[serde(default)]
    pub follow_mouse: Option<bool>,
    #[serde(default = "default_ease")]
    pub ease_in_ms: u32,
    #[serde(default = "default_ease")]
    pub ease_out_ms: u32,
    #[serde(default)]
    pub source: ZoomSource,
    /// Why auto zoom added this block, e.g. "Click on Export".
    #[serde(default)]
    pub reason: Option<String>,
}

fn default_ease() -> u32 {
    700
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Edits {
    /// Ranges removed from the output.
    pub cuts: Vec<TimeRange>,
    pub speed: Vec<SpeedRegion>,
    pub texts: Vec<TextOverlay>,
    pub blurs: Vec<BlurRegion>,
    pub arrows: Vec<ArrowOverlay>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeRange {
    pub start_us: u64,
    pub end_us: u64,
}

impl TimeRange {
    pub fn duration_us(self) -> u64 {
        self.end_us.saturating_sub(self.start_us)
    }

    pub fn contains(self, t_us: u64) -> bool {
        t_us >= self.start_us && t_us < self.end_us
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpeedRegion {
    pub range: TimeRange,
    pub factor: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextOverlay {
    pub id: String,
    pub range: TimeRange,
    pub text: String,
    /// Position as a fraction of the canvas (0 to 1).
    pub position: Point,
    #[serde(default)]
    pub style: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlurRegion {
    pub id: String,
    pub range: TimeRange,
    /// Region in capture points.
    pub rect: crate::geometry::Rect,
    #[serde(default = "default_blur")]
    pub strength: f64,
}

fn default_blur() -> f64 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArrowOverlay {
    pub id: String,
    pub range: TimeRange,
    pub from: Point,
    pub to: Point,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WebcamStyle {
    pub visible: bool,
    pub corner: Corner,
    /// Diameter as a fraction of the canvas height.
    pub size: f64,
    pub roundness: f64,
    pub mirror: bool,
    pub shadow: bool,
    /// Shrink the bubble while the screen is zoomed in, so it never covers the action.
    pub shrink_on_zoom: bool,
}

impl Default for WebcamStyle {
    fn default() -> Self {
        Self {
            visible: true,
            corner: Corner::BottomRight,
            size: 0.22,
            roundness: 1.0,
            mirror: true,
            shadow: true,
            shrink_on_zoom: true,
        }
    }
}

/// What the editor does to the recorded voice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct VoiceSettings {
    /// Play and export the cleaned-up copy of the microphone (less hum, fan noise and hiss, an even level).
    pub cleanup: bool,
    /// Long pauses where nothing happens are cut.
    pub cut_silences: bool,
    /// The pauses "Cut silences" took out, so turning it off brings exactly those back.
    pub silences: Vec<TimeRange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Caption {
    pub range: TimeRange,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    #[default]
    Mp4,
    Gif,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportPreset {
    #[default]
    Youtube1080,
    Social,
    Vertical,
    Gif,
    Uhd4k,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExportSettings {
    pub preset: ExportPreset,
    pub format: ExportFormat,
    pub height: u32,
    pub fps: u32,
    pub folder: Option<String>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            preset: ExportPreset::Youtube1080,
            format: ExportFormat::Mp4,
            height: 1080,
            fps: 60,
            folder: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentInfo {
    /// e.g. "Claude Code".
    pub client: String,
    pub take_script: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_json_loads_with_defaults() {
        let p: Project = serde_json::from_str(r#"{"name":"Launch demo","future_field":true}"#).unwrap();
        assert_eq!(p.name, "Launch demo");
        assert_eq!(p.schema_version, PROJECT_SCHEMA_VERSION);
        assert_eq!(p.canvas.aspect, AspectRatio::Widescreen);
        assert!(p.zoom.auto);
    }

    #[test]
    fn aspect_ratios_serialise_as_people_write_them() {
        assert_eq!(serde_json::to_string(&AspectRatio::Vertical).unwrap(), "\"9:16\"");
        let custom: AspectRatio = serde_json::from_str(r#"{"custom":{"width":21,"height":9}}"#).unwrap();
        assert!((custom.ratio().unwrap() - 21.0 / 9.0).abs() < 1e-9);
        assert_eq!(AspectRatio::Original.ratio(), None);
    }

    #[test]
    fn duration_comes_from_the_screen_track() {
        let mut p = Project::default();
        p.tracks.push(MediaTrack {
            id: "s".into(),
            kind: TrackKind::Screen,
            path: "media/screen.mp4".into(),
            duration_us: 42_000_000,
            offset_us: 0,
            volume: 1.0,
            muted: false,
        });
        assert_eq!(p.duration_us(), 42_000_000);
    }
}
