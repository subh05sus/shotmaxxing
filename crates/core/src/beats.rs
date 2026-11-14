//! The beats compiler: turns a planned take into a performance that reads as a person using the app.
//!
//! Three ideas carry most of the effect:
//! - **Fitts's law for travel time.** Moving to a target takes `a + b · log2(1 + distance / width)`: long moves
//!   take longer than short ones, small targets longer than big ones, and neither grows linearly.
//! - **Minimum-jerk motion along an arc.** Hands accelerate and decelerate smoothly (the minimum-jerk profile) and
//!   travel along gentle curves, never straight lines at constant speed.
//! - **Settle, press, look.** The cursor settles before a press, holds it briefly, then pauses so viewers can see
//!   what the click did.
//!
//! The compiler is pure and deterministic: the same beats and seed always give the same performance, so takes can be
//! previewed, tested and re-filmed exactly.

use crate::geometry::{Point, Rect};
use crate::hash::{mix64, unit};
use crate::keys::KeyChord;
use crate::telemetry::MouseButton;
use serde::Serialize;

/// A beat whose targets have been located on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedBeat {
    Click { target: Aim, button: MouseButton, count: u8, dwell_ms: Option<u32> },
    Hover { target: Aim, dwell_ms: Option<u32> },
    Drag { from: Aim, to: Aim },
    Type { text: String, cps: Option<f64> },
    Key { chord: KeyChord },
    Scroll { at: Option<Aim>, dx: f64, dy: f64 },
    Wait { ms: u32 },
    Say { text: String, ms: Option<u32> },
    UserStep { message: String },
}

/// Where to aim, and how big the target is (Fitts's law needs the width).
#[derive(Clone, Debug, PartialEq)]
pub struct Aim {
    pub point: Point,
    /// The control's bounds, when known.
    pub bounds: Option<Rect>,
    pub label: Option<String>,
}

impl Aim {
    pub fn point(point: Point) -> Self {
        Self { point, bounds: None, label: None }
    }

    pub fn control(bounds: Rect, label: Option<String>) -> Self {
        Self { point: bounds.center(), bounds: Some(bounds), label }
    }

    fn width(&self, default: f64) -> f64 {
        self.bounds.map_or(default, |b| b.width.min(b.height).max(4.0))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    /// Fitts's law intercept and slope, in milliseconds.
    pub fitts_a_ms: f64,
    pub fitts_b_ms: f64,
    pub min_travel_ms: f64,
    pub max_travel_ms: f64,
    /// Target width assumed for bare points.
    pub default_target_width: f64,
    pub settle_ms: f64,
    pub hold_ms: f64,
    pub double_click_gap_ms: f64,
    pub pause_after_ms: f64,
    /// Average time per typed character, and how much it varies (0 to 1).
    pub type_char_ms: f64,
    pub type_jitter: f64,
    pub word_pause_ms: f64,
    pub sentence_pause_ms: f64,
    pub key_pre_ms: f64,
    pub key_post_ms: f64,
    /// Arc bulge as a fraction of the distance, and its cap in points.
    pub arc_strength: f64,
    pub max_arc: f64,
    /// Dragging is slower than moving.
    pub drag_slowdown: f64,
    pub lead_ms: f64,
    pub tail_ms: f64,
    /// Where the cursor enters from, relative to the first target, when no start point is given.
    pub approach: Point,
    pub seed: u64,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            fitts_a_ms: 110.0,
            fitts_b_ms: 125.0,
            min_travel_ms: 120.0,
            max_travel_ms: 1_000.0,
            default_target_width: 24.0,
            settle_ms: 90.0,
            hold_ms: 75.0,
            double_click_gap_ms: 95.0,
            pause_after_ms: 380.0,
            type_char_ms: 55.0,
            type_jitter: 0.35,
            word_pause_ms: 55.0,
            sentence_pause_ms: 160.0,
            key_pre_ms: 120.0,
            key_post_ms: 280.0,
            arc_strength: 0.14,
            max_arc: 90.0,
            drag_slowdown: 1.35,
            lead_ms: 600.0,
            tail_ms: 900.0,
            approach: Point::new(-150.0, -110.0),
            seed: 0x5307_6d61_7878_696e,
        }
    }
}

impl Timing {
    /// Fitts's law movement time for a distance and target width.
    pub fn travel_ms(&self, distance: f64, width: f64) -> f64 {
        if distance < 0.5 {
            return 0.0;
        }
        let id = (1.0 + distance / width.max(1.0)).log2();
        (self.fitts_a_ms + self.fitts_b_ms * id).clamp(self.min_travel_ms, self.max_travel_ms)
    }
}

/// One cursor movement: a cubic Bézier from `from` to `to`, timed with a minimum-jerk profile.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Segment {
    pub start_ms: f64,
    pub end_ms: f64,
    pub from: Point,
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

impl Segment {
    pub fn position_at(&self, t_ms: f64) -> Point {
        let span = self.end_ms - self.start_ms;
        let tau = if span > 0.0 { ((t_ms - self.start_ms) / span).clamp(0.0, 1.0) } else { 1.0 };
        bezier(self.from, self.c1, self.c2, self.to, min_jerk(tau))
    }
}

/// Something that happens at an instant of the performance.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "cue", rename_all = "snake_case")]
pub enum Cue {
    BeatStart {
        at_ms: f64,
        beat: usize,
    },
    MouseDown {
        at_ms: f64,
        point: Point,
        button: MouseButton,
        count: u8,
    },
    MouseUp {
        at_ms: f64,
        point: Point,
        button: MouseButton,
    },
    Char {
        at_ms: f64,
        ch: char,
    },
    Key {
        at_ms: f64,
        chord: String,
    },
    Scroll {
        at_ms: f64,
        point: Point,
        dx: f64,
        dy: f64,
    },
    Caption {
        at_ms: f64,
        until_ms: f64,
        text: String,
    },
    /// The clock stops here until the person hands control back.
    UserStep {
        at_ms: f64,
        message: String,
    },
}

impl Cue {
    pub fn at_ms(&self) -> f64 {
        match self {
            Self::BeatStart { at_ms, .. }
            | Self::MouseDown { at_ms, .. }
            | Self::MouseUp { at_ms, .. }
            | Self::Char { at_ms, .. }
            | Self::Key { at_ms, .. }
            | Self::Scroll { at_ms, .. }
            | Self::Caption { at_ms, .. }
            | Self::UserStep { at_ms, .. } => *at_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Performance {
    pub start: Point,
    pub segments: Vec<Segment>,
    pub cues: Vec<Cue>,
    pub duration_ms: f64,
}

impl Performance {
    /// Where the cursor is at a moment of the performance.
    pub fn cursor_at(&self, t_ms: f64) -> Point {
        let idx = self.segments.partition_point(|s| s.start_ms <= t_ms);
        if idx == 0 {
            return self.start;
        }
        let seg = &self.segments[idx - 1];
        if t_ms >= seg.end_ms { seg.to } else { seg.position_at(t_ms) }
    }

    /// Samples the cursor path at a fixed rate, for previews and synthetic telemetry.
    pub fn sample_cursor(&self, hz: f64) -> Vec<(f64, Point)> {
        let step = 1000.0 / hz.max(1.0);
        let count = (self.duration_ms / step).ceil() as usize + 1;
        (0..count)
            .map(|i| {
                let t = (i as f64 * step).min(self.duration_ms);
                (t, self.cursor_at(t))
            })
            .collect()
    }
}

struct Builder<'a> {
    timing: &'a Timing,
    now: f64,
    pos: Point,
    segments: Vec<Segment>,
    cues: Vec<Cue>,
    rng: u64,
}

impl Builder<'_> {
    fn move_to(&mut self, aim: &Aim, slowdown: f64, arc_scale: f64) {
        let to = aim.point;
        let distance = self.pos.distance(to);
        let ms = self.timing.travel_ms(distance, aim.width(self.timing.default_target_width)) * slowdown;
        if ms <= 0.0 {
            self.pos = to;
            return;
        }
        let (c1, c2) = arc_controls(self.pos, to, self.timing.arc_strength * arc_scale, self.timing.max_arc);
        self.segments.push(Segment { start_ms: self.now, end_ms: self.now + ms, from: self.pos, c1, c2, to });
        self.now += ms;
        self.pos = to;
    }

    fn wait(&mut self, ms: f64) {
        self.now += ms.max(0.0);
    }

    fn jitter(&mut self) -> f64 {
        self.rng = mix64(self.rng);
        unit(self.rng) * 2.0 - 1.0
    }
}

/// Compiles resolved beats into a timed performance.
pub fn compile(beats: &[ResolvedBeat], start: Option<Point>, timing: &Timing) -> Performance {
    let first_aim = beats.iter().find_map(|b| match b {
        ResolvedBeat::Click { target, .. } | ResolvedBeat::Hover { target, .. } => Some(target.point),
        ResolvedBeat::Drag { from, .. } => Some(from.point),
        ResolvedBeat::Scroll { at: Some(a), .. } => Some(a.point),
        _ => None,
    });
    let start = start.or_else(|| first_aim.map(|p| p + timing.approach)).unwrap_or(Point::ZERO);
    let mut b = Builder {
        timing,
        now: timing.lead_ms,
        pos: start,
        segments: Vec::new(),
        cues: Vec::new(),
        rng: timing.seed,
    };

    for (index, beat) in beats.iter().enumerate() {
        b.cues.push(Cue::BeatStart { at_ms: b.now, beat: index });
        match beat {
            ResolvedBeat::Click { target, button, count, dwell_ms } => {
                b.move_to(target, 1.0, 1.0);
                b.wait(timing.settle_ms);
                for i in 0..(*count).max(1) {
                    if i > 0 {
                        b.wait(timing.double_click_gap_ms);
                    }
                    b.cues.push(Cue::MouseDown { at_ms: b.now, point: b.pos, button: *button, count: i + 1 });
                    b.wait(timing.hold_ms);
                    b.cues.push(Cue::MouseUp { at_ms: b.now, point: b.pos, button: *button });
                }
                b.wait(dwell_ms.map_or(timing.pause_after_ms, f64::from));
            }
            ResolvedBeat::Hover { target, dwell_ms } => {
                b.move_to(target, 1.0, 1.0);
                b.wait(dwell_ms.map_or(timing.pause_after_ms * 1.6, f64::from));
            }
            ResolvedBeat::Drag { from, to } => {
                b.move_to(from, 1.0, 1.0);
                b.wait(timing.settle_ms);
                b.cues.push(Cue::MouseDown {
                    at_ms: b.now,
                    point: b.pos,
                    button: MouseButton::Left,
                    count: 1,
                });
                b.wait(timing.hold_ms);
                // Dragging is slower and straighter than a free move.
                b.move_to(to, timing.drag_slowdown, 0.35);
                b.wait(timing.settle_ms);
                b.cues.push(Cue::MouseUp { at_ms: b.now, point: b.pos, button: MouseButton::Left });
                b.wait(timing.pause_after_ms);
            }
            ResolvedBeat::Type { text, cps } => {
                let base = cps.map_or(timing.type_char_ms, |c| 1000.0 / c.clamp(1.0, 60.0));
                let mut previous: Option<char> = None;
                for ch in text.chars() {
                    let jitter = 1.0 + b.jitter() * timing.type_jitter;
                    let mut gap = base * jitter;
                    if previous.is_some_and(|p| p == ' ') {
                        gap += timing.word_pause_ms;
                    }
                    if previous.is_some_and(|p| matches!(p, '.' | ',' | '!' | '?' | ';' | ':' | '\n')) {
                        gap += timing.sentence_pause_ms;
                    }
                    if previous.is_some() {
                        b.wait(gap);
                    }
                    b.cues.push(Cue::Char { at_ms: b.now, ch });
                    previous = Some(ch);
                }
                b.wait(timing.pause_after_ms);
            }
            ResolvedBeat::Key { chord } => {
                b.wait(timing.key_pre_ms);
                b.cues
                    .push(Cue::Key { at_ms: b.now, chord: chord.display(crate::keys::Platform::current()) });
                b.wait(timing.key_post_ms);
            }
            ResolvedBeat::Scroll { at, dx, dy } => {
                if let Some(at) = at {
                    b.move_to(at, 1.0, 1.0);
                    b.wait(timing.settle_ms);
                }
                // Long scrolls are spread over a few wheel events, as a hand would.
                let distance = dx.hypot(*dy);
                let steps = (distance / 120.0).ceil().clamp(1.0, 12.0) as usize;
                for _ in 0..steps {
                    b.cues.push(Cue::Scroll {
                        at_ms: b.now,
                        point: b.pos,
                        dx: dx / steps as f64,
                        dy: dy / steps as f64,
                    });
                    b.wait(45.0);
                }
                b.wait(timing.pause_after_ms);
            }
            ResolvedBeat::Wait { ms } => b.wait(f64::from(*ms)),
            ResolvedBeat::Say { text, ms } => {
                // A caption shows over the beats that follow; it doesn't hold the clock.
                let reading_ms = ms
                    .map_or_else(|| (text.chars().count() as f64 * 55.0).clamp(1_500.0, 6_000.0), f64::from);
                b.cues.push(Cue::Caption { at_ms: b.now, until_ms: b.now + reading_ms, text: text.clone() });
                b.wait(150.0);
            }
            ResolvedBeat::UserStep { message } => {
                b.cues.push(Cue::UserStep { at_ms: b.now, message: message.clone() });
            }
        }
    }
    b.wait(timing.tail_ms);
    let duration_ms = b.now;
    Performance { start, segments: b.segments, cues: b.cues, duration_ms }
}

/// Control points for a gentle arc. Moves bow upward on screen (a wrist pivots), vertical moves bow outward.
fn arc_controls(from: Point, to: Point, strength: f64, cap: f64) -> (Point, Point) {
    let delta = to - from;
    let d = delta.length();
    if d < 1.0 {
        return (from, to);
    }
    let dir = delta * (1.0 / d);
    let mut normal = Point::new(-dir.y, dir.x);
    if normal.y > 0.0 || (normal.y.abs() < 1e-9 && normal.x < 0.0) {
        normal = normal * -1.0;
    }
    let bulge = (d * strength).min(cap);
    let c1 = from + dir * (d / 3.0) + normal * bulge;
    let c2 = from + dir * (2.0 * d / 3.0) + normal * bulge;
    (c1, c2)
}

fn bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

/// The minimum-jerk position profile: smooth start, smooth stop, fastest in the middle.
pub fn min_jerk(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (10.0 - 15.0 * t + 6.0 * t * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(x: f64, y: f64) -> ResolvedBeat {
        ResolvedBeat::Click {
            target: Aim::point(Point::new(x, y)),
            button: MouseButton::Left,
            count: 1,
            dwell_ms: None,
        }
    }

    #[test]
    fn fitts_law_shapes_travel_time() {
        let t = Timing::default();
        assert!(t.travel_ms(800.0, 24.0) > t.travel_ms(100.0, 24.0));
        assert!(t.travel_ms(400.0, 12.0) > t.travel_ms(400.0, 120.0));
        assert_eq!(t.travel_ms(0.0, 24.0), 0.0);
        assert!(t.travel_ms(5_000.0, 1.0) <= t.max_travel_ms);
        // Not linear: doubling the distance adds far less than double the time.
        assert!(t.travel_ms(1_600.0, 24.0) < 1.4 * t.travel_ms(800.0, 24.0));
    }

    #[test]
    fn clicks_land_exactly_on_target_after_settling() {
        let perf = compile(
            &[click(500.0, 300.0), click(900.0, 650.0)],
            Some(Point::new(100.0, 100.0)),
            &Timing::default(),
        );
        let downs: Vec<_> = perf
            .cues
            .iter()
            .filter_map(|c| match c {
                Cue::MouseDown { at_ms, point, .. } => Some((*at_ms, *point)),
                _ => None,
            })
            .collect();
        assert_eq!(downs.len(), 2);
        for (at, point) in &downs {
            let here = perf.cursor_at(*at);
            assert!(here.distance(*point) < 1e-6, "cursor {here:?} vs click {point:?}");
            // The movement finished before the press.
            let seg = perf.segments.iter().rev().find(|s| s.end_ms <= *at).unwrap();
            assert!(at - seg.end_ms >= Timing::default().settle_ms - 1e-9);
        }
    }

    #[test]
    fn segments_are_continuous_and_curved() {
        let perf = compile(
            &[click(500.0, 300.0), click(900.0, 300.0)],
            Some(Point::new(100.0, 300.0)),
            &Timing::default(),
        );
        for pair in perf.segments.windows(2) {
            assert!(pair[0].to.distance(pair[1].from) < 1e-9);
            assert!(pair[1].start_ms >= pair[0].end_ms);
        }
        // A horizontal move bows upward (smaller y) in the middle.
        let s = perf.segments[1];
        let mid = s.position_at((s.start_ms + s.end_ms) / 2.0);
        assert!(mid.y < 300.0 - 5.0, "{mid:?}");
    }

    #[test]
    fn motion_eases_in_and_out() {
        assert_eq!(min_jerk(0.0), 0.0);
        assert_eq!(min_jerk(1.0), 1.0);
        assert!((min_jerk(0.5) - 0.5).abs() < 1e-12);
        // Slow at the ends, fast in the middle.
        assert!(min_jerk(0.1) < 0.1 && min_jerk(0.9) > 0.9);
    }

    #[test]
    fn typing_is_in_order_readable_and_deterministic() {
        let beats = [ResolvedBeat::Type { text: "Hello, world".into(), cps: None }];
        let a = compile(&beats, Some(Point::ZERO), &Timing::default());
        let b = compile(&beats, Some(Point::ZERO), &Timing::default());
        assert_eq!(a, b);
        let chars: Vec<_> = a
            .cues
            .iter()
            .filter_map(|c| match c {
                Cue::Char { at_ms, ch } => Some((*at_ms, *ch)),
                _ => None,
            })
            .collect();
        assert_eq!(chars.iter().map(|c| c.1).collect::<String>(), "Hello, world");
        assert!(chars.windows(2).all(|w| w[1].0 > w[0].0));
        // A pause after the comma.
        let gap_after_comma = chars[7].0 - chars[6].0;
        let ordinary_gap = chars[2].0 - chars[1].0;
        assert!(gap_after_comma > ordinary_gap);
    }

    #[test]
    fn captions_do_not_hold_the_clock_and_user_steps_are_marked() {
        let beats = [
            ResolvedBeat::Say { text: "Open the docs".into(), ms: Some(2_000) },
            click(100.0, 100.0),
            ResolvedBeat::UserStep { message: "Sign in".into() },
        ];
        let perf = compile(&beats, Some(Point::new(50.0, 50.0)), &Timing::default());
        let caption = perf
            .cues
            .iter()
            .find_map(|c| match c {
                Cue::Caption { at_ms, until_ms, .. } => Some((*at_ms, *until_ms)),
                _ => None,
            })
            .unwrap();
        let click_at = perf
            .cues
            .iter()
            .find_map(|c| match c {
                Cue::MouseDown { at_ms, .. } => Some(*at_ms),
                _ => None,
            })
            .unwrap();
        assert!(click_at < caption.1, "the click happens while the caption shows");
        assert!(perf.cues.iter().any(|c| matches!(c, Cue::UserStep { .. })));
    }

    #[test]
    fn the_cursor_approaches_from_up_and_left() {
        let perf = compile(&[click(500.0, 400.0)], None, &Timing::default());
        assert!(perf.start.x < 500.0 && perf.start.y < 400.0);
        assert_eq!(perf.cursor_at(0.0), perf.start);
    }
}
