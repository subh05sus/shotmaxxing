//! Recording straight into a `.smx` project: the video goes to `media/`, telemetry to `telemetry/`, and auto zoom
//! runs as soon as the recording stops, so the editor opens with zooms already in place.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::Size;
use shotmaxxing_core::keys::Platform;
use shotmaxxing_core::project::{
    AgentInfo, AspectRatio, Canvas, MediaTrack, Project, ProjectKind, SourceKind, TrackKind,
};
use shotmaxxing_core::telemetry::{encode_cursor, write_events};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};

use crate::{
    CaptureError, DeviceChoice, DeviceRecording, RecordOptions, RecordedTrack, Recorder, Screenshot, Target,
};

pub const SCREEN_TRACK: &str = "media/screen.mp4";
/// A screenshot project's picture.
pub const SCREENSHOT_FILE: &str = "media/screenshot.png";
pub const SYSTEM_AUDIO_TRACK: &str = "media/system-audio.m4a";
pub const MIC_TRACK: &str = "media/microphone.m4a";
pub const CAMERA_TRACK: &str = "media/camera.mp4";
pub const CURSOR_FILE: &str = "telemetry/cursor.smxc";
pub const EVENTS_FILE: &str = "telemetry/events.jsonl";

/// A recording in progress, inside its project folder.
pub struct ProjectRecording {
    bundle: ProjectBundle,
    project: Project,
    recorder: Recorder,
    kind: SourceKind,
}

/// How to record a project.
#[derive(Clone, Debug)]
pub struct ProjectOptions {
    pub target: Target,
    /// The most frames per second to capture. The real rate follows what changes on screen.
    pub max_fps: u32,
    /// Leave the desktop icons out of screen recordings.
    pub hide_desktop_icons: bool,
    /// The agent filming the take, if an agent is.
    pub agent: Option<AgentInfo>,
    /// Record what the computer plays.
    pub system_audio: bool,
    pub microphone: DeviceChoice,
    pub camera: DeviceChoice,
    /// Places whose clicks are Shotmaxxing's own: see [`RecordOptions::left_out`].
    pub left_out: Vec<shotmaxxing_core::geometry::Rect>,
}

impl ProjectOptions {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            max_fps: 60,
            hide_desktop_icons: false,
            agent: None,
            system_audio: false,
            microphone: DeviceChoice::Off,
            camera: DeviceChoice::Off,
            left_out: Vec::new(),
        }
    }
}

/// What a finished recording left on disk.
#[derive(Clone, Debug)]
pub struct FinishedProject {
    pub root: PathBuf,
    pub project: Project,
    pub frames: u64,
    pub dropped: u64,
    /// What didn't work without stopping the recording, such as a microphone that wasn't allowed.
    pub warnings: Vec<String>,
}

fn device(choice: &DeviceChoice, root: &Path, file: &str) -> Option<DeviceRecording> {
    let device = match choice {
        DeviceChoice::Off => return None,
        DeviceChoice::Default => None,
        DeviceChoice::Id(id) => Some(id.clone()),
    };
    Some(DeviceRecording { device, output: root.join(file) })
}

fn media_track(id: &str, kind: TrackKind, path: &str, recorded: &RecordedTrack) -> MediaTrack {
    MediaTrack {
        id: id.into(),
        kind,
        path: path.into(),
        duration_us: recorded.duration_us,
        offset_us: recorded.offset_us,
        volume: 1.0,
        muted: false,
        name: None,
    }
}

fn project_error(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::System(format!("couldn't save the project: {error}"))
}

/// Saves `shot`, taken of `target`, as a new screenshot project `<parent>/<name>.smx` (numbered if the name is
/// taken): the picture, framed like a recording, ready to style, annotate and copy in the editor. With `look`, it
/// starts with that canvas's background, padding, corners and shadow.
pub fn save_screenshot(
    parent: &Path,
    name: &str,
    target: Target,
    shot: &Screenshot,
    look: Option<&Canvas>,
) -> Result<FinishedProject, CaptureError> {
    std::fs::create_dir_all(parent)?;
    let mut project = Project { name: name.to_string(), kind: ProjectKind::Screenshot, ..Project::default() };
    let bundle = ProjectBundle::create(parent, &mut project).map_err(project_error)?;
    let saved = (|| -> Result<(), CaptureError> {
        let picture = bundle.root().join(SCREENSHOT_FILE);
        if let Some(media) = picture.parent() {
            std::fs::create_dir_all(media)?;
        }
        std::fs::write(&picture, &shot.png)?;
        let scale = if shot.scale.is_finite() && shot.scale > 0.0 { shot.scale } else { 1.0 };
        project.source.kind = match target {
            Target::Display(_) => SourceKind::Display,
            Target::Window(_) => SourceKind::Window,
        };
        project.source.app_name = shot.app_name.clone();
        project.source.window_title = shot.window_title.clone();
        project.source.size = Size::new(f64::from(shot.width) / scale, f64::from(shot.height) / scale);
        project.source.scale = scale;
        project.source.platform = Some(Platform::current());
        project.tracks = vec![MediaTrack {
            id: "screen".into(),
            kind: TrackKind::Screen,
            path: SCREENSHOT_FILE.into(),
            duration_us: 0,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        }];
        // Nothing moves in a still, so there's nothing to zoom to, and the frame takes the picture's own shape.
        project.zoom.auto = false;
        project.canvas.aspect = AspectRatio::Original;
        // The look someone gave their last screenshot carries over: background, padding, corners and shadow.
        if let Some(look) = look {
            project.canvas.background = look.background.clone();
            project.canvas.padding = look.padding;
            project.canvas.corner_radius = look.corner_radius;
            project.canvas.shadow = look.shadow.clone();
        }
        bundle.save(&mut project).map_err(project_error)?;
        bundle.snapshot(&project, "Taken").map_err(project_error)?;
        Ok(())
    })();
    if let Err(error) = saved {
        let _ = std::fs::remove_dir_all(bundle.root());
        return Err(error);
    }
    Ok(FinishedProject {
        root: bundle.root().to_path_buf(),
        project,
        frames: 1,
        dropped: 0,
        warnings: Vec::new(),
    })
}

impl ProjectRecording {
    /// Creates `<parent>/<name>.smx` (numbered if the name is taken) and starts recording into it.
    pub fn start(parent: &Path, name: &str, options: ProjectOptions) -> Result<Self, CaptureError> {
        std::fs::create_dir_all(parent)?;
        let target = options.target;
        let mut project = Project { name: name.to_string(), agent: options.agent, ..Project::default() };
        let bundle = ProjectBundle::create(parent, &mut project).map_err(project_error)?;
        let mut record = RecordOptions::new(target, bundle.root().join(SCREEN_TRACK));
        record.max_fps = options.max_fps;
        record.hide_desktop_icons = options.hide_desktop_icons;
        record.system_audio = options.system_audio.then(|| bundle.root().join(SYSTEM_AUDIO_TRACK));
        record.microphone = device(&options.microphone, bundle.root(), MIC_TRACK);
        record.camera = device(&options.camera, bundle.root(), CAMERA_TRACK);
        record.left_out = options.left_out;
        let recorder = match Recorder::start(record) {
            Ok(recorder) => recorder,
            Err(error) => {
                // Don't leave an empty project behind.
                let _ = std::fs::remove_dir_all(bundle.root());
                return Err(error);
            }
        };
        let kind = match target {
            Target::Display(_) => SourceKind::Display,
            Target::Window(_) => SourceKind::Window,
        };
        Ok(Self { bundle, project, recorder, kind })
    }

    pub fn root(&self) -> &Path {
        self.bundle.root()
    }

    pub fn elapsed(&self) -> f64 {
        self.recorder.elapsed()
    }

    /// Frames written so far: the count rises whenever something on screen changes.
    pub fn frames(&self) -> u64 {
        self.recorder.frames()
    }

    /// Stops, writes telemetry, plans auto zoom and saves `project.json`.
    pub fn stop(self) -> Result<FinishedProject, CaptureError> {
        let Self { bundle, mut project, recorder, kind } = self;
        let recording = match recorder.stop() {
            Ok(recording) if recording.frames > 0 => recording,
            Ok(_) => {
                let _ = std::fs::remove_dir_all(bundle.root());
                return Err(CaptureError::System(
                    "nothing was recorded. The screen may be asleep, or the window minimised or on another desktop. \
                     Nothing was saved."
                        .into(),
                ));
            }
            Err(error) => return Err(error),
        };

        let telemetry = bundle.telemetry_dir();
        std::fs::create_dir_all(&telemetry)?;
        std::fs::write(bundle.root().join(CURSOR_FILE), encode_cursor(&recording.cursor))?;
        let events_file = BufWriter::new(File::create(bundle.root().join(EVENTS_FILE))?);
        write_events(events_file, &recording.events).map_err(project_error)?;

        project.source.kind = kind;
        project.source.app_name = recording.app_name.clone();
        project.source.window_title = recording.window_title.clone();
        project.source.size = Size::new(recording.width, recording.height);
        project.source.scale = recording.scale;
        project.source.platform = Some(Platform::current());
        project.tracks = vec![MediaTrack {
            id: "screen".into(),
            kind: TrackKind::Screen,
            path: SCREEN_TRACK.into(),
            duration_us: recording.duration_us,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        }];
        if let Some(track) = &recording.system_audio {
            project.tracks.push(media_track(
                "system-audio",
                TrackKind::SystemAudio,
                SYSTEM_AUDIO_TRACK,
                track,
            ));
        }
        if let Some(track) = &recording.microphone {
            project.tracks.push(media_track("microphone", TrackKind::Microphone, MIC_TRACK, track));
        }
        if let Some(track) = &recording.camera {
            project.tracks.push(media_track("camera", TrackKind::Camera, CAMERA_TRACK, track));
        }

        if project.zoom.auto {
            let input = ZoomInput {
                duration_us: recording.duration_us,
                capture: project.source.size,
                events: &recording.events,
                cursor: &recording.cursor,
            };
            project.zooms = plan_zooms(
                &input,
                &project.zoom.tuning,
                project.zoom.depth,
                project.zoom.motion.transition_ms(),
            );
        }

        bundle.save(&mut project).map_err(project_error)?;
        bundle.snapshot(&project, "Recorded").map_err(project_error)?;
        Ok(FinishedProject {
            root: bundle.root().to_path_buf(),
            project,
            frames: recording.frames,
            dropped: recording.dropped,
            warnings: recording.warnings,
        })
    }
}
