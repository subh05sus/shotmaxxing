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
use shotmaxxing_core::telemetry::{
    CursorSample, InputEvent, InputSource, MouseButton, encode_cursor, write_events,
};
use shotmaxxing_render::{ExportOptions, Format, RenderError, export};

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
}

fn range(start_ms: u64, end_ms: u64) -> TimeRange {
    TimeRange { start_us: start_ms * 1000, end_us: end_ms * 1000 }
}

fn track(kind: TrackKind, path: &str, duration_us: u64, offset_us: i64, volume: f64) -> MediaTrack {
    MediaTrack {
        id: path.into(),
        kind,
        path: path.into(),
        duration_us,
        offset_us,
        volume,
        muted: false,
        name: None,
    }
}

fn ffmpeg(args: &[&str], out: &Path) {
    let status = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).arg(out).status().unwrap();
    assert!(status.success(), "ffmpeg couldn't make {}", out.display());
}

/// A four-second "Retina" project with a zoom, a click, a keystroke, a cut, a blur, an arrow and a title, plus a
/// microphone that started 0.2 s before the screen (turned up to 140%), a shorter system sound and a camera.
fn make_project(dir: &Path) -> PathBuf {
    let mut project = Project { name: "Export test".into(), ..Project::default() };
    project.source.size = Size::new(1440.0, 900.0);
    project.source.scale = 2.0;
    project.tracks = vec![
        track(TrackKind::Screen, "media/screen.mp4", 4_000_000, 0, 1.0),
        track(TrackKind::Microphone, "media/microphone.m4a", 4_300_000, -200_000, 1.4),
        track(TrackKind::SystemAudio, "media/system-audio.m4a", 2_000_000, 500_000, 0.8),
        track(TrackKind::Camera, "media/camera.mp4", 4_000_000, 100_000, 1.0),
    ];
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
    let media = bundle.media_dir();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=2880x1800:rate=30",
            "-t",
            "4",
            "-c:v",
            "hevc_videotoolbox",
            "-tag:v",
            "hvc1",
            "-b:v",
            "8M",
        ],
        &media.join("screen.mp4"),
    );
    ffmpeg(
        &["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=4.3", "-c:a", "aac"],
        &media.join("microphone.m4a"),
    );
    ffmpeg(
        &["-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000:duration=2", "-ac", "2", "-c:a", "aac"],
        &media.join("system-audio.m4a"),
    );
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x360:rate=30",
            "-t",
            "4",
            "-c:v",
            "hevc_videotoolbox",
            "-tag:v",
            "hvc1",
            "-b:v",
            "8M",
        ],
        &media.join("camera.mp4"),
    );

    let cursor: Vec<CursorSample> = (0..=80u32)
        .map(|i| {
            let u = f64::from(i) / 80.0;
            CursorSample {
                t_us: u64::from(i) * 50_000,
                x: 200.0 + 800.0 * u,
                y: 200.0 + 400.0 * u,
                ..CursorSample::default()
            }
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

/// The exported sound's codec, channels and length in seconds.
fn probe_sound(path: &Path) -> Option<(String, u32, f64)> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0"])
        .args(["-show_entries", "stream=codec_name,channels,duration", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let parts: Vec<&str> = text.trim().split(',').collect();
    match parts[..] {
        [codec, channels, duration] => {
            Some((codec.to_string(), channels.parse().ok()?, duration.parse().ok()?))
        }
        _ => None,
    }
}

/// The loudest moment and the average level of the exported sound, in dB.
fn loudness(path: &Path) -> (f64, f64) {
    let out = Command::new("ffmpeg")
        .arg("-i")
        .arg(path)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    let find = |key: &str| -> f64 {
        log.lines()
            .find_map(|l| l.split(key).nth(1))
            .and_then(|rest| rest.trim().trim_end_matches("dB").trim().parse().ok())
            .unwrap_or(f64::NAN)
    };
    (find("max_volume:"), find("mean_volume:"))
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
    let options = ExportOptions {
        width: 1280,
        height: 720,
        fps: 30,
        format: Format::H264,
        bitrate: 8e6,
        output: output.clone(),
        at: None,
    };
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
    // The microphone covers the whole video, so the sound is as long as the picture.
    let (codec, channels, seconds) = probe_sound(&output).expect("the export has no sound");
    assert_eq!((codec.as_str(), channels), ("aac", 2));
    assert!((seconds - 3.6).abs() < 0.08, "the sound lasts {seconds} s");
    // A -18 dB tone at 140%, with a quieter one on top for a while: clearly there, and never clipping.
    let (peak, mean) = loudness(&output);
    assert!(peak < 0.0 && peak > -15.0, "peak {peak} dB");
    assert!(mean > -25.0, "mean {mean} dB");
    if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
        std::fs::copy(&output, Path::new(&keep).join("export-test.mp4")).unwrap();
        // The project too, for trying other sizes with `shotmaxxing export`.
        let copied = Command::new("cp").arg("-R").arg(&root).arg(&keep).status().unwrap();
        assert!(copied.success());
    }
}

#[test]
fn click_sounds_play_under_presses_even_without_a_recorded_sound() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let (bundle, mut project) = ProjectBundle::open(&root).unwrap();
    for track in &mut project.tracks {
        if matches!(track.kind, TrackKind::Microphone | TrackKind::SystemAudio) {
            track.muted = true;
        }
    }
    project.cursor.click_sound = true;
    bundle.save(&mut project).unwrap();

    let output = dir.path().join("Clicks.mp4");
    let options = ExportOptions {
        width: 640,
        height: 360,
        fps: 30,
        format: Format::H264,
        bitrate: 4e6,
        output: output.clone(),
        at: None,
    };
    export(&root, &options, &mut |_| true).unwrap();
    // The click needs a sound track, which runs the whole video.
    let (codec, channels, seconds) = probe_sound(&output).expect("the export has no sound");
    assert_eq!((codec.as_str(), channels), ("aac", 2));
    assert!((seconds - 3.6).abs() < 0.08, "the sound lasts {seconds} s");
    // One click at about -9 dB, when the press happened, and quiet around it.
    let (peak, mean) = loudness(&output);
    assert!(peak > -13.0 && peak < -5.0, "peak {peak} dB");
    assert!(mean < -30.0, "mean {mean} dB");
    let at = loudest_at(&output);
    assert!((at - 1.5).abs() < 0.03, "the click sounds at {at} s");
}

#[test]
fn music_plays_on_the_videos_own_timeline_and_fades_out() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let (bundle, mut project) = ProjectBundle::open(&root).unwrap();
    for track in &mut project.tracks {
        if matches!(track.kind, TrackKind::Microphone | TrackKind::SystemAudio) {
            track.muted = true;
        }
    }
    // Ten seconds of music, longer than the video, starting half a second in.
    ffmpeg(
        &["-f", "lavfi", "-i", "sine=frequency=330:sample_rate=44100:duration=10", "-ac", "2", "-c:a", "aac"],
        &bundle.media_dir().join("music.m4a"),
    );
    let mut music = track(TrackKind::Music, "media/music.m4a", 10_000_000, 500_000, 1.0);
    music.name = Some("Theme.m4a".into());
    project.tracks.push(music);
    bundle.save(&mut project).unwrap();

    let output = dir.path().join("Music.mp4");
    let options = ExportOptions {
        width: 640,
        height: 360,
        fps: 30,
        format: Format::H264,
        bitrate: 4e6,
        output: output.clone(),
        at: None,
    };
    export(&root, &options, &mut |_| true).unwrap();
    // The cut leaves 3.6 s, and the music runs to the very end of it, not past.
    let (_, _, seconds) = probe_sound(&output).expect("the export has no sound");
    assert!((seconds - 3.6).abs() < 0.08, "the sound lasts {seconds} s");
    let pcm = samples(&output);
    let level = |from: f64, to: f64| level_db(&pcm, from, to);
    // Quiet before it starts, full while it plays (across the cut at 3.2 s in the recording), and faded at the end.
    assert!(level(0.05, 0.4) < -60.0, "before the music: {} dB", level(0.05, 0.4));
    let full = level(0.8, 1.6);
    assert!(full > -30.0, "the music plays at {full} dB");
    // The fade takes the last 1.55 s: half of what's left after the music starts, at most two seconds.
    assert!((level(1.6, 1.9) - full).abs() < 1.0, "it fades too soon: {} dB", level(1.6, 1.9));
    assert!(level(2.9, 3.1) < full - 4.0, "it isn't fading: {} dB", level(2.9, 3.1));
    assert!(level(3.45, 3.55) < full - 15.0, "the end is {} dB", level(3.45, 3.55));
}

/// The exported sound as mono samples at 48 kHz.
fn samples(path: &Path) -> Vec<f32> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
        .output()
        .unwrap();
    out.stdout.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

/// The loudness from `from` to `to` seconds, in dB below full scale.
fn level_db(pcm: &[f32], from: f64, to: f64) -> f64 {
    let window = &pcm[(from * 48_000.0) as usize..((to * 48_000.0) as usize).min(pcm.len())];
    let power = window.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / window.len().max(1) as f64;
    10.0 * power.max(1e-12).log10()
}

/// When the exported sound is loudest, in seconds.
fn loudest_at(path: &Path) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
        .output()
        .unwrap();
    let loudest = out
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b).abs())
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(i, _)| i);
    loudest as f64 / 48_000.0
}

#[test]
fn saves_one_styled_moment_as_a_png() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let output = dir.path().join("Frame.png");
    // Past two seconds, the decoder starts near the moment instead of at the beginning.
    let options = ExportOptions {
        width: 1600,
        height: 900,
        fps: 30,
        format: Format::Png,
        bitrate: 0.0,
        output: output.clone(),
        at: Some(2.5),
    };
    export(&root, &options, &mut |_| true).unwrap();
    let png = std::fs::read(&output).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let side = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    assert_eq!((side(16), side(20)), (1600, 900));
    // The frame was drawn, not left empty: it decodes and has more than one colour.
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-vf", "scale=64:36", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .unwrap();
    assert_eq!(out.stdout.len(), 64 * 36);
    let (lo, hi) = out.stdout.iter().fold((u8::MAX, 0), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(hi - lo > 40, "the frame is flat: {lo}..{hi}");
    // Nothing else was left in the folder.
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(!names.iter().any(|n| n.to_string_lossy().contains("partial")), "{names:?}");
}

#[test]
fn exports_a_looping_gif() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let output = dir.path().join("Export test.gif");
    let options = ExportOptions {
        width: 640,
        height: 400,
        fps: 15,
        format: Format::Gif,
        bitrate: 0.0,
        output: output.clone(),
        at: None,
    };
    export(&root, &options, &mut |_| true).unwrap();
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..6], b"GIF89a");
    // 3.6 seconds at 15 fps.
    assert_eq!(probe(&output), Some((640, 400, 54)));
    if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
        std::fs::copy(&output, Path::new(&keep).join("export-test.gif")).unwrap();
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
    let options = ExportOptions {
        width: 640,
        height: 360,
        fps: 30,
        format: Format::H264,
        bitrate: 2e6,
        output: output.clone(),
        at: None,
    };
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

/// How far apart two RGBA frames are, seen at a quarter of their size so that a shade of difference along soft edges
/// (Core Image blends in linear light, the portable compositor in sRGB) counts for little and anything out of place
/// counts for a lot: the mean difference, and the share of pixels more than 24 levels apart.
fn difference(a: &[u8], b: &[u8], width: usize, height: usize) -> (f64, f64, Vec<u8>) {
    let (w, h) = (width / 4, height / 4);
    let mut diff = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut worst = 0u32;
            for c in 0..3 {
                let (mut sa, mut sb) = (0u32, 0u32);
                for dy in 0..4 {
                    for dx in 0..4 {
                        let i = ((y * 4 + dy) * width + x * 4 + dx) * 4 + c;
                        sa += u32::from(a[i]);
                        sb += u32::from(b[i]);
                    }
                }
                worst = worst.max(sa.abs_diff(sb) / 16);
            }
            diff[y * w + x] = worst.min(255) as u8;
        }
    }
    let mean = diff.iter().map(|&d| f64::from(d)).sum::<f64>() / diff.len() as f64;
    let far = diff.iter().filter(|&&d| d > 24).count() as f64 / diff.len() as f64;
    (mean, far, diff)
}

#[test]
fn the_portable_compositor_draws_what_core_image_draws() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path());
    let keep = std::env::var("SHOTMAXXING_KEEP_EXPORT").ok();
    // Still, with the blur, arrow, title and camera; zooming in, just after a click; zoomed in, with a keystroke.
    for at in [0.5, 1.6, 2.1] {
        let options = ExportOptions {
            width: 1600,
            height: 900,
            fps: 30,
            format: Format::Png,
            bitrate: 0.0,
            output: dir.path().join("unused.png"),
            at: Some(at),
        };
        let (core, soft) = shotmaxxing_render::draw_both_ways(&root, &options).unwrap();
        assert_eq!(core.len(), soft.len());
        let (mean, far, diff) = difference(&core, &soft, 1600, 900);
        eprintln!("at {at} s: mean difference {mean:.2}, {:.2}% of pixels far apart", far * 100.0);
        if let Some(keep) = &keep {
            let save = |name: &str, rgba: Vec<u8>| {
                let size = tiny_skia::IntSize::from_wh(1600, 900).unwrap();
                tiny_skia::Pixmap::from_vec(rgba, size)
                    .unwrap()
                    .save_png(Path::new(keep).join(name))
                    .unwrap();
            };
            save(&format!("core-{at}.png"), core);
            save(&format!("soft-{at}.png"), soft);
            let heat: Vec<u8> = diff.iter().flat_map(|&d| [d.saturating_mul(8), 0, 0, 255]).collect();
            let size = tiny_skia::IntSize::from_wh(400, 225).unwrap();
            let heat = tiny_skia::Pixmap::from_vec(heat, size).unwrap();
            heat.save_png(Path::new(keep).join(format!("diff-{at}.png"))).unwrap();
        }
        assert!(mean < 2.0, "at {at} s the frames differ by {mean:.2} on average");
        assert!(far < 0.005, "at {at} s {:.2}% of the frame is far apart", far * 100.0);
    }
}
