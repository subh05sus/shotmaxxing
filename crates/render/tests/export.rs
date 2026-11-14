//! Exports a small project end to end. A test pattern from ffmpeg stands in for a screen recording, so nothing on
//! this machine's screen is captured. Skipped when ffmpeg isn't installed.
//!
//! Set `SHOTMAXXING_KEEP_EXPORT` to a folder to keep the exported video and its project for a look.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::{Point, Rect, Size};
use shotmaxxing_core::project::{
    ArrowOverlay, BlurRegion, ClickEffect, MediaTrack, Project, TextOverlay, TimeRange, TrackKind, ZoomRegion,
};
use shotmaxxing_core::telemetry::{CursorSample, InputEvent, InputSource, MouseButton, encode_cursor, write_events};
use shotmaxxing_render::{ExportOptions, Format, RenderError, export};

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
}

fn range(start_ms: u64, end_ms: u64) -> TimeRange {
    TimeRange { start_us: start_ms * 1000, end_us: end_ms * 1000 }
}

/// A four-second "Retina" project with a zoom, a click, a keystroke, a cut, a blur, an arrow and a title.
fn make_project(dir: &Path) -> PathBuf {
    let mut project = Project { name: "Export test".into(), ..Project::default() };
    project.source.size = Size::new(1440.0, 900.0);
    project.source.scale = 2.0;
    project.tracks = vec![MediaTrack {
        id: "screen".into(),
        kind: TrackKind::Screen,
        path: "media/screen.mp4".into(),
        duration_us: 4_000_000,
        offset_us: 0,
        volume: 1.0,
        muted: false,
    }];
    project.zooms = vec![ZoomRegion {
        id: "z1".into(),
        start_us: 1_000_000,
        end_us: 3_000_000,
        scale: 2.0,
        focus: Point::new(500.0, 320.0),
        ..serde_json::from_value(serde_json::json!({
            "id": "z", "start_us": 0, "end_us": 1, "scale": 1.0, "focus": {"x": 0.0, "y": 0.0}
        }))
        .unwrap()
    }];
    project.zoom.keystrokes = true;
    project.cursor.click_effect = ClickEffect::Ripple { color: "#ffffff".into(), size: 1.0 };
    project.cursor.spotlight.enabled = true;
    project.edits.cuts = vec![range(3_200, 3_600)];
    project.edits.blurs = vec![BlurRegion {
        id: "b1".into(),
        range: range(0, 4_000),
        rect: Rect { x: 900.0, y: 120.0, width: 320.0, height: 90.0 },
        strength: 1.0,
    }];
    project.edits.arrows = vec![ArrowOverlay {
        id: "a1".into(),
        range: range(0, 4_000),
        from: Point::new(1200.0, 720.0),
        to: Point::new(980.0, 560.0),
        color: None,
    }];
    project.edits.texts = vec![TextOverlay {
        id: "t1".into(),
        range: range(0, 4_000),
        text: "Export test".into(),
        position: Point::new(0.05, 0.06),
        style: String::new(),
    }];

    let bundle = ProjectBundle::create(dir, &mut project).unwrap();
    let video = bundle.media_dir().join("screen.mp4");
    let encoded = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=2880x1800:rate=30", "-t", "4"])
        .args(["-c:v", "hevc_videotoolbox", "-tag:v", "hvc1", "-b:v", "12M"])
        .arg(&video)
        .status()
        .unwrap();
    assert!(encoded.success(), "ffmpeg couldn't make the test video");

    let cursor: Vec<CursorSample> = (0..=80u32)
        .map(|i| {
            let u = f64::from(i) / 80.0;
            CursorSample { t_us: u64::from(i) * 50_000, x: 200.0 + 800.0 * u, y: 200.0 + 400.0 * u, ..CursorSample::default() }
        })
        .collect();
    std::fs::write(bundle.telemetry_dir().join("cursor.smxc"), encode_cursor(&cursor)).unwrap();
    let events = vec![
        InputEvent::MouseDown {
            t_us: 1_500_000,
            x: 500.0,
            y: 300.0,
            button: MouseButton::Left,
            clicks: 1,
            source: InputSource::Person,
        },
        InputEvent::KeyDown {
            t_us: 2_000_000,
            label: Some("⌘K".into()),
            modifiers_only: false,
            redacted: false,
            source: InputSource::Person,
        },
    ];
    let file = std::fs::File::create(bundle.telemetry_dir().join("events.jsonl")).unwrap();
    write_events(file, &events).unwrap();
    bundle.save(&mut project).unwrap();
    bundle.root().to_path_buf()
}

fn probe(path: &Path) -> Option<(u32, u32, u32)> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width,height,nb_read_frames", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let parts: Vec<u32> = text.trim().split(',').filter_map(|p| p.parse().ok()).collect();
    match parts[..] {
        [w, h, frames] => Some((w, h, frames)),
        _ => None,
    }
}

#[test]
fn exports_every_frame_at_the_right_size() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let output = dir.path().join("Export test.mp4");
    let options =
        ExportOptions { width: 1280, height: 720, fps: 30, format: Format::H264, bitrate: 8e6, output: output.clone() };
    let mut last = 0.0;
    export(&root, &options, &mut |done| {
        assert!(done >= last, "progress went backwards");
        last = done;
        true
    })
    .unwrap();
    assert_eq!(last, 1.0);
    // Four seconds minus the 0.4 s cut, at 30 fps.
    assert_eq!(probe(&output), Some((1280, 720, 108)));
    if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
        std::fs::copy(&output, Path::new(&keep).join("export-test.mp4")).unwrap();
        // The project too, for trying other sizes with `shotmaxxing export`.
        let copied = Command::new("cp").arg("-R").arg(&root).arg(&keep).status().unwrap();
        assert!(copied.success());
    }
}

#[test]
fn cancelling_leaves_nothing_behind() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let output = dir.path().join("Cancelled.mp4");
    let options =
        ExportOptions { width: 640, height: 360, fps: 30, format: Format::H264, bitrate: 2e6, output: output.clone() };
    let mut calls = 0;
    let result = export(&root, &options, &mut |_| {
        calls += 1;
        calls < 10
    });
    assert!(matches!(result, Err(RenderError::Cancelled)), "{result:?}");
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .filter(|name| !name.to_string_lossy().ends_with(".smx"))
        .collect();
    assert!(left.is_empty(), "left behind: {left:?}");
}
