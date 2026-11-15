//! What to draw at each moment. This is the editor preview's layout and maths (Canvas.svelte and camera.ts in the
//! desktop app) in Rust, so a frame of an export matches the same moment in the preview.
//!
//! Frame coordinates are output pixels with the origin at the top left. The camera layer (wallpaper, window,
//! cursor) moves with the zoom; titles and the keystroke overlay sit on top of it and stay put.

use shotmaxxing_core::cursor::position_at;
use shotmaxxing_core::geometry::{Point, Rect, Size};

use crate::model::{Click, RenderProject, Zoom};

/// Zooms closer together than this glide from one to the next instead of zooming out in between.
const GLIDE_GAP: f64 = 1.4;
/// How long a click's ring or press lasts.
const CLICK_S: f64 = 0.5;
/// The cursor hides after resting this long, when the project asks for it.
const IDLE_S: f64 = 2.5;
/// How long a keystroke stays on screen.
const KEY_S: f64 = 0.9;
/// The cursor's drawn width at size 1, in capture points: about the real cursor's size.
pub const CURSOR_POINTS: f64 = 20.0;
/// How long the cursor smears at full motion blur. The editor's switch uses 0.3 of it: 15 ms, which only shows on
/// fast moves.
pub const MOTION_BLUR_S: f64 = 0.05;

/// Sizes that stay the same for a whole export.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    /// Where the recording sits in the frame.
    pub window: Rect,
    /// Frame pixels per capture point.
    pub k: f64,
    pub radius: f64,
    /// The cursor's drawn width.
    pub cursor_px: f64,
    pub shadow_offset: f64,
    pub shadow_sigma: f64,
    pub shadow_alpha: f64,
    pub blur_sigma: f64,
    pub blur_radius: f64,
    /// Font size of titles and the keystroke overlay.
    pub text_px: f64,
}

/// Where the window sits in a `width` × `height` frame: centred and padded, keeping the capture's shape.
pub fn place_window(width: f64, height: f64, capture: Size, padding: f64) -> Rect {
    let pad = width.min(height) * padding;
    let (avail_w, avail_h) = (width - pad * 2.0, height - pad * 2.0);
    let aspect = capture.width / capture.height.max(1.0);
    let (mut w, mut h) = (avail_w, avail_w / aspect);
    if h > avail_h {
        h = avail_h;
        w = h * aspect;
    }
    Rect { x: (width - w) / 2.0, y: (height - h) / 2.0, width: w, height: h }
}

impl Layout {
    pub fn new(project: &RenderProject, width: u32, height: u32) -> Self {
        let (fw, fh) = (f64::from(width), f64::from(height));
        let window = place_window(fw, fh, project.capture, project.padding);
        let k = window.width / project.capture.width.max(1.0);
        Self {
            width: fw,
            height: fh,
            window,
            k,
            radius: project.radius * fh / 1080.0,
            cursor_px: CURSOR_POINTS * project.cursor_size * k,
            shadow_offset: fh * 0.02,
            // CSS box-shadow blur is twice the Gaussian's standard deviation.
            shadow_sigma: fh * 0.03,
            shadow_alpha: project.shadow * 0.55,
            blur_sigma: fh * 0.016,
            blur_radius: fh * 0.012,
            text_px: fh * 0.035,
        }
    }

    /// A rectangle in capture points, in frame pixels inside the camera layer.
    pub fn to_frame(&self, r: Rect) -> Rect {
        Rect {
            x: self.window.x + r.x * self.k,
            y: self.window.y + r.y * self.k,
            width: r.width * self.k,
            height: r.height * self.k,
        }
    }

    pub fn point_to_frame(&self, p: Point) -> Point {
        Point::new(self.window.x + p.x * self.k, self.window.y + p.y * self.k)
    }
}

/// The camera in capture points: how far it is zoomed and what it looks at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}

fn smootherstep(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn clamp_camera(c: Camera, w: f64, h: f64) -> Camera {
    let hw = w / (2.0 * c.scale);
    let hh = h / (2.0 * c.scale);
    Camera {
        scale: c.scale,
        x: c.x.clamp(hw.min(w - hw), hw.max(w - hw)),
        y: c.y.clamp(hh.min(h - hh), hh.max(h - hh)),
    }
}

/// When the camera glides from zoom `z` on to the next, `n`: through the gap between them, and over at least one
/// ramp's time, taken half from each when the gap is shorter (zooms that touch would jump from one to the other),
/// but never past either zoom's middle.
fn glide(z: &Zoom, n: &Zoom, ramp: f64) -> (f64, f64) {
    let mid = (z.end + n.start) / 2.0;
    let half = (n.start - z.end).max(ramp) / 2.0;
    let from = (mid - half).max((z.start + z.end) / 2.0);
    let to = (mid + half).min((n.start + n.end) / 2.0);
    (from, to.max(from))
}

/// Blends two cameras, the scale in log space so a zoom feels even from start to end.
fn blend(from: Camera, to: Camera, u: f64) -> Camera {
    Camera {
        scale: lerp(from.scale.ln(), to.scale.ln(), u).exp(),
        x: lerp(from.x, to.x, u),
        y: lerp(from.y, to.y, u),
    }
}

/// The camera at `t` seconds. `zooms` must be sorted by start. `cursor` is used when the camera follows it.
pub fn camera_at(t: f64, zooms: &[Zoom], capture: Size, instant: bool, cursor: Option<Point>) -> Camera {
    let (w, h) = (capture.width, capture.height);
    let ramp = if instant { 0.001 } else { 0.7 };
    let wide = Camera { scale: 1.0, x: w / 2.0, y: h / 2.0 };
    let target = |z: &Zoom| {
        let (mut x, mut y) = (z.focus.x, z.focus.y);
        if let Some(c) = cursor {
            x = lerp(x, c.x, 0.35);
            y = lerp(y, c.y, 0.35);
        }
        Camera { scale: z.scale.max(1e-6), x, y }
    };
    for (i, z) in zooms.iter().enumerate() {
        let before = i.checked_sub(1).map(|j| &zooms[j]).filter(|p| z.start - p.end < GLIDE_GAP);
        let next = zooms.get(i + 1).filter(|n| n.start - z.end < GLIDE_GAP);
        let after = next.map(|n| glide(z, n, ramp));
        if let (Some(n), Some((from, to))) = (next, after)
            && t > from
            && t < to
        {
            return clamp_camera(blend(target(z), target(n), smootherstep((t - from) / (to - from))), w, h);
        }
        // This zoom's own time: from where the glide into it ends to where the glide out of it starts.
        let start = before.map_or(z.start, |p| glide(p, z, ramp).1);
        let end = after.map_or(z.end, |(from, _)| from);
        if t < start || t > end {
            continue;
        }
        let r = f64::min(ramp, (z.end - z.start) / 2.0).max(1e-6);
        let weight = f64::min(
            if before.is_some() { 1.0 } else { smootherstep((t - z.start) / r) },
            if after.is_some() { 1.0 } else { smootherstep((z.end - t) / r) },
        );
        return clamp_camera(blend(wide, target(z), weight), w, h);
    }
    wide
}

/// How the camera layer maps onto the frame: a point `p` lands at `(tx + scale·p.x, ty + scale·p.y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorDraw {
    /// The hotspot, in the camera layer.
    pub at: Point,
    /// 0.85 while a press squashes the cursor, else 1.
    pub press: f64,
    /// How far a click ring has grown, 0 to 1.
    pub ripple: Option<f64>,
    /// Where the hotspot was when the motion-blur shutter opened, when the cursor moved since.
    pub from: Option<Point>,
}

/// Everything that changes from frame to frame. Effects are indexes into the project's lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub view: View,
    pub cursor: Option<CursorDraw>,
    pub blurs: Vec<usize>,
    pub arrows: Vec<usize>,
    pub titles: Vec<usize>,
    pub key: Option<usize>,
    /// The caption showing, and how far through its time it is, from 0 to 1 (a long one shows in pages).
    pub caption: Option<(usize, f64)>,
}

/// The frame showing the recording at `t` seconds.
pub fn frame_at(project: &RenderProject, layout: &Layout, t: f64) -> Frame {
    let cursor = position_at(&project.path, t * 1_000_000.0);
    let follow = if project.follow_cursor { cursor } else { None };
    let cam = camera_at(t, &project.zooms, project.capture, project.instant_zoom, follow);

    let (fw, fh, s) = (layout.width, layout.height, cam.scale);
    let focus_x = layout.window.x + cam.x * layout.k;
    let focus_y = layout.window.y + cam.y * layout.k;
    let (lo_x, hi_x) = (fw / (2.0 * s), fw - fw / (2.0 * s));
    let (lo_y, hi_y) = (fh / (2.0 * s), fh - fh / (2.0 * s));
    let view_x = focus_x.clamp(lo_x.min(hi_x), hi_x.max(lo_x));
    let view_y = focus_y.clamp(lo_y.min(hi_y), hi_y.max(lo_y));
    let view = View { scale: s, tx: fw / 2.0 - view_x * s, ty: fh / 2.0 - view_y * s };

    let cursor = cursor.filter(|_| project.cursor_visible).and_then(|p| {
        let last_sample = project
            .path
            .iter()
            .rev()
            .find(|c| c.t_us as f64 / 1_000_000.0 <= t)
            .map_or(f64::NEG_INFINITY, |c| c.t_us as f64 / 1_000_000.0);
        let last_click =
            project.clicks.iter().rev().find(|(ct, _)| *ct <= t).map_or(f64::NEG_INFINITY, |c| c.0);
        let idle = project.hide_idle && t - last_sample.max(last_click) > IDLE_S && t > IDLE_S;
        if idle {
            return None;
        }
        let click = project.clicks.iter().find(|(ct, _)| t >= *ct && t < ct + CLICK_S);
        let age = click.map(|(ct, _)| (t - ct) / CLICK_S);
        let shutter = project.motion_blur * MOTION_BLUR_S;
        let from = (shutter > 0.0)
            .then(|| position_at(&project.path, (t - shutter).max(0.0) * 1_000_000.0))
            .flatten()
            .filter(|q| q.distance(p) >= 1.0)
            .map(|q| layout.point_to_frame(q));
        Some(CursorDraw {
            at: layout.point_to_frame(p),
            press: if project.click == Click::Press && age.is_some_and(|a| a < 0.3) { 0.85 } else { 1.0 },
            ripple: if project.click == Click::Ripple { age } else { None },
            from,
        })
    });

    let active = |start: f64, end: f64| t >= start && t <= end;
    let caption = if project.show_captions {
        // A voice-over's captions are timed on the finished video.
        let out = if project.captions.iter().any(|c| c.finished) { project.export_time(t) } else { None };
        project.captions.iter().enumerate().rev().find_map(|(i, c)| {
            let at = if c.finished { out? } else { t };
            (at >= c.start && at < c.end).then(|| (i, ((at - c.start) / (c.end - c.start)).clamp(0.0, 1.0)))
        })
    } else {
        None
    };
    Frame {
        view,
        cursor,
        blurs: (0..project.blurs.len())
            .filter(|&i| active(project.blurs[i].start, project.blurs[i].end))
            .collect(),
        arrows: (0..project.arrows.len())
            .filter(|&i| active(project.arrows[i].start, project.arrows[i].end))
            .collect(),
        titles: (0..project.titles.len())
            .filter(|&i| active(project.titles[i].start, project.titles[i].end))
            .collect(),
        key: if project.keystrokes {
            project.keys.iter().rposition(|(kt, _)| t >= *kt && t < kt + KEY_S)
        } else {
            None
        },
        caption,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE: Size = Size::new(1600.0, 1000.0);

    fn zoom(start: f64, end: f64, scale: f64, x: f64, y: f64) -> Zoom {
        Zoom { start, end, scale, focus: Point::new(x, y) }
    }

    #[test]
    fn window_keeps_its_shape_inside_the_padding() {
        let r = place_window(1920.0, 1080.0, CAPTURE, 0.08);
        assert!((r.width / r.height - 1.6).abs() < 1e-9);
        assert!((r.y - 1080.0 * 0.08).abs() < 1e-9);
        assert!((r.x + r.width / 2.0 - 960.0).abs() < 1e-9);
    }

    #[test]
    fn camera_eases_in_holds_and_eases_out() {
        let zooms = [zoom(1.0, 4.0, 2.0, 400.0, 300.0)];
        assert_eq!(camera_at(0.5, &zooms, CAPTURE, false, None).scale, 1.0);
        let held = camera_at(2.5, &zooms, CAPTURE, false, None);
        assert!((held.scale - 2.0).abs() < 1e-9);
        // The focus is pulled inwards so the view stays inside the capture.
        assert_eq!((held.x, held.y), (400.0, 300.0));
        let easing = camera_at(1.35, &zooms, CAPTURE, false, None).scale;
        assert!(easing > 1.0 && easing < 2.0);
    }

    #[test]
    fn close_zooms_glide_without_zooming_out() {
        let zooms = [zoom(1.0, 3.0, 2.0, 400.0, 300.0), zoom(3.5, 6.0, 3.0, 1200.0, 700.0)];
        let between = camera_at(3.25, &zooms, CAPTURE, false, None);
        assert!(between.scale > 2.0 && between.scale < 3.0, "{between:?}");
    }

    #[test]
    fn the_camera_never_jumps() {
        // Zooms that touch (auto zoom trims overlaps so), nearly touch, overlap, sit apart, and a short one alone.
        let cases = [
            vec![zoom(1.0, 3.0, 2.0, 400.0, 300.0), zoom(3.0, 5.0, 3.0, 1200.0, 700.0)],
            vec![zoom(1.0, 3.0, 2.0, 400.0, 300.0), zoom(3.1, 5.0, 1.5, 1300.0, 200.0)],
            vec![zoom(1.0, 3.4, 2.0, 400.0, 300.0), zoom(3.0, 5.0, 2.5, 1200.0, 700.0)],
            vec![zoom(1.0, 3.0, 2.0, 400.0, 300.0), zoom(6.0, 8.0, 2.0, 1200.0, 700.0)],
            vec![zoom(1.0, 1.6, 2.4, 300.0, 200.0)],
        ];
        for zooms in &cases {
            let mut last = camera_at(0.0, zooms, CAPTURE, false, None);
            for step in 1..=9_000 {
                let t = f64::from(step) / 1_000.0;
                let now = camera_at(t, zooms, CAPTURE, false, None);
                let (scale, moved) =
                    ((now.scale.ln() - last.scale.ln()).abs(), (now.x - last.x).hypot(now.y - last.y));
                assert!(scale < 0.01 && moved < 6.0, "{t}s: {last:?} to {now:?} in {zooms:?}");
                last = now;
            }
        }
    }

    #[test]
    fn a_frame_shows_the_caption_of_its_moment_on_its_timeline() {
        use crate::model::{CaptionLine, Clip};
        let mut project = RenderProject::from_parts(
            std::path::Path::new("/tmp"),
            &shotmaxxing_core::project::Project::default(),
            &[],
            &[],
        );
        project.capture = CAPTURE;
        project.duration = 10.0;
        // Two seconds cut from 2 s: the recording's 5 s plays at 3 s in the finished video.
        project.clips =
            vec![Clip { start: 0.0, end: 2.0, speed: 1.0 }, Clip { start: 4.0, end: 10.0, speed: 1.0 }];
        let line =
            |start: f64, end: f64, finished: bool| CaptionLine { start, end, text: "x".into(), finished };
        project.captions = vec![line(0.5, 1.5, false), line(4.5, 6.0, false), line(2.8, 3.2, true)];
        let layout = Layout::new(&project, 1920, 1080);
        let at = |t: f64| frame_at(&project, &layout, t).caption.map(|(i, _)| i);
        assert_eq!(at(1.0), Some(0));
        assert_eq!(at(1.5), None, "a caption ends where its range does");
        assert_eq!(at(4.6), Some(1));
        // The recording's 5 s is the finished video's 3 s, where the voice-over's caption wins.
        assert_eq!(at(5.0), Some(2));
        assert_eq!(at(9.0), None);
        let (_, fraction) = frame_at(&project, &layout, 5.25).caption.unwrap();
        assert!((fraction - 0.5).abs() < 1e-9, "halfway through the voice-over's caption: {fraction}");
        project.show_captions = false;
        assert_eq!(frame_at(&project, &layout, 1.0).caption, None, "hidden from the video");
    }

    #[test]
    fn view_never_shows_past_the_frame() {
        let mut project = RenderProject::from_parts(
            std::path::Path::new("/tmp"),
            &shotmaxxing_core::project::Project::default(),
            &[],
            &[],
        );
        project.capture = CAPTURE;
        project.duration = 10.0;
        project.zooms = vec![zoom(0.0, 10.0, 2.0, 0.0, 0.0)];
        let layout = Layout::new(&project, 1920, 1080);
        let frame = frame_at(&project, &layout, 5.0);
        // The top-left corner of the frame stays covered: tx and ty can't go positive.
        assert!(frame.view.tx <= 1e-9 && frame.view.ty <= 1e-9, "{:?}", frame.view);
    }

    #[test]
    fn the_cursor_smears_only_while_it_moves() {
        use shotmaxxing_core::telemetry::CursorSample;
        let sample = |t_us: u64, x: f64| CursorSample { t_us, x, y: 300.0, ..CursorSample::default() };
        // A fast move from 100 to 900 between 1 s and 1.3 s, then still, sampled every 10 ms as the recorder does.
        let path: Vec<CursorSample> = (0..=500u64)
            .map(|i| {
                let u = ((i as f64 / 100.0 - 1.0) / 0.3).clamp(0.0, 1.0);
                sample(i * 10_000, 100.0 + 800.0 * u)
            })
            .collect();
        let mut project = RenderProject::from_parts(
            std::path::Path::new("/tmp"),
            &shotmaxxing_core::project::Project::default(),
            &path,
            &[],
        );
        project.capture = CAPTURE;
        project.duration = 5.0;
        project.hide_idle = false;
        let layout = Layout::new(&project, 1920, 1080);

        project.motion_blur = 1.0;
        let moving = frame_at(&project, &layout, 1.15).cursor.expect("a cursor mid-move");
        let from = moving.from.expect("a smear mid-move");
        assert!(from.x < moving.at.x, "the smear trails behind: {from:?} → {:?}", moving.at);
        assert!(frame_at(&project, &layout, 4.0).cursor.unwrap().from.is_none(), "no smear when still");

        project.motion_blur = 0.0;
        assert!(frame_at(&project, &layout, 1.15).cursor.unwrap().from.is_none(), "no smear when it's off");
    }
}
