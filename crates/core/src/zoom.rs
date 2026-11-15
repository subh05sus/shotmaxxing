//! Auto zoom: decides where and when the camera pushes in, and says why.
//!
//! Most recorders centre a zoom on the cursor at every click. That frames half a button when the click lands near
//! an edge, and ignores what the click caused. Auto zoom here reads several signals:
//!
//! | Signal | What it frames |
//! | --- | --- |
//! | Click | the control under the click (from accessibility focus), or an area around the pointer |
//! | Typing burst | the field being typed into |
//! | Screen change after a click | the control **and** the result that appeared |
//! | Dwell | where the pointer rested, when nothing was clicked |
//!
//! Moments close in time and space merge into one zoom, so a busy minute becomes a few calm shots instead of a
//! jittery cut every second. Every zoom keeps a plain-language reason, shown on the timeline.

use crate::geometry::{Point, Rect, Size};
use crate::project::{ZoomDepth, ZoomRegion, ZoomSource, ZoomTuning};
use crate::telemetry::{CursorSample, InputEvent};

/// How far from a click (in time) a focus change may be and still describe the clicked control.
const FOCUS_WINDOW_US: u64 = 400_000;
/// How long after a click a screen change still counts as its result.
const RESULT_WINDOW_US: u64 = 1_500_000;
/// Area framed around a bare click when no control bounds are known, in points.
const CLICK_BOX: Size = Size::new(280.0, 180.0);
const DWELL_RADIUS: f64 = 14.0;
/// How long a zoom holds, fully in, before the video ends, at the least. A moment later than that can't be framed
/// in time (the zoom would still be pushing in when the video stops), and it's usually the click that stopped the
/// take.
const END_HOLD_US: u64 = 500_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Moment {
    pub start_us: u64,
    pub end_us: u64,
    pub rect: Rect,
    pub reason: String,
    pub weight: f64,
}

pub struct ZoomInput<'a> {
    pub duration_us: u64,
    /// Capture size in points.
    pub capture: Size,
    pub events: &'a [InputEvent],
    pub cursor: &'a [CursorSample],
}

/// Finds the moments worth zooming on: only those on the recorded area. A click on another window or display (the
/// one on the recording pill, say), typing into another app, or the pointer resting off the take has nothing to
/// frame; a zoom on it would only push into the take's edge.
pub fn find_moments(input: &ZoomInput<'_>, tuning: &ZoomTuning) -> Vec<Moment> {
    let mut moments = Vec::new();
    // To a point either way, for rounding.
    let area = input.capture.bounds().inflate(1.0, 1.0);
    let focuses: Vec<(u64, Rect, Option<String>, String)> = input
        .events
        .iter()
        .filter_map(|e| match e {
            InputEvent::Focus { t_us, bounds, label, role } => {
                Some((*t_us, *bounds, label.clone(), role.clone()))
            }
            _ => None,
        })
        .collect();

    let focus_near = |t: u64, p: Point| {
        focuses
            .iter()
            .filter(|(ft, bounds, _, _)| {
                ft.abs_diff(t) <= FOCUS_WINDOW_US && bounds.inflate(6.0, 6.0).contains(p)
            })
            .min_by_key(|(ft, ..)| ft.abs_diff(t))
    };

    // Clicks.
    for event in input.events {
        if let InputEvent::MouseDown { t_us, x, y, .. } = event {
            let p = Point::new(*x, *y);
            if !area.contains(p) {
                continue;
            }
            let (rect, reason) = match focus_near(*t_us, p) {
                Some((_, bounds, label, role)) if bounds.area() > 16.0 => {
                    (*bounds, format!("Click on {}", describe(label.as_deref(), role)))
                }
                _ => (Rect::from_center(p, CLICK_BOX), "Click".to_string()),
            };
            let mut rect = rect;
            let mut reason = reason;
            if tuning.frame_results
                && let Some(change) = result_of_click(input.events, *t_us, input.capture)
            {
                rect = rect.union(change);
                reason.push_str(", then the result");
            }
            moments.push(Moment { start_us: *t_us, end_us: *t_us, rect, reason, weight: 1.0 });
        }
    }

    // Typing bursts, unless the project turned them off.
    let keys: Vec<(u64, bool)> = input
        .events
        .iter()
        .filter(|_| tuning.typing)
        .filter_map(|e| match e {
            InputEvent::KeyDown { t_us, modifiers_only, .. } if !modifiers_only => Some((*t_us, true)),
            _ => None,
        })
        .collect();
    let gap = u64::from(tuning.typing_gap_ms) * 1_000;
    let mut i = 0;
    while i < keys.len() {
        let start = keys[i].0;
        let mut end = start;
        let mut count = 1;
        while i + 1 < keys.len() && keys[i + 1].0 - keys[i].0 <= gap {
            i += 1;
            end = keys[i].0;
            count += 1;
        }
        i += 1;
        // The field being typed into: the most recent focus before the burst.
        let field = focuses.iter().filter(|(t, ..)| *t <= end).max_by_key(|(t, ..)| *t);
        let (rect, reason) = match field {
            Some((_, bounds, label, role)) if bounds.area() > 16.0 => {
                (*bounds, format!("Typing in {}", describe(label.as_deref(), role)))
            }
            _ => {
                let p = crate::telemetry::cursor_at(input.cursor, start).unwrap_or(input.capture.center());
                (Rect::from_center(p, CLICK_BOX), "Typing".to_string())
            }
        };
        if !area.contains(rect.center()) {
            continue;
        }
        let weight = if count >= 3 { 0.9 } else { 0.5 };
        moments.push(Moment { start_us: start, end_us: end, rect, reason, weight });
    }

    // Dwells: the pointer rests without clicking.
    if tuning.dwell {
        let min_dwell = u64::from(tuning.dwell_ms) * 1_000;
        let clicks: Vec<u64> = input
            .events
            .iter()
            .filter(|e| matches!(e, InputEvent::MouseDown { .. }))
            .map(InputEvent::t_us)
            .collect();
        for (start, end, p) in dwells(input.cursor, min_dwell) {
            let clicked = clicks.iter().any(|t| *t + 300_000 >= start && *t <= end + 300_000);
            if !clicked && area.contains(p) {
                moments.push(Moment {
                    start_us: start,
                    end_us: end,
                    rect: Rect::from_center(p, CLICK_BOX),
                    reason: "Pointing".into(),
                    weight: 0.4,
                });
            }
        }
    }

    moments.sort_by_key(|m| m.start_us);
    moments
}

/// Turns moments into zoom regions: pads them, merges neighbours, picks a scale and focus.
pub fn plan_zooms(
    input: &ZoomInput<'_>,
    tuning: &ZoomTuning,
    depth: ZoomDepth,
    transition_ms: u32,
) -> Vec<ZoomRegion> {
    let moments = find_moments(input, tuning);
    let bounds = input.capture.bounds();
    let max_scale = depth.max_scale();
    let lead = u64::from(tuning.lead_ms) * 1_000;
    let hold = u64::from(tuning.hold_ms) * 1_000;
    let merge_gap = u64::from(tuning.merge_gap_ms) * 1_000;
    // A merged zoom may only grow until it can still be framed at the smallest worthwhile scale.
    let max_framed =
        Size::new(input.capture.width / tuning.min_scale, input.capture.height / tuning.min_scale);

    struct Group {
        start: u64,
        end: u64,
        rect: Rect,
        reasons: Vec<String>,
        weight: f64,
    }
    let ease = u64::from(transition_ms) * 1_000;
    let mut groups: Vec<Group> = Vec::new();
    // The last moment that can still be zoomed into: zoomed in, held a moment and eased out before the video ends.
    let in_time = |m: &Moment| m.start_us.saturating_sub(lead) + END_HOLD_US + ease <= input.duration_us;
    for m in moments.into_iter().filter(in_time) {
        // Zoomed in from `lead` before the activity until `hold` after it. The camera eases in before that and out
        // after (see below), so it's still while things happen rather than moving through them.
        let start = m.start_us.saturating_sub(lead);
        let end = (m.end_us + hold).min(input.duration_us.max(m.end_us));
        if let Some(last) = groups.last_mut() {
            let union = last.rect.union(m.rect);
            let fits = union.width <= max_framed.width && union.height <= max_framed.height;
            if start <= last.end + merge_gap && fits {
                last.end = last.end.max(end);
                last.rect = union;
                if !last.reasons.contains(&m.reason) {
                    last.reasons.push(m.reason);
                }
                last.weight = last.weight.max(m.weight);
                continue;
            }
        }
        groups.push(Group { start, end, rect: m.rect, reasons: vec![m.reason], weight: m.weight });
    }

    let min_len = u64::from(tuning.min_duration_ms) * 1_000;
    let max_len = u64::from(tuning.max_duration_ms) * 1_000;
    let aspect = input.capture.aspect();
    let mut regions: Vec<ZoomRegion> = Vec::new();
    for (index, g) in groups.into_iter().enumerate() {
        // Weak, lonely moments (a single dwell) don't earn a zoom.
        if g.weight < 0.45 && g.reasons.len() == 1 {
            continue;
        }
        let framed = g
            .rect
            .inflate(g.rect.width * tuning.margin / 2.0, g.rect.height * tuning.margin / 2.0)
            .expand_to_aspect(aspect);
        let scale =
            (input.capture.width / framed.width).min(input.capture.height / framed.height).min(max_scale);
        if scale < tuning.min_scale {
            continue;
        }
        let viewport = Rect::from_center(
            framed.center(),
            Size::new(input.capture.width / scale, input.capture.height / scale),
        )
        .clamp_inside(bounds);
        let (mut start, mut end) =
            (g.start.saturating_sub(ease), (g.end + ease).min(input.duration_us.max(g.end)));
        if end - start < min_len {
            let grow = (min_len - (end - start)) / 2;
            start = start.saturating_sub(grow);
            end = (end + grow).min(input.duration_us.max(end));
        }
        if end - start > max_len {
            end = start + max_len;
        }
        regions.push(ZoomRegion {
            id: format!("auto-{index}"),
            start_us: start,
            end_us: end,
            scale: (scale * 100.0).round() / 100.0,
            focus: viewport.center(),
            follow_mouse: None,
            ease_in_ms: transition_ms,
            ease_out_ms: transition_ms,
            source: ZoomSource::Auto,
            reason: Some(join_reasons(&g.reasons)),
        });
    }

    // No overlaps: a later zoom starts where the earlier one ends.
    for i in 1..regions.len() {
        if regions[i].start_us < regions[i - 1].end_us {
            regions[i - 1].end_us = regions[i].start_us;
        }
    }
    regions.retain(|r| r.end_us > r.start_us);
    regions
}

fn result_of_click(events: &[InputEvent], click_t: u64, capture: Size) -> Option<Rect> {
    let capture_area = capture.width * capture.height;
    events
        .iter()
        .filter_map(|e| match e {
            InputEvent::ScreenChange { t_us, region, fraction }
                if *t_us > click_t
                    && *t_us - click_t <= RESULT_WINDOW_US
                    && (0.002..=0.45).contains(fraction) =>
            {
                Some(*region)
            }
            _ => None,
        })
        .filter(|r| r.area() < capture_area * 0.45)
        .reduce(Rect::union)
}

fn dwells(cursor: &[CursorSample], min_us: u64) -> Vec<(u64, u64, Point)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < cursor.len() {
        let anchor = cursor[i].point();
        let mut j = i;
        while j + 1 < cursor.len() && cursor[j + 1].point().distance(anchor) <= DWELL_RADIUS {
            j += 1;
        }
        if cursor[j].t_us - cursor[i].t_us >= min_us {
            out.push((cursor[i].t_us, cursor[j].t_us, anchor));
        }
        i = j + 1;
    }
    out
}

fn describe(label: Option<&str>, role: &str) -> String {
    let role = role.replace('_', " ");
    match label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => label.to_string(),
        None => format!("the {role}"),
    }
}

fn join_reasons(reasons: &[String]) -> String {
    match reasons {
        [] => String::new(),
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first} (+{} more)", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{InputSource, MouseButton};

    const CAPTURE: Size = Size::new(1440.0, 900.0);

    fn click(t_ms: u64, x: f64, y: f64) -> InputEvent {
        InputEvent::MouseDown {
            t_us: t_ms * 1000,
            x,
            y,
            button: MouseButton::Left,
            clicks: 1,
            source: InputSource::Person,
        }
    }

    fn focus(t_ms: u64, bounds: Rect, label: &str) -> InputEvent {
        InputEvent::Focus { t_us: t_ms * 1000, bounds, role: "button".into(), label: Some(label.into()) }
    }

    fn key(t_ms: u64) -> InputEvent {
        InputEvent::KeyDown {
            t_us: t_ms * 1000,
            label: Some("a".into()),
            modifiers_only: false,
            redacted: false,
            source: InputSource::Person,
        }
    }

    fn plan(events: &[InputEvent]) -> Vec<ZoomRegion> {
        let input = ZoomInput { duration_us: 60_000_000, capture: CAPTURE, events, cursor: &[] };
        plan_zooms(&input, &ZoomTuning { dwell: false, ..Default::default() }, ZoomDepth::Standard, 700)
    }

    #[test]
    fn far_apart_clicks_make_separate_zooms_with_reasons() {
        let export = Rect::new(1200.0, 20.0, 90.0, 30.0);
        let events =
            [focus(1_000, export, "Export"), click(1_000, 1240.0, 35.0), click(20_000, 200.0, 700.0)];
        let zooms = plan(&events);
        assert_eq!(zooms.len(), 2);
        assert_eq!(zooms[0].reason.as_deref(), Some("Click on Export"));
        assert!(zooms[0].scale > 1.2 && zooms[0].scale <= ZoomDepth::Standard.max_scale());
        // The framed viewport stays inside the capture even for a corner control.
        for z in &zooms {
            let viewport =
                Rect::from_center(z.focus, Size::new(CAPTURE.width / z.scale, CAPTURE.height / z.scale));
            assert!(viewport.min_x() >= -1e-6 && viewport.max_x() <= CAPTURE.width + 1e-6);
            assert!(viewport.min_y() >= -1e-6 && viewport.max_y() <= CAPTURE.height + 1e-6);
        }
    }

    #[test]
    fn nearby_activity_merges_into_one_calm_zoom() {
        let events = [click(1_000, 400.0, 300.0), click(1_800, 460.0, 320.0), click(2_600, 420.0, 360.0)];
        let zooms = plan(&events);
        assert_eq!(zooms.len(), 1);
        assert!(zooms[0].reason.as_deref().unwrap().contains("Click"));
        assert!(zooms[0].end_us - zooms[0].start_us >= 1_400_000);
    }

    #[test]
    fn typing_zooms_can_be_turned_off() {
        let events: Vec<InputEvent> = (0..12).map(|i| key(5_000 + i * 120)).collect();
        let input = ZoomInput { duration_us: 60_000_000, capture: CAPTURE, events: &events, cursor: &[] };
        let tuning = ZoomTuning { dwell: false, typing: false, ..Default::default() };
        assert!(plan_zooms(&input, &tuning, ZoomDepth::Standard, 700).is_empty());
        assert_eq!(plan(&events).len(), 1, "on by default");
    }

    #[test]
    fn typing_frames_the_field() {
        let field = Rect::new(500.0, 400.0, 360.0, 32.0);
        let mut events = vec![InputEvent::Focus {
            t_us: 4_900_000,
            bounds: field,
            role: "text_field".into(),
            label: Some("Search".into()),
        }];
        events.extend((0..12).map(|i| key(5_000 + i * 120)));
        let zooms = plan(&events);
        assert_eq!(zooms.len(), 1);
        assert_eq!(zooms[0].reason.as_deref(), Some("Typing in Search"));
        let focus = zooms[0].focus;
        assert!(field.inflate(200.0, 200.0).contains(focus));
    }

    #[test]
    fn results_after_a_click_are_framed_too() {
        let button = Rect::new(100.0, 100.0, 80.0, 30.0);
        let events = [
            focus(1_000, button, "Convert"),
            click(1_000, 140.0, 115.0),
            InputEvent::ScreenChange {
                t_us: 1_400_000,
                region: Rect::new(100.0, 160.0, 300.0, 120.0),
                fraction: 0.03,
            },
        ];
        let zooms = plan(&events);
        assert_eq!(zooms[0].reason.as_deref(), Some("Click on Convert, then the result"));
    }

    #[test]
    fn only_what_happens_on_the_take_is_zoomed() {
        // The click on the recording pill, beside the recorded window, and one on another display.
        assert!(plan(&[click(5_000, -768.0, -1.0), click(9_000, 1700.0, 300.0)]).is_empty());
        assert_eq!(plan(&[click(5_000, -768.0, -1.0), click(9_000, 700.0, 300.0)]).len(), 1);
        // Typing into another app's field, off the take.
        let mut events = vec![InputEvent::Focus {
            t_us: 4_900_000,
            bounds: Rect::new(1600.0, 400.0, 360.0, 32.0),
            role: "text_field".into(),
            label: Some("Search".into()),
        }];
        events.extend((0..12).map(|i| key(5_000 + i * 120)));
        assert!(plan(&events).is_empty());
        // Typing with no field known and the pointer off the take.
        let cursor =
            [CursorSample { t_us: 0, x: -200.0, y: 450.0, shape: crate::telemetry::CursorShape::Arrow }];
        let keys: Vec<InputEvent> = (0..12).map(|i| key(5_000 + i * 120)).collect();
        let input = ZoomInput { duration_us: 60_000_000, capture: CAPTURE, events: &keys, cursor: &cursor };
        let tuning = ZoomTuning { dwell: false, ..Default::default() };
        assert!(plan_zooms(&input, &tuning, ZoomDepth::Standard, 700).is_empty());
        // The pointer resting off the take.
        let resting = [
            CursorSample {
                t_us: 1_000_000,
                x: 1600.0,
                y: 450.0,
                shape: crate::telemetry::CursorShape::Arrow,
            },
            CursorSample {
                t_us: 6_000_000,
                x: 1601.0,
                y: 450.0,
                shape: crate::telemetry::CursorShape::Arrow,
            },
        ];
        let input = ZoomInput { duration_us: 60_000_000, capture: CAPTURE, events: &[], cursor: &resting };
        let typing_and_dwell = ZoomTuning { dwell: true, dwell_ms: 1_000, ..Default::default() };
        assert!(find_moments(&input, &typing_and_dwell).is_empty());
    }

    #[test]
    fn a_click_just_before_the_end_isnt_zoomed() {
        let plan_in = |events: &[InputEvent]| {
            let input = ZoomInput { duration_us: 36_655_896, capture: CAPTURE, events, cursor: &[] };
            plan_zooms(&input, &ZoomTuning { dwell: false, ..Default::default() }, ZoomDepth::Standard, 700)
        };
        // The owner's take: the click that stopped it came 0.12 s before the end.
        assert!(plan_in(&[click(36_540, 700.0, 300.0)]).is_empty());
        // One that leaves time to push in and hold is.
        assert_eq!(plan_in(&[click(35_000, 700.0, 300.0)]).len(), 1);
    }

    #[test]
    fn zooms_never_overlap() {
        let events: Vec<_> = (0..20)
            .map(|i| click(i * 2_500, if i % 2 == 0 { 100.0 } else { 1300.0 }, 100.0 + i as f64 * 30.0))
            .collect();
        let zooms = plan(&events);
        for pair in zooms.windows(2) {
            assert!(pair[1].start_us >= pair[0].end_us);
        }
    }
}
