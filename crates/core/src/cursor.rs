//! The rendered cursor's path: smoothed, snapped onto clicks, and optionally looped.
//!
//! Smoothing runs a zero-phase filter (forward then backward), so the cursor doesn't lag behind the hand the way a
//! plain moving average does. Snap-to-click then pulls the path onto each click point around the moment of the
//! click, so a near-miss reads as a clean hit and click effects land exactly under the cursor.

use crate::geometry::Point;
use crate::telemetry::{CursorSample, InputEvent};

/// How far around a click the path is pulled onto the click point.
const SNAP_WINDOW_US: f64 = 180_000.0;
/// How long a looping cursor takes to ease back to where it started.
const LOOP_MS: u64 = 1_000;
/// Drawn paths keep one sample per frame at 60 fps.
const THIN_US: u64 = 16_000;
/// Time constant of the smoothing filter at strength 1.
const MAX_TAU_US: f64 = 90_000.0;

/// The cursor settings that shape the drawn path.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PathSettings {
    /// 0 to 1.
    pub smoothing: f64,
    pub snap_to_clicks: bool,
    pub loop_to_start: bool,
}

/// The cursor path as drawn: smoothed, snapped onto clicks, eased back to its start when looping, and thinned to 60
/// samples a second. The editor's preview and the exporter both draw from this path, so they always agree.
pub fn render_path(
    samples: &[CursorSample],
    events: &[InputEvent],
    settings: PathSettings,
) -> Vec<CursorSample> {
    let mut path = smooth(samples, settings.smoothing);
    if settings.snap_to_clicks {
        snap_to_clicks(&mut path, events);
    }
    if settings.loop_to_start {
        loop_to_start(&mut path, LOOP_MS);
    }
    thin(path)
}

/// Keeps about one sample per 60 fps frame, plus the last one.
pub fn thin(samples: Vec<CursorSample>) -> Vec<CursorSample> {
    let mut out: Vec<CursorSample> = Vec::with_capacity(samples.len());
    let last = samples.last().copied();
    for sample in samples {
        if out.last().is_none_or(|prev| sample.t_us >= prev.t_us + THIN_US) {
            out.push(sample);
        }
    }
    if let Some(last) = last
        && out.last().is_some_and(|prev| prev.t_us != last.t_us)
    {
        out.push(last);
    }
    out
}

/// Where the drawn cursor is at `t_us`: a Catmull–Rom curve through the path with minimum-jerk timing between
/// samples, so it glides rather than stepping from sample to sample. Matches the editor preview.
pub fn position_at(path: &[CursorSample], t_us: f64) -> Option<Point> {
    let first = path.first()?;
    let last = path.last()?;
    if t_us <= first.t_us as f64 {
        return Some(first.point());
    }
    if t_us >= last.t_us as f64 {
        return Some(last.point());
    }
    let i = path.partition_point(|s| (s.t_us as f64) <= t_us).saturating_sub(1).min(path.len() - 2);
    let p1 = path[i];
    let p2 = path[i + 1];
    let p0 = if i > 0 { path[i - 1] } else { p1 };
    let p3 = path.get(i + 2).copied().unwrap_or(p2);
    let span = (p2.t_us - p1.t_us).max(1) as f64;
    let u = crate::beats::min_jerk((t_us - p1.t_us as f64) / span);
    let cr = |a: f64, b: f64, c: f64, d: f64| {
        0.5 * (2.0 * b
            + (-a + c) * u
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * u * u
            + (-a + 3.0 * b - 3.0 * c + d) * u * u * u)
    };
    Some(Point::new(cr(p0.x, p1.x, p2.x, p3.x), cr(p0.y, p1.y, p2.y, p3.y)))
}

/// Smooths cursor samples. `strength` 0 returns the input unchanged; 1 is the smoothest.
pub fn smooth(samples: &[CursorSample], strength: f64) -> Vec<CursorSample> {
    let strength = strength.clamp(0.0, 1.0);
    if samples.len() < 3 || strength <= 0.0 {
        return samples.to_vec();
    }
    let tau = MAX_TAU_US * strength * strength;
    let mut out = samples.to_vec();
    let filter = |out: &mut [CursorSample], order: &mut dyn Iterator<Item = usize>| {
        let mut prev: Option<(u64, Point)> = None;
        for i in order {
            let s = out[i];
            let p = s.point();
            let filtered = match prev {
                None => p,
                Some((pt, pp)) => {
                    let dt = s.t_us.abs_diff(pt) as f64;
                    let alpha = 1.0 - (-dt / tau).exp();
                    pp.lerp(p, alpha)
                }
            };
            out[i].x = filtered.x;
            out[i].y = filtered.y;
            prev = Some((s.t_us, filtered));
        }
    };
    let n = out.len();
    filter(&mut out, &mut (0..n));
    filter(&mut out, &mut (0..n).rev());
    out
}

/// Pulls the path onto click points around each click, with a smooth falloff.
pub fn snap_to_clicks(samples: &mut [CursorSample], events: &[InputEvent]) {
    let clicks: Vec<(u64, Point)> = events
        .iter()
        .filter_map(|e| match e {
            InputEvent::MouseDown { t_us, x, y, .. } | InputEvent::MouseUp { t_us, x, y, .. } => {
                Some((*t_us, Point::new(*x, *y)))
            }
            _ => None,
        })
        .collect();
    if clicks.is_empty() {
        return;
    }
    for s in samples.iter_mut() {
        // The nearest click in time decides the pull.
        let Some((ct, cp)) = clicks.iter().min_by_key(|(t, _)| t.abs_diff(s.t_us)) else { continue };
        let dt = ct.abs_diff(s.t_us) as f64;
        if dt >= SNAP_WINDOW_US {
            continue;
        }
        let w = crate::camera::smootherstep(1.0 - dt / SNAP_WINDOW_US);
        let p = s.point().lerp(*cp, w);
        s.x = p.x;
        s.y = p.y;
    }
}

/// Eases the cursor back to its first position over the last `ms`, so looping GIFs don't jump.
pub fn loop_to_start(samples: &mut [CursorSample], ms: u64) {
    let (Some(first), Some(last)) = (samples.first().copied(), samples.last().copied()) else { return };
    let window = ms * 1_000;
    let begin = last.t_us.saturating_sub(window);
    let anchor = crate::telemetry::cursor_at(samples, begin).unwrap_or(last.point());
    for s in samples.iter_mut().filter(|s| s.t_us >= begin) {
        let t = if window > 0 { (s.t_us - begin) as f64 / window as f64 } else { 1.0 };
        let p = anchor.lerp(first.point(), crate::beats::min_jerk(t));
        s.x = p.x;
        s.y = p.y;
    }
}

/// Times at which the cursor should hide because it has been still for `idle_ms`.
pub fn idle_spans(samples: &[CursorSample], idle_ms: u64) -> Vec<(u64, u64)> {
    let idle = idle_ms * 1_000;
    let mut spans = Vec::new();
    let mut i = 0;
    while i < samples.len() {
        let anchor = samples[i].point();
        let mut j = i;
        while j + 1 < samples.len() && samples[j + 1].point().distance(anchor) < 1.0 {
            j += 1;
        }
        // Samples are only written when something changes, so a still cursor shows up as a gap before the next one.
        let end = samples.get(j + 1).map_or(samples[j].t_us, |s| s.t_us);
        if end - samples[i].t_us > idle {
            spans.push((samples[i].t_us + idle, end));
        }
        i = j + 1;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{InputSource, MouseButton};

    fn jittery_line() -> Vec<CursorSample> {
        (0..240)
            .map(|i| {
                let jitter = if i % 2 == 0 { 3.0 } else { -3.0 };
                CursorSample { t_us: i * 8_333, x: i as f64 * 4.0, y: 300.0 + jitter, ..Default::default() }
            })
            .collect()
    }

    fn roughness(s: &[CursorSample]) -> f64 {
        s.windows(3).map(|w| (w[0].y - 2.0 * w[1].y + w[2].y).abs()).sum::<f64>() / s.len() as f64
    }

    #[test]
    fn smoothing_removes_jitter_without_lag() {
        let raw = jittery_line();
        let smooth = smooth(&raw, 0.7);
        assert!(roughness(&smooth) < roughness(&raw) * 0.2);
        // Zero-phase: in the middle of a straight run the smoothed x is still where the hand was.
        let mid = 120;
        assert!((smooth[mid].x - raw[mid].x).abs() < 2.0, "{} vs {}", smooth[mid].x, raw[mid].x);
        assert_eq!(super::smooth(&raw, 0.0), raw);
    }

    #[test]
    fn snapping_lands_on_the_click() {
        let mut path = smooth(&jittery_line(), 0.7);
        let click = InputEvent::MouseDown {
            t_us: 100 * 8_333,
            x: 405.0,
            y: 310.0,
            button: MouseButton::Left,
            clicks: 1,
            source: InputSource::Person,
        };
        snap_to_clicks(&mut path, &[click]);
        let at = crate::telemetry::cursor_at(&path, 100 * 8_333).unwrap();
        assert!(at.distance(Point::new(405.0, 310.0)) < 1e-6);
        // Far from the click, nothing changes.
        assert!((path[10].y - smooth(&jittery_line(), 0.7)[10].y).abs() < 1e-9);
    }

    #[test]
    fn loop_returns_to_the_start() {
        let mut path = jittery_line();
        loop_to_start(&mut path, 500);
        let first = path.first().unwrap().point();
        let last = path.last().unwrap().point();
        assert!(first.distance(last) < 1e-6);
    }

    #[test]
    fn thins_to_about_sixty_samples_a_second() {
        let thinned = thin(jittery_line());
        assert!(thinned.len() > 100 && thinned.len() < 130, "{}", thinned.len());
        assert_eq!(thinned.last().map(|s| s.t_us), Some(239 * 8_333));
    }

    #[test]
    fn position_passes_through_the_samples_and_holds_at_the_ends() {
        let path = vec![
            CursorSample { t_us: 0, x: 0.0, y: 0.0, ..Default::default() },
            CursorSample { t_us: 1_000_000, x: 100.0, y: 50.0, ..Default::default() },
            CursorSample { t_us: 2_000_000, x: 200.0, y: 0.0, ..Default::default() },
        ];
        assert_eq!(position_at(&path, -5.0), Some(Point::new(0.0, 0.0)));
        assert_eq!(position_at(&path, 1_000_000.0), Some(Point::new(100.0, 50.0)));
        assert_eq!(position_at(&path, 9e9), Some(Point::new(200.0, 0.0)));
        let between = position_at(&path, 500_000.0).unwrap();
        assert!(between.x > 0.0 && between.x < 100.0);
        assert_eq!(position_at(&[], 0.0), None);
    }

    #[test]
    fn idle_spans_find_still_cursors() {
        let samples = vec![
            CursorSample { t_us: 0, x: 0.0, y: 0.0, ..Default::default() },
            CursorSample { t_us: 5_000_000, x: 100.0, y: 0.0, ..Default::default() },
        ];
        assert_eq!(idle_spans(&samples, 2_000), vec![(2_000_000, 5_000_000)]);
    }
}
