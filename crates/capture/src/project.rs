//! Recording straight into a `.smx` project: the video goes to `media/`, telemetry to `telemetry/`, and auto zoom
//! runs as soon as the recording stops, so the editor opens with zooms already in place.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::Size;
use shotmaxxing_core::keys::Platform;
use shotmaxxing_core::project::{AgentInfo, MediaTrack, Project, SourceKind, TrackKind};
use shotmaxxing_core::telemetry::{encode_cursor, write_events};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};

use crate::{CaptureError, RecordOptions, Recorder, Target};

pub const SCREEN_TRACK: &str = "media/screen.mp4";
pub const CURSOR_FILE: &str = "telemetry/cursor.smxc";
pub const EVENTS_FILE: &str = "telemetry/events.jsonl";

/// A recording in progress, inside its project folder.
pub struct ProjectRecording {
    bundle: ProjectBundle,
    project: Project,
    recorder: Recorder,
    kind: SourceKind,
}

/// What a finished recording left on disk.
#[derive(Clone, Debug)]
pub struct FinishedProject {
    pub root: PathBuf,
    pub project: Project,
    pub frames: u64,
    pub dropped: u64,
}

fn project_error(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::System(format!("couldn't save the project: {error}"))
}

impl ProjectRecording {
    /// Creates `<parent>/<name>.smx` (numbered if the name is taken) and starts recording into it.
    pub fn start(
        parent: &Path,
        name: &str,
        target: Target,
        max_fps: u32,
        agent: Option<AgentInfo>,
    ) -> Result<Self, CaptureError> {
        std::fs::create_dir_all(parent)?;
        let mut project = Project { name: name.to_string(), agent, ..Project::default() };
        let bundle = ProjectBundle::create(parent, &mut project).map_err(project_error)?;
        let mut options = RecordOptions::new(target, bundle.root().join(SCREEN_TRACK));
        options.max_fps = max_fps;
        let recorder = match Recorder::start(options) {
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

    /// Stops, writes telemetry, plans auto zoom and saves `project.json`.
    pub fn stop(self) -> Result<FinishedProject, CaptureError> {
        let Self { bundle, mut project, recorder, kind } = self;
        let recording = match recorder.stop() {
            Ok(recording) if recording.frames > 0 => recording,
            Ok(_) => {
                let _ = std::fs::remove_dir_all(bundle.root());
                return Err(CaptureError::System(
                    "nothing was captured. The screen may be asleep or the window minimised; nothing was saved.".into(),
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
        }];

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
        })
    }
}
