//! Exporting on Windows: Media Foundation reads the recording, the portable compositor draws each frame, and Media
//! Foundation encodes the video, on the graphics card where it can. GIFs and stills share the drawing. Sound comes
//! next; until then a Windows export is silent.

mod decode;
mod encode;
mod text;

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

use tiny_skia::Pixmap;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_FULL, MFStartup};
use windows::Win32::System::Com::CoIncrementMTAUsage;

use self::decode::Decoder;
use self::encode::VideoWriter;
use crate::model::RenderProject;
use crate::scene::{Layout, frame_at};
use crate::{ExportOptions, Format, RenderError, animated, soft};

fn system(error: windows::core::Error) -> RenderError {
    RenderError::System(error.message())
}

/// COM and Media Foundation, started once for the whole process and left running.
fn start() -> Result<(), RenderError> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| {
            // SAFETY: plain system calls. Joining the process-wide MTA lets every export thread use COM.
            unsafe {
                CoIncrementMTAUsage().map_err(|e| e.message())?;
                MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| e.message())
            }
        })
        .clone()
        .map_err(|e| RenderError::System(format!("Media Foundation couldn't start: {e}")))
}

/// Everything one export needs to draw its frames, in order.
struct Job<'a> {
    project: &'a RenderProject,
    layout: Layout,
    compositor: soft::Compositor<'a>,
    decoder: Decoder,
    /// The camera recording, when the bubble shows.
    camera: Option<Decoder>,
    fps: u32,
    /// Frames in the exported video.
    total: u64,
    /// Where the recording ends, for a last frame that rounding pushes past the final clip.
    last: f64,
}

impl<'a> Job<'a> {
    /// A job whose first frame is at `t` seconds into the recording: its decoders start just before it.
    fn starting(project: &'a RenderProject, options: &ExportOptions, t: f64) -> Result<Self, RenderError> {
        start()?;
        let screen = project
            .screen
            .as_ref()
            .ok_or_else(|| RenderError::Project("there's no recording to export".into()))?;
        let layout = Layout::new(project, options.width, options.height);
        let fps =
            if options.format == Format::Gif { options.fps.clamp(1, 50) } else { options.fps.clamp(1, 120) };
        Ok(Self {
            project,
            layout,
            compositor: soft::Compositor::new(project, layout, text::draw)?,
            decoder: Decoder::open_near(screen, t)?,
            camera: project.bubble.as_ref().map(|b| Decoder::open_near(&b.path, t - b.offset)).transpose()?,
            fps,
            total: (project.output_duration() * f64::from(fps) - 1e-6).ceil().max(1.0) as u64,
            last: project.clips.last().map_or(0.0, |c| (c.end - 1e-3).max(c.start)),
        })
    }

    /// The moment of the recording that frame `i` shows.
    fn time(&self, i: u64) -> f64 {
        self.project.source_time(i as f64 / f64::from(self.fps)).unwrap_or(self.last)
    }

    /// Draws frame `i`. Frames must be asked for in order.
    fn draw(&mut self, i: u64) -> Result<&Pixmap, RenderError> {
        let t = self.time(i);
        self.draw_at(t)
    }

    /// Draws the moment `t` seconds into the recording. Moments must be asked for in order.
    fn draw_at(&mut self, t: f64) -> Result<&Pixmap, RenderError> {
        let screen = self
            .decoder
            .frame_at(t)?
            .ok_or_else(|| RenderError::Project("the recording has no frames".into()))?;
        let camera = match (&mut self.camera, &self.project.bubble) {
            (Some(decoder), Some(bubble)) => decoder.frame_at(t - bubble.offset)?,
            _ => None,
        };
        let frame = frame_at(self.project, &self.layout, t);
        Ok(self.compositor.compose(&frame, screen.as_ref(), camera.map(Pixmap::as_ref)))
    }
}

pub fn export(
    project: &RenderProject,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<(), RenderError> {
    let temp = crate::partial(&options.output);
    let started = std::time::Instant::now();
    let result = match options.format {
        Format::Gif => export_gif(project, options, &temp, progress),
        Format::H264 | Format::Hevc => export_video(project, options, &temp, progress),
        Format::Png => export_still(project, options, &temp),
    };
    match result {
        Ok(frames) => {
            std::fs::rename(&temp, &options.output)?;
            let seconds = started.elapsed().as_secs_f64();
            tracing::info!(frames, seconds, "exported {}×{}", options.width, options.height);
            progress(1.0);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

fn export_video(
    project: &RenderProject,
    options: &ExportOptions,
    temp: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<u64, RenderError> {
    let mut job = Job::starting(project, options, 0.0)?;
    let bitrate = if options.bitrate > 0.0 {
        options.bitrate
    } else {
        crate::default_bitrate(options.width, options.height, job.fps, options.format)
    };
    let mut writer = VideoWriter::new(temp, options.width, options.height, job.fps, options.format, bitrate)?;
    for i in 0..job.total {
        if !progress(i as f64 / job.total as f64) {
            return Err(RenderError::Cancelled);
        }
        writer.write(job.draw(i)?)?;
    }
    writer.finish()?;
    Ok(job.total)
}

/// One moment, styled like the video, as a PNG.
fn export_still(project: &RenderProject, options: &ExportOptions, temp: &Path) -> Result<u64, RenderError> {
    let t = project.kept_time(options.at.unwrap_or(0.0).max(0.0));
    let mut job = Job::starting(project, options, t)?;
    let png = job
        .draw_at(t)?
        .encode_png()
        .map_err(|e| RenderError::System(format!("couldn't make the PNG: {e}")))?;
    std::fs::write(temp, png)?;
    Ok(1)
}

fn export_gif(
    project: &RenderProject,
    options: &ExportOptions,
    temp: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<u64, RenderError> {
    let (width, height) = (options.width as usize, options.height as usize);
    if width > usize::from(u16::MAX) || height > usize::from(u16::MAX) {
        return Err(RenderError::Project("that's too large for a GIF".into()));
    }

    // A first, quick pass over a few frames finds the colours the whole clip needs.
    let mut sampler = Job::starting(project, options, 0.0)?;
    let count = animated::PALETTE_SAMPLES.min(sampler.total);
    let mut samples = Vec::new();
    for k in 0..count {
        if !progress(0.08 * k as f64 / count as f64) {
            return Err(RenderError::Cancelled);
        }
        let i = k * sampler.total / count;
        // Every other row is plenty to learn colours from.
        for row in sampler.draw(i)?.data().chunks_exact(width * 4).step_by(2) {
            samples.extend_from_slice(row);
        }
    }
    drop(sampler);

    let mut job = Job::starting(project, options, 0.0)?;
    let file = std::io::BufWriter::new(std::fs::File::create(temp)?);
    let mut writer = animated::GifWriter::new(file, width as u16, height as u16, job.fps, &samples)?;
    drop(samples);
    for i in 0..job.total {
        if !progress(0.08 + 0.92 * i as f64 / job.total as f64) {
            return Err(RenderError::Cancelled);
        }
        writer.write(job.draw(i)?.data())?;
    }
    let mut file = writer.finish()?;
    file.flush()?;
    Ok(job.total)
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::bundle::ProjectBundle;
    use shotmaxxing_core::geometry::Size;
    use shotmaxxing_core::project::{MediaTrack, Project, TrackKind};
    use tiny_skia::{Color, Paint, Rect, Transform};

    use super::*;

    /// Records `frames` frames of a light square gliding across a dark picture, as a stand-in for a screen
    /// recording, with the exporter's own writer.
    fn record(path: &Path, width: u32, height: u32, frames: u64) -> Result<(), RenderError> {
        start()?;
        let mut writer = VideoWriter::new(path, width, height, 30, Format::H264, 2e6)?;
        let mut frame = Pixmap::new(width, height).unwrap();
        let mut light = Paint::default();
        light.set_color(Color::from_rgba8(240, 240, 240, 255));
        for i in 0..frames {
            frame.fill(Color::from_rgba8(20, 24, 32, 255));
            let square = Rect::from_xywh(40.0 + 6.0 * i as f32, 100.0, 120.0, 120.0).unwrap();
            frame.fill_rect(square, &light, Transform::identity(), None);
            writer.write(&frame)?;
        }
        writer.finish()
    }

    /// How many pictures a video holds, and their size.
    fn frames_in(path: &Path) -> (u64, (u32, u32)) {
        let mut decoder = Decoder::open(path).unwrap();
        let size = decoder.size();
        (decoder.count().unwrap(), size)
    }

    #[test]
    fn exports_video_gifs_and_stills() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        if let Err(error) = record(&source, 640, 400, 60) {
            eprintln!("skipped: this computer can't encode video: {error}");
            return;
        }

        // The recording reads back at its size, with the square where it was drawn.
        let mut decoder = Decoder::open(&source).unwrap();
        let first = decoder.frame_at(0.0).unwrap().unwrap();
        assert_eq!((first.width(), first.height()), (640, 400));
        let pixel = |p: &Pixmap, x: u32, y: u32| p.pixel(x, y).unwrap().demultiply();
        assert!(pixel(first, 100, 160).red() > 200, "the square is missing");
        assert!(pixel(first, 400, 300).red() < 60, "the background is too light");

        let mut project = Project { name: "Windows export".into(), ..Project::default() };
        project.source.size = Size::new(640.0, 400.0);
        project.source.scale = 1.0;
        project.tracks = vec![MediaTrack {
            id: "screen".into(),
            kind: TrackKind::Screen,
            path: "media/screen.mp4".into(),
            duration_us: 2_000_000,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        }];
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        std::fs::copy(&source, bundle.media_dir().join("screen.mp4")).unwrap();
        bundle.save(&mut project).unwrap();
        let root = bundle.root().to_path_buf();
        let options = |format: Format, name: &str| ExportOptions {
            width: 1280,
            height: 720,
            fps: 30,
            format,
            bitrate: 0.0,
            output: dir.path().join(name),
            at: Some(1.0),
        };

        let video = options(Format::H264, "Export.mp4");
        crate::export(&root, &video, &mut |_| true).unwrap();
        let (count, size) = frames_in(&video.output);
        assert_eq!(size, (1280, 720));
        assert!((59..=61).contains(&count), "{count} frames");

        let gif = ExportOptions { width: 320, height: 180, fps: 10, ..options(Format::Gif, "Export.gif") };
        crate::export(&root, &gif, &mut |_| true).unwrap();
        assert_eq!(&std::fs::read(&gif.output).unwrap()[..6], b"GIF89a");

        let still = options(Format::Png, "Frame.png");
        crate::export(&root, &still, &mut |_| true).unwrap();
        let png = Pixmap::load_png(&still.output).unwrap();
        assert_eq!((png.width(), png.height()), (1280, 720));

        // Nothing but the project and the three files is left.
        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.ends_with(".smx"))
            .collect();
        names.sort();
        assert_eq!(names, ["Export.gif", "Export.mp4", "Frame.png", "source.mp4"]);
    }
}
