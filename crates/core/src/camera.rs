//! The virtual camera: what part of the capture is on screen at each moment.
//!
//! Zoom regions say where the camera should be; this module moves it there smoothly. Scale is interpolated in log
//! space, so a zoom from 1x to 4x passes 2x at its midpoint and feels even, rather than rushing at the start.

use crate::geometry::{Point, Rect, Size};
use crate::project::ZoomRegion;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub scale: f64,
    pub focus: Point,
}

impl CameraState {
    pub fn wide(capture: Size) -> Self {
        Self { scale: 1.0, focus: capture.center() }
    }

    fn blend(self, other: Self, t: f64) -> Self {
        let scale = (self.scale.ln() + (other.scale.ln() - self.scale.ln()) * t).exp();
        Self { scale, focus: self.focus.lerp(other.focus, t) }
    }

    /// The visible part of the capture.
    pub fn viewport(self, capture: Size) -> Rect {
        let size = Size::new(capture.width / self.scale.max(1.0), capture.height / self.scale.max(1.0));
        Rect::from_center(self.focus, size).clamp_inside(capture.bounds())
    }
}

pub struct CameraOptions<'a> {
    pub follow_mouse: bool,
    /// 0 to 1.
    pub follow_strength: f64,
    /// Smoothed cursor position at a time, for follow mouse.
    pub cursor: Option<&'a dyn Fn(u64) -> Option<Point>>,
}

impl Default for CameraOptions<'_> {
    fn default() -> Self {
        Self { follow_mouse: false, follow_strength: 0.35, cursor: None }
    }
}

/// Camera state at a time. `zooms` must be sorted and non-overlapping (auto zoom guarantees both).
pub fn camera_at(t_us: u64, zooms: &[ZoomRegion], capture: Size, options: &CameraOptions<'_>) -> CameraState {
    let wide = CameraState::wide(capture);
    let idx = zooms.partition_point(|z| z.start_us <= t_us);
    if idx == 0 {
        return wide;
    }
    let i = idx - 1;
    let zone = &zooms[i];
    if t_us >= zone.end_us {
        return wide;
    }
    let target = region_state(zone, t_us, capture, options);
    let ease_in = u64::from(zone.ease_in_ms) * 1_000;
    let ease_out = u64::from(zone.ease_out_ms) * 1_000;
    let adjacent_prev = i > 0 && zooms[i - 1].end_us + 1_000 >= zone.start_us;
    let adjacent_next = zooms.get(i + 1).is_some_and(|n| zone.end_us + 1_000 >= n.start_us);

    let into = t_us - zone.start_us;
    if ease_in > 0 && into < ease_in {
        let from = if adjacent_prev {
            region_state(&zooms[i - 1], zooms[i - 1].end_us, capture, options)
        } else {
            wide
        };
        return from.blend(target, smootherstep(into as f64 / ease_in as f64));
    }
    let left = zone.end_us - t_us;
    if !adjacent_next && ease_out > 0 && left < ease_out {
        return target.blend(wide, smootherstep(1.0 - left as f64 / ease_out as f64));
    }
    target
}

fn region_state(zone: &ZoomRegion, t_us: u64, capture: Size, options: &CameraOptions<'_>) -> CameraState {
    let mut focus = zone.focus;
    let follow = zone.follow_mouse.unwrap_or(options.follow_mouse);
    if follow
        && let Some(cursor) = options.cursor
        && let Some(p) = cursor(t_us)
    {
        focus = focus.lerp(p, options.follow_strength.clamp(0.0, 1.0));
    }
    let state = CameraState { scale: zone.scale.max(1.0), focus };
    // Keep the focus where the viewport stays inside the capture.
    CameraState { scale: state.scale, focus: state.viewport(capture).center() }
}

/// Ken Perlin's smootherstep: zero velocity and acceleration at both ends.
pub fn smootherstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ZoomSource;

    const CAPTURE: Size = Size::new(1600.0, 1000.0);

    fn zoom(start_ms: u64, end_ms: u64, scale: f64, focus: Point) -> ZoomRegion {
        ZoomRegion {
            id: "z".into(),
            start_us: start_ms * 1000,
            end_us: end_ms * 1000,
            scale,
            focus,
            follow_mouse: None,
            ease_in_ms: 500,
            ease_out_ms: 500,
            source: ZoomSource::Manual,
            reason: None,
        }
    }

    #[test]
    fn wide_outside_zooms_and_zoomed_inside() {
        let zooms = [zoom(1_000, 4_000, 2.0, Point::new(400.0, 300.0))];
        let opts = CameraOptions::default();
        assert_eq!(camera_at(0, &zooms, CAPTURE, &opts), CameraState::wide(CAPTURE));
        let mid = camera_at(2_500_000, &zooms, CAPTURE, &opts);
        assert_eq!(mid.scale, 2.0);
        assert_eq!(mid.focus, Point::new(400.0, 300.0));
        assert_eq!(camera_at(5_000_000, &zooms, CAPTURE, &opts).scale, 1.0);
    }

    #[test]
    fn scale_moves_evenly_in_log_space() {
        let zooms = [zoom(0, 10_000, 4.0, CAPTURE.center())];
        let half = camera_at(250_000, &zooms, CAPTURE, &CameraOptions::default());
        assert!((half.scale - 2.0).abs() < 1e-9, "{}", half.scale);
    }

    #[test]
    fn viewport_stays_inside_the_capture_everywhere() {
        let zooms =
            [zoom(0, 3_000, 3.0, Point::new(5.0, 5.0)), zoom(3_000, 6_000, 2.5, Point::new(1590.0, 990.0))];
        let cursor = |t: u64| Some(Point::new((t as f64 / 1000.0) % 1600.0, 500.0));
        let opts = CameraOptions { follow_mouse: true, follow_strength: 0.5, cursor: Some(&cursor) };
        for t in (0..7_000_000).step_by(37_000) {
            let v = camera_at(t, &zooms, CAPTURE, &opts).viewport(CAPTURE);
            assert!(v.min_x() >= -1e-6 && v.min_y() >= -1e-6, "{t}: {v:?}");
            assert!(v.max_x() <= CAPTURE.width + 1e-6 && v.max_y() <= CAPTURE.height + 1e-6, "{t}: {v:?}");
        }
    }

    #[test]
    fn adjacent_zooms_glide_without_returning_wide() {
        let zooms = [
            zoom(0, 2_000, 2.0, Point::new(500.0, 500.0)),
            zoom(2_000, 4_000, 2.0, Point::new(1100.0, 500.0)),
        ];
        let opts = CameraOptions::default();
        // Just before the hand-over the camera is still zoomed in, not easing out.
        assert_eq!(camera_at(1_990_000, &zooms, CAPTURE, &opts).scale, 2.0);
        let during = camera_at(2_250_000, &zooms, CAPTURE, &opts);
        assert_eq!(during.scale, 2.0);
        assert!(during.focus.x > 500.0 && during.focus.x < 1100.0);
    }
}
