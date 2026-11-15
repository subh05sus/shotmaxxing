//! Screenshots, drawn the way a recording's frame is: the wallpaper, the padding, the rounded corners and shadow,
//! and every title, arrow and blur. One CPU path draws them on every system, so the copy, the saved image and the
//! editor's preview match. A window's own rounded corners are clear in its picture, so the wallpaper shows through
//! them.

use std::path::Path;

use tiny_skia::Pixmap;

use crate::model::RenderProject;
use crate::scene::{Layout, frame_at, place_window};
use crate::{ExportOptions, RenderError, Still, soft};

/// The largest side a screenshot is drawn at.
const LARGEST: f64 = 7680.0;

fn picture(project: &RenderProject) -> Result<Pixmap, RenderError> {
    let path = project
        .screen
        .as_deref()
        .ok_or_else(|| RenderError::Project("this screenshot's picture is missing".into()))?;
    Pixmap::load_png(path).map_err(|e| RenderError::Project(format!("couldn't read the screenshot: {e}")))
}

/// The size that shows a picture `width` pixels wide pixel for pixel: it fills the window exactly, and the canvas
/// keeps its shape and padding around it. At most 7680 pixels on a side, and never under 16.
pub fn native_size(project: &RenderProject, width: u32) -> (u32, u32) {
    let capture = project.capture;
    let aspect = project.aspect.unwrap_or(capture.width / capture.height.max(1.0)).clamp(0.1, 10.0);
    let (w, h) = if aspect >= 1.0 { (1000.0 * aspect, 1000.0) } else { (1000.0, 1000.0 / aspect) };
    let window = place_window(w, h, capture, project.padding);
    let mut scale = f64::from(width.max(1)) / window.width.max(1.0);
    scale = scale.min(LARGEST / w.max(h));
    ((w * scale).round().max(16.0) as u32, (h * scale).round().max(16.0) as u32)
}

/// Draws the screenshot project at `root` at `size`, or at its own size when `size` is `None`.
pub fn draw_at(root: &Path, size: Option<(u32, u32)>) -> Result<Still, RenderError> {
    let project = RenderProject::load(root)?;
    if !project.still {
        return Err(RenderError::Project("that project is a recording, not a screenshot".into()));
    }
    let picture = picture(&project)?;
    let (width, height) = size.unwrap_or_else(|| native_size(&project, picture.width()));
    draw_picture(&project, &picture, width, height)
}

fn draw_picture(
    project: &RenderProject,
    picture: &Pixmap,
    width: u32,
    height: u32,
) -> Result<Still, RenderError> {
    if width < 16 || height < 16 || f64::from(width.max(height)) > LARGEST {
        return Err(RenderError::Project(format!(
            "{width} × {height} isn't a size a screenshot can be drawn at"
        )));
    }
    let layout = Layout::new(project, width, height);
    let frame = frame_at(project, &layout, 0.0);
    let mut compositor = soft::Compositor::new(project, layout, crate::draw_text)?;
    let drawn = compositor.compose(&frame, picture.as_ref(), None);
    let png = drawn.encode_png().map_err(|e| RenderError::System(format!("couldn't make the PNG: {e}")))?;
    let mut rgba = Vec::with_capacity(drawn.data().len());
    for pixel in drawn.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(Still { width, height, rgba, png })
}

/// Exports a screenshot project as a PNG at the export's size, or pixel for pixel at 0 × 0. Nothing is left behind
/// if it fails.
pub(crate) fn export(project: &RenderProject, options: &ExportOptions) -> Result<(), RenderError> {
    let picture = picture(project)?;
    let (width, height) = if options.native() {
        native_size(project, picture.width())
    } else {
        (options.width, options.height)
    };
    let still = draw_picture(project, &picture, width, height)?;
    let temp = crate::partial(&options.output);
    if let Err(error) =
        std::fs::write(&temp, &still.png).and_then(|()| std::fs::rename(&temp, &options.output))
    {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::bundle::ProjectBundle;
    use shotmaxxing_core::geometry::{Point, Size};
    use shotmaxxing_core::project::{
        ArrowOverlay, MediaTrack, Project, ProjectKind, TextOverlay, TimeRange, TrackKind,
    };
    use tiny_skia::{Color, Paint, Transform};

    use super::*;
    use crate::{Format, draw_screenshot};

    /// A screenshot project: a 400 × 250 picture (200 × 125 points at 2x), white with a red square in the middle,
    /// and a title and an arrow placed for the first second only.
    fn project(dir: &Path) -> std::path::PathBuf {
        let mut picture = Pixmap::new(400, 250).unwrap();
        picture.fill(Color::WHITE);
        let mut red = Paint::default();
        red.set_color_rgba8(220, 30, 30, 255);
        let square = tiny_skia::Rect::from_xywh(150.0, 75.0, 100.0, 100.0).unwrap();
        picture.fill_rect(square, &red, Transform::identity(), None);
        let mut project =
            Project { name: "Shot".into(), kind: ProjectKind::Screenshot, ..Project::default() };
        project.source.size = Size::new(200.0, 125.0);
        project.source.scale = 2.0;
        project.canvas.padding = 0.1;
        let first = TimeRange { start_us: 0, end_us: 1_000_000 };
        project.edits.texts.push(TextOverlay {
            id: "t".into(),
            range: first,
            text: "Look here".into(),
            position: Point::new(0.1, 0.1),
            style: String::new(),
        });
        project.edits.arrows.push(ArrowOverlay {
            id: "a".into(),
            range: first,
            from: Point::new(20.0, 20.0),
            to: Point::new(90.0, 55.0),
            color: None,
        });
        let bundle = ProjectBundle::create(dir, &mut project).unwrap();
        std::fs::create_dir_all(bundle.root().join("media")).unwrap();
        picture.save_png(bundle.root().join("media/screenshot.png")).unwrap();
        project.tracks = vec![MediaTrack {
            id: "screen".into(),
            kind: TrackKind::Screen,
            path: "media/screenshot.png".into(),
            duration_us: 0,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        }];
        bundle.save(&mut project).unwrap();
        bundle.root().to_path_buf()
    }

    #[test]
    fn a_screenshot_is_framed_pixel_for_pixel_with_its_effects() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(dir.path());
        let loaded = RenderProject::load(&root).unwrap();
        assert!(loaded.still && loaded.zooms.is_empty() && !loaded.cursor_visible);
        // Effects placed for one second show all the same: a still is one moment.
        assert!(loaded.titles[0].start.is_infinite() && loaded.arrows[0].end.is_infinite());

        let still = draw_screenshot(&root, None).unwrap();
        let layout = Layout::new(&loaded, still.width, still.height);
        assert!((layout.window.width - 400.0).abs() < 1.0, "the picture isn't scaled: {:?}", layout.window);
        assert_eq!(still.rgba.len(), still.width as usize * still.height as usize * 4);
        assert_eq!(&still.png[1..4], b"PNG");
        if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
            std::fs::write(Path::new(&keep).join("screenshot-test.png"), &still.png).unwrap();
        }

        // The red square sits in the middle of the window, untouched.
        let at = |x: f64, y: f64| {
            let i = (y as usize * still.width as usize + x as usize) * 4;
            [still.rgba[i], still.rgba[i + 1], still.rgba[i + 2]]
        };
        let (cx, cy) = (layout.window.x + 200.0, layout.window.y + 125.0);
        assert_eq!(at(cx, cy), [220, 30, 30]);
        // The wallpaper fills the padding, and the arrow is drawn over the picture.
        assert_ne!(at(2.0, 2.0), [255, 255, 255]);
        let frame = frame_at(&loaded, &layout, 0.0);
        assert_eq!((frame.titles.len(), frame.arrows.len()), (1, 1));

        // An export at another size draws the same picture smaller; a video can't be made of it.
        let out = dir.path().join("Shot.png");
        let options = ExportOptions {
            width: 640,
            height: 400,
            fps: 1,
            format: Format::Png,
            bitrate: 0.0,
            output: out.clone(),
            at: None,
        };
        crate::export(&root, &options, &mut |_| true).unwrap();
        assert_eq!(Pixmap::load_png(&out).unwrap().width(), 640);
        // At 0 × 0 it's drawn pixel for pixel, as it's copied.
        let native =
            ExportOptions { width: 0, height: 0, output: dir.path().join("Native.png"), ..options.clone() };
        crate::export(&root, &native, &mut |_| true).unwrap();
        assert_eq!(Pixmap::load_png(&native.output).unwrap().width(), still.width);
        assert_eq!(crate::png_size(&native.output), Some((still.width, still.height)));
        assert_eq!(crate::png_size(&dir.path().join("Missing.png")), None);
        let video = ExportOptions { format: Format::H264, output: dir.path().join("Shot.mp4"), ..options };
        let refused = crate::export(&root, &video, &mut |_| true).unwrap_err().to_string();
        assert!(refused.contains("PNG"), "{refused}");
    }

    #[test]
    fn native_sizes_keep_the_canvas_shape_and_stay_in_bounds() {
        let mut project = RenderProject::from_parts(Path::new("/nowhere"), &Project::default(), &[], &[]);
        project.capture = Size::new(1440.0, 900.0);
        project.padding = 0.08;
        project.aspect = Some(16.0 / 9.0);
        let (w, h) = native_size(&project, 2880);
        assert!(((f64::from(w) / f64::from(h)) - 16.0 / 9.0).abs() < 0.01, "{w} × {h}");
        let layout = Layout::new(&project, w, h);
        assert!((layout.window.width - 2880.0).abs() < 2.0 || layout.window.height >= 1799.0, "{layout:?}");
        // A huge picture is drawn smaller rather than past the largest size.
        let (w, h) = native_size(&project, 20_000);
        assert!(w <= 7680 && h <= 7680, "{w} × {h}");
    }
}
