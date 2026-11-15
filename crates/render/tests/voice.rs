//! The voice tools on a generated take: a still screen and a "voice" that speaks for half a second every two
//! seconds over a quiet room. Skipped when ffmpeg isn't installed.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::Size;
use shotmaxxing_core::project::{MediaTrack, Project, TrackKind};
use shotmaxxing_render::{CLEAN_MIC, clean_voice, find_pauses};

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
}

fn ffmpeg(args: &[&str], out: &Path) {
    let status = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).arg(out).status().unwrap();
    assert!(status.success(), "ffmpeg couldn't make {}", out.display());
}

fn track(kind: TrackKind, path: &str) -> MediaTrack {
    MediaTrack {
        id: path.into(),
        kind,
        path: path.into(),
        duration_us: 6_000_000,
        offset_us: 0,
        volume: 1.0,
        muted: false,
        name: None,
    }
}

fn make_take(dir: &Path) -> PathBuf {
    let mut project = Project { name: "Voice test".into(), ..Project::default() };
    project.source.size = Size::new(1440.0, 900.0);
    project.source.scale = 1.0;
    project.tracks = vec![
        track(TrackKind::Screen, "media/screen.mp4"),
        track(TrackKind::Microphone, "media/microphone.m4a"),
    ];
    let bundle = ProjectBundle::create(dir, &mut project).unwrap();
    let media = bundle.media_dir();
    // A still screen stores almost no frames, like a real recording of a screen where nothing moves.
    ffmpeg(
        &["-f", "lavfi", "-i", "color=c=0x334455:size=1440x900:rate=30:duration=6", "-vf", "mpdecimate"],
        &media.join("tmp.mp4"),
    );
    ffmpeg(
        &[
            "-i",
            media.join("tmp.mp4").to_str().unwrap(),
            "-fps_mode",
            "vfr",
            "-c:v",
            "hevc_videotoolbox",
            "-tag:v",
            "hvc1",
        ],
        &media.join("screen.mp4"),
    );
    std::fs::remove_file(media.join("tmp.mp4")).unwrap();
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=color=pink:amplitude=0.5:duration=6:sample_rate=48000",
            "-af",
            "volume='if(lt(mod(t,2),0.5),1,0.03)':eval=frame",
            "-c:a",
            "aac",
        ],
        &media.join("microphone.m4a"),
    );
    bundle.save(&mut project).unwrap();
    bundle.root().to_path_buf()
}

fn level(path: &Path, start: f64, length: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-ss", &start.to_string(), "-t", &length.to_string(), "-i"])
        .arg(path)
        .args(["-af", "volumedetect", "-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stderr);
    log.lines()
        .find_map(|l| l.split("mean_volume:").nth(1))
        .and_then(|rest| rest.trim().trim_end_matches("dB").trim().parse().ok())
        .unwrap_or(f64::NAN)
}

/// The samples a player actually plays, as Core Audio counts them: AAC's encoder priming and end padding left out.
fn samples(path: &Path) -> u64 {
    let out = Command::new("afinfo").arg(path).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    // The line reads "audio 288000 valid frames + 2112 priming + 704 remainder = 290816".
    text.lines()
        .filter(|l| l.contains(" valid frames"))
        .find_map(|l| l.trim().strip_prefix("audio ")?.split(' ').next()?.parse().ok())
        .unwrap_or(0)
}

#[test]
fn long_pauses_are_found_and_the_voice_is_cleaned() {
    if !has_ffmpeg() {
        eprintln!("skipped: ffmpeg isn't installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = make_take(dir.path());

    // Each 1.5 s gap loses all but a breath at each end.
    let cuts = find_pauses(&root).unwrap();
    assert_eq!(cuts.len(), 3, "{cuts:?}");
    for (cut, expected) in cuts.iter().zip([(0.8, 1.7), (2.8, 3.7), (4.8, 5.8)]) {
        assert!(
            (cut.0 - expected.0).abs() < 0.08 && (cut.1 - expected.1).abs() < 0.08,
            "{cut:?} vs {expected:?}"
        );
    }

    let mut last = 0.0;
    let cleaned = clean_voice(&root, &mut |p| {
        assert!(p >= last);
        last = p;
        true
    })
    .unwrap();
    assert_eq!(cleaned, root.join(CLEAN_MIC));
    assert_eq!(last, 1.0);
    // Same length, so it stays in sync with the picture.
    let original = root.join("media/microphone.m4a");
    assert_eq!(samples(&cleaned), samples(&original));
    assert!(samples(&original) > 0);
    // The room between words is clearly quieter, while the voice stays strong.
    let (room_before, room_after) = (level(&original, 1.0, 0.8), level(&cleaned, 1.0, 0.8));
    assert!(room_after < room_before - 6.0, "room {room_before} -> {room_after} dB");
    let (voice_before, voice_after) = (level(&original, 2.05, 0.4), level(&cleaned, 2.05, 0.4));
    assert!(voice_after > voice_before - 6.0, "voice {voice_before} -> {voice_after} dB");
    if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
        let copied = Command::new("cp").arg("-R").arg(&root).arg(&keep).status().unwrap();
        assert!(copied.success());
    }
}
