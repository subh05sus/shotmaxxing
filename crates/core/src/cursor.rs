//! The rendered cursor's path: smoothed, snapped onto clicks, and optionally looped.
//!
//! Smoothing runs a zero-phase filter (forward then backward), so the cursor doesn't lag behind the hand the way a
//! plain moving average does. Snap-to-click then pulls the path onto each click point around the moment of the
//! click, so a near-miss reads as a clean hit and click effects land exactly under the cursor.

use crate::geometry::Point;
use crate::telemetry::{CursorSample, InputEvent};

/// How far around a click the path is pulled onto the click point.
const SNAP_WINDOW_US: f64 = 180_000.0;
/// Time constant of the smoothing filter at strength 1.
const MAX_TAU_US: f64 = 90_000.0;

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
    fn idle_spans_find_still_cursors() {
        let samples = vec![
            CursorSample { t_us: 0, x: 0.0, y: 0.0, ..Default::default() },
            CursorSample { t_us: 5_000_000, x: 100.0, y: 0.0, ..Default::default() },
        ];
        assert_eq!(idle_spans(&samples, 2_000), vec![(2_000_000, 5_000_000)]);
    }
}
