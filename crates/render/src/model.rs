//! A project as the renderer sees it: the editor's model, read from `project.json` and the telemetry files.
//!
//! This mirrors `fromCore` in the editor (apps/desktop/src/lib/project.ts) so an export shows exactly what the
//! preview showed: the same clips, zooms, effects and cursor settings, in seconds.

use std::path::{Path, PathBuf};

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::cursor::{PathSettings, render_path};
use shotmaxxing_core::geometry::{Point, Rect, Size};
use shotmaxxing_core::project::{
    Background, ClickEffect, Corner, CursorTheme, Project, TrackKind, ZoomMotion, ZoomRegion,
};
use shotmaxxing_core::telemetry::{CursorSample, InputEvent, decode_cursor, read_events};

use crate::RenderError;

const CURSOR_FILE: &str = "telemetry/cursor.smxc";
const EVENTS_FILE: &str = "telemetry/events.jsonl";

/// A part of the recording that is kept, played at `speed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub start: f64,
    pub end: f64,
    pub speed: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    pub start: f64,
    pub end: f64,
    pub scale: f64,
    pub focus: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorLook {
    Mac,
    Windows,
    Dot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    None,
    Press,
    Ripple,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Top-left corner as fractions of the frame.
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    pub start: f64,
    pub end: f64,
    /// In capture points.
    pub rect: Rect,
    /// 1 is the standard blur.
    pub strength: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrow {
    pub start: f64,
    pub end: f64,
    /// In capture points.
    pub from: Point,
    pub to: Point,
}

/// A recorded sound, placed on the screen recording's timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub path: PathBuf,
    /// Where it starts on the screen's timeline, in seconds; negative when it started first.
    pub offset: f64,
    /// 1 is as recorded.
    pub volume: f64,
}

/// The camera bubble, drawn in a corner above everything else.
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub path: PathBuf,
    /// Where the camera starts on the screen's timeline, in seconds.
    pub offset: f64,
    /// Diameter as a fraction of the frame height.
    pub size: f64,
    /// Corner radius as a fraction of the diameter: 0.5 is a circle.
    pub radius: f64,
    pub left: bool,
    pub top: bool,
    pub mirror: bool,
    pub shadow: bool,
    /// Ease down to 70% while zoomed in, so it never covers the action.
    pub shrink: bool,
}

#[derive(Clone, Debug)]
pub struct RenderProject {
    pub name: String,
    /// The capture's size in points.
    pub capture: Size,
    /// The screen recording.
    pub screen: Option<PathBuf>,
    pub duration: f64,
    pub clips: Vec<Clip>,
    pub zooms: Vec<Zoom>,
    pub instant_zoom: bool,
    pub follow_cursor: bool,
    pub wallpaper: String,
    pub padding: f64,
    pub radius: f64,
    /// 0 to 1.
    pub shadow: f64,
    /// Width / height of the canvas, or `None` for the capture's own shape.
    pub aspect: Option<f64>,
    pub cursor_visible: bool,
    pub cursor_look: CursorLook,
    pub cursor_size: f64,
    pub click: Click,
    pub spotlight: bool,
    pub hide_idle: bool,
    /// Motion blur strength, 0 (off) to 1: the cursor smears over `MOTION_BLUR_S` times this.
    pub motion_blur: f64,
    /// A soft click under every press, as the editor plays it.
    pub click_sounds: bool,
    /// The drawn cursor path, in seconds and capture points.
    pub path: Vec<CursorSample>,
    pub clicks: Vec<(f64, Point)>,
    pub keys: Vec<(f64, String)>,
    pub keystrokes: bool,
    pub titles: Vec<Title>,
    pub blurs: Vec<Blur>,
    pub arrows: Vec<Arrow>,
    /// Every sound to mix into the export, loudness included.
    pub sounds: Vec<Sound>,
    pub bubble: Option<Bubble>,
}

const fn sec(us: u64) -> f64 {
    us as f64 / 1_000_000.0
}

/// The kept parts of the recording: the gaps between cuts, each with the speed set where it starts.
pub fn clips_of(project: &Project, duration: f64) -> Vec<Clip> {
    let mut cuts = project.edits.cuts.clone();
    cuts.sort_by_key(|c| c.start_us);
    let speed_at = |t: f64| {
        let us = (t * 1_000_000.0).round().max(0.0) as u64;
        project.edits.speed.iter().find(|s| s.range.contains(us)).map_or(1.0, |s| s.factor.clamp(0.1, 16.0))
    };
    let mut clips = Vec::new();
    let mut push = |start: f64, end: f64| {
        if end - start > 0.01 {
            clips.push(Clip { start, end, speed: speed_at(start) });
        }
    };
    let mut at = 0.0;
    for cut in cuts {
        push(at, sec(cut.start_us));
        at = f64::max(at, sec(cut.end_us));
    }
    push(at, duration);
    if clips.is_empty() {
        clips.push(Clip { start: 0.0, end: duration, speed: 1.0 });
    }
    clips
}

fn zoom_of(z: &ZoomRegion) -> Zoom {
    Zoom { start: sec(z.start_us), end: sec(z.end_us), scale: z.scale, focus: z.focus }
}

pub(crate) fn read_telemetry(root: &Path) -> (Vec<CursorSample>, Vec<InputEvent>) {
    let cursor = std::fs::read(root.join(CURSOR_FILE))
        .ok()
        .and_then(|bytes| decode_cursor(&bytes).ok())
        .unwrap_or_default();
    let events = std::fs::File::open(root.join(EVENTS_FILE))
        .ok()
        .and_then(|file| read_events(std::io::BufReader::new(file)).ok())
        .unwrap_or_default();
    (cursor, events)
}

impl RenderProject {
    /// Reads the project at `root` (an `.smx` folder).
    pub fn load(root: &Path) -> Result<Self, RenderError> {
        let (bundle, project) = ProjectBundle::open(root).map_err(|e| RenderError::Project(e.to_string()))?;
        let root = bundle.root();
        let (cursor, events) = read_telemetry(root);
        Ok(Self::from_parts(root, &project, &cursor, &events))
    }

    pub fn from_parts(
        root: &Path,
        project: &Project,
        cursor: &[CursorSample],
        events: &[InputEvent],
    ) -> Self {
        let duration = sec(project.duration_us());
        let screen = project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Screen)
            .map(|t| root.join(&t.path))
            .filter(|p| p.exists());
        let mut zooms: Vec<Zoom> = project.zooms.iter().map(zoom_of).collect();
        zooms.sort_by(|a, b| a.start.total_cmp(&b.start));
        let wallpaper = match &project.canvas.background {
            Background::Wallpaper { id } => id.clone(),
            _ => "aurora-lime".into(),
        };
        let style = &project.cursor;
        let cursor_look = match style.theme {
            CursorTheme::Minimal => CursorLook::Dot,
            CursorTheme::Windows => CursorLook::Windows,
            CursorTheme::MacosClassic => CursorLook::Mac,
            CursorTheme::Native => {
                if cfg!(windows) {
                    CursorLook::Windows
                } else {
                    CursorLook::Mac
                }
            }
        };
        let click = match style.click_effect {
            ClickEffect::None => Click::None,
            ClickEffect::Press => Click::Press,
            ClickEffect::Ripple { .. } => Click::Ripple,
        };
        let path = render_path(
            cursor,
            events,
            PathSettings {
                smoothing: style.smoothing,
                snap_to_clicks: style.snap_to_clicks,
                loop_to_start: style.loop_to_start,
            },
        );
        let mut clicks = Vec::new();
        let mut keys = Vec::new();
        for event in events {
            match event {
                InputEvent::MouseDown { t_us, x, y, .. } => clicks.push((sec(*t_us), Point::new(*x, *y))),
                InputEvent::KeyDown { t_us, label: Some(label), .. } if !label.is_empty() => {
                    keys.push((sec(*t_us), label.clone()));
                }
                _ => {}
            }
        }
        let sounds = project
            .tracks
            .iter()
            .filter(|t| {
                matches!(t.kind, TrackKind::Microphone | TrackKind::SystemAudio | TrackKind::Music)
                    && !t.muted
                    && t.volume > 0.0
            })
            .map(|t| {
                // With cleanup on, the voice plays from its cleaned copy, as in the editor.
                let clean = root.join(crate::CLEAN_MIC);
                let path = if t.kind == TrackKind::Microphone && project.voice.cleanup && clean.exists() {
                    clean
                } else {
                    root.join(&t.path)
                };
                (t, path)
            })
            .filter(|(_, path)| path.exists())
            .map(|(t, path)| Sound {
                path,
                offset: t.offset_us as f64 / 1e6,
                volume: t.volume.clamp(0.0, 2.0),
            })
            .collect();
        let webcam = &project.webcam;
        let bubble = project
            .tracks
            .iter()
            .filter(|_| webcam.visible)
            .find(|t| t.kind == TrackKind::Camera)
            .map(|t| (t, root.join(&t.path)))
            .filter(|(_, path)| path.exists())
            .map(|(t, path)| Bubble {
                path,
                offset: t.offset_us as f64 / 1e6,
                size: webcam.size.clamp(0.05, 0.6),
                // The editor's three shapes: circle, rounded and square.
                radius: if webcam.roundness >= 0.9 {
                    0.5
                } else if webcam.roundness >= 0.15 {
                    0.22
                } else {
                    0.06
                },
                left: matches!(webcam.corner, Corner::TopLeft | Corner::BottomLeft),
                top: matches!(webcam.corner, Corner::TopLeft | Corner::TopRight),
                mirror: webcam.mirror,
                shadow: webcam.shadow,
                shrink: webcam.shrink_on_zoom,
            });
        let edits = &project.edits;
        Self {
            name: project.name.clone(),
            capture: project.source.size,
            screen,
            duration,
            clips: clips_of(project, duration),
            zooms,
            instant_zoom: project.zoom.motion == ZoomMotion::Instant,
            follow_cursor: project.zoom.follow_mouse,
            wallpaper,
            padding: project.canvas.padding,
            radius: project.canvas.corner_radius,
            shadow: (project.canvas.shadow.opacity / 0.6).clamp(0.0, 1.0),
            aspect: project.canvas.aspect.ratio(),
            cursor_visible: style.visible,
            cursor_look,
            cursor_size: style.size,
            click,
            spotlight: style.spotlight.enabled,
            hide_idle: style.auto_hide_ms.unwrap_or(0) > 0,
            motion_blur: if style.motion_blur.is_finite() { style.motion_blur.clamp(0.0, 1.0) } else { 0.0 },
            click_sounds: style.click_sound,
            path,
            clicks,
            keys,
            keystrokes: project.zoom.keystrokes,
            titles: edits
                .texts
                .iter()
                .map(|t| Title {
                    start: sec(t.range.start_us),
                    end: sec(t.range.end_us),
                    text: t.text.clone(),
                    at: t.position,
                })
                .collect(),
            blurs: edits
                .blurs
                .iter()
                .map(|b| Blur {
                    start: sec(b.range.start_us),
                    end: sec(b.range.end_us),
                    rect: b.rect,
                    strength: b.strength.clamp(0.2, 4.0),
                })
                .collect(),
            arrows: edits
                .arrows
                .iter()
                .map(|a| Arrow {
                    start: sec(a.range.start_us),
                    end: sec(a.range.end_us),
                    from: a.from,
                    to: a.to,
                })
                .collect(),
            sounds,
            bubble,
        }
    }

    /// The canvas's shape, width ÷ height.
    pub fn canvas_aspect(&self) -> f64 {
        self.aspect.unwrap_or(self.capture.width / self.capture.height.max(1.0))
    }

    /// Length of the video after cuts and speed changes.
    pub fn output_duration(&self) -> f64 {
        self.clips.iter().map(|c| (c.end - c.start) / c.speed).sum()
    }

    /// The moment of the recording shown at `t` seconds into the exported video.
    pub fn source_time(&self, t: f64) -> Option<f64> {
        self.clip_at(t).map(|(_, source)| source)
    }

    /// Which clip plays at `t` seconds into the exported video, and the moment of the recording it shows.
    pub fn clip_at(&self, t: f64) -> Option<(usize, f64)> {
        let mut at = 0.0;
        for (i, clip) in self.clips.iter().enumerate() {
            let length = (clip.end - clip.start) / clip.speed;
            if t < at + length {
                return Some((i, clip.start + (t - at) * clip.speed));
            }
            at += length;
        }
        None
    }

    /// When the moment `source` of the recording shows in the exported video; `None` when it was cut.
    pub fn export_time(&self, source: f64) -> Option<f64> {
        let mut at = 0.0;
        for clip in &self.clips {
            if source >= clip.start && source < clip.end {
                return Some(at + (source - clip.start) / clip.speed);
            }
            at += (clip.end - clip.start) / clip.speed;
        }
        None
    }

    /// When each click sounds in the exported video, in order: every press that wasn't cut, if click sounds are on.
    pub fn click_times(&self) -> Vec<f64> {
        if !self.click_sounds {
            return Vec::new();
        }
        let mut times: Vec<f64> = self.clicks.iter().filter_map(|(t, _)| self.export_time(*t)).collect();
        times.sort_by(f64::total_cmp);
        times
    }

    /// The largest zoom, so sprites can be drawn sharp enough for it.
    pub fn max_zoom(&self) -> f64 {
        self.zooms.iter().map(|z| z.scale).fold(1.0, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shotmaxxing_core::project::{SpeedRegion, TimeRange};

    fn project_with(cuts: Vec<TimeRange>, speed: Vec<SpeedRegion>) -> Project {
        let mut project = Project::default();
        project.edits.cuts = cuts;
        project.edits.speed = speed;
        project
    }

    fn range(start_ms: u64, end_ms: u64) -> TimeRange {
        TimeRange { start_us: start_ms * 1000, end_us: end_ms * 1000 }
    }

    #[test]
    fn clips_are_the_gaps_between_cuts() {
        let project = project_with(vec![range(2_000, 3_000), range(8_000, 10_000)], vec![]);
        let clips = clips_of(&project, 10.0);
        assert_eq!(
            clips,
            vec![Clip { start: 0.0, end: 2.0, speed: 1.0 }, Clip { start: 3.0, end: 8.0, speed: 1.0 }]
        );
    }

    #[test]
    fn output_time_maps_through_cuts_and_speed() {
        let project = project_with(
            vec![range(2_000, 3_000)],
            vec![SpeedRegion { range: range(3_000, 10_000), factor: 2.0 }],
        );
        let mut render = RenderProject::from_parts(Path::new("/tmp"), &project, &[], &[]);
        render.clips = clips_of(&project, 10.0);
        assert_eq!(render.output_duration(), 2.0 + 3.5);
        assert_eq!(render.source_time(1.0), Some(1.0));
        // Just after the cut, the recording continues at 3 s, twice as fast.
        assert_eq!(render.source_time(2.5), Some(4.0));
        assert_eq!(render.source_time(6.0), None);
    }

    #[test]
    fn clicks_sound_where_they_land_in_the_export() {
        let project = project_with(
            vec![range(2_000, 3_000)],
            vec![SpeedRegion { range: range(3_000, 10_000), factor: 2.0 }],
        );
        let mut render = RenderProject::from_parts(Path::new("/tmp"), &project, &[], &[]);
        render.clips = clips_of(&project, 10.0);
        for t in [0.5, 1.9, 4.0, 9.5] {
            assert_eq!(render.source_time(render.export_time(t).unwrap()), Some(t));
        }
        assert_eq!(render.export_time(2.5), None, "inside the cut");
        assert_eq!(render.export_time(4.0), Some(2.5));

        // Out of order, and one inside the cut.
        render.clicks = [5.0, 1.0, 2.5].iter().map(|&t| (t, Point::new(0.0, 0.0))).collect();
        assert!(render.click_times().is_empty(), "off unless the project asks");
        render.click_sounds = true;
        assert_eq!(render.click_times(), vec![1.0, 3.0]);
    }
}
