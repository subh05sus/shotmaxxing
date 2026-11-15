//! Input telemetry: the cursor path, clicks, scrolls and shortcuts, plus which control each click landed on.
//!
//! Three small threads, each idle most of the time:
//! - a cursor sampler that reads the pointer position at up to 120 Hz and keeps only changes;
//! - a listen-only event tap for clicks, scrolls and keys (it can observe, never alter, input);
//! - an accessibility worker that asks what was clicked, off the tap's thread so input is never delayed.
//!
//! Privacy: typed characters are never stored. Plain typing is recorded as "a key was pressed" so auto zoom can
//! notice typing; only shortcuts with ⌘ or ⌃ keep a label such as "⌘K". Password fields switch macOS into secure
//! input, which hides keystrokes from every app, including this one.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CGRect, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use shotmaxxing_core::geometry::Rect;
use shotmaxxing_core::telemetry::{
    AGENT_EVENT_MARK, CursorSample, CursorShape, InputEvent, InputSource, MouseButton,
};

use super::ax::{
    AX_VALUE_CGPOINT, AX_VALUE_CGSIZE, AXIsProcessTrusted, AXUIElementCopyElementAtPosition,
    AXUIElementCreateSystemWide, AXUIElementSetMessagingTimeout, CFRelease, Pair, geometry_attribute,
    string_attribute,
};
use super::{SendBox, clock, window};

/// A raw observation, stamped with host seconds.
#[derive(Clone, Debug)]
enum Raw {
    Down { at: f64, x: f64, y: f64, button: MouseButton, clicks: u8, source: InputSource },
    Up { at: f64, x: f64, y: f64, button: MouseButton, source: InputSource },
    Scroll { at: f64, x: f64, y: f64, dx: f64, dy: f64 },
    Key { at: f64, label: Option<String>, source: InputSource },
    Focus { at: f64, bounds: Rect, role: String, label: Option<String> },
}

struct TapState {
    events: Mutex<Vec<Raw>>,
    clicks: Mutex<Sender<(f64, f64, f64)>>,
    /// Processes whose windows are never in a take (Shotmaxxing's own).
    left_out: Vec<i32>,
    /// Whether the left and the right button's last press was on one of those windows, so its release is left out too.
    dropped: [AtomicBool; 2],
}

/// Where the captured picture's top-left corner is on the desktop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Origin {
    /// A screen's, which stays put.
    Fixed(f64, f64),
    /// A window's, which moves with it. `start` is the recorded area's corner when the take began and `size` the
    /// take's size in points: the window's real size, with a full-screen app's toolbars `top` points above it. The
    /// window is followed through the take, readings taken while it's a Stage Manager thumbnail are skipped, and while
    /// it's another size, positions are scaled as the video shows it then: the whole window, fitted into the take.
    Window { id: u32, start: (f64, f64), size: (f64, f64), top: f64 },
}

/// How a point on the desktop lands in the take: measured from the recorded area's corner, and scaled when the area
/// isn't the take's size (a window resized during it), as ScreenCaptureKit draws a source rect into a stream of
/// another size: the whole area, fitted and centred.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    x: f64,
    y: f64,
    scale: f64,
    dx: f64,
    dy: f64,
}

impl Fit {
    /// The take's own size, with its corner at `x`, `y`.
    fn at((x, y): (f64, f64)) -> Self {
        Self { x, y, scale: 1.0, dx: 0.0, dy: 0.0 }
    }

    /// An area shown in a take of `size`.
    fn new(area: CGRect, size: (f64, f64)) -> Self {
        let (w, h) = (area.size.width, area.size.height);
        let corner = (area.origin.x, area.origin.y);
        // The same size, to the window server's rounding, isn't scaled.
        if w <= 0.0 || h <= 0.0 || window::at_real_size(area, size) {
            return Self::at(corner);
        }
        let scale = (size.0 / w).min(size.1 / h);
        Self {
            x: corner.0,
            y: corner.1,
            scale,
            dx: (size.0 - w * scale) / 2.0,
            dy: (size.1 - h * scale) / 2.0,
        }
    }

    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.dx + (x - self.x) * self.scale, self.dy + (y - self.y) * self.scale)
    }
}

pub struct Telemetry {
    stop: Arc<AtomicBool>,
    origin: Origin,
    /// Where a recorded window's area was, and how big, when it changed.
    places: Option<JoinHandle<Vec<(f64, CGRect)>>>,
    cursor: Option<JoinHandle<Vec<(f64, f64, f64)>>>,
    tap_loop: Option<SendBox<CFRetained<CFRunLoop>>>,
    tap_thread: Option<JoinHandle<()>>,
    focus_thread: Option<JoinHandle<Vec<Raw>>>,
    state: Arc<TapState>,
}

impl Telemetry {
    pub fn start(origin: Origin, _started: f64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));

        let places = match origin {
            Origin::Window { id, size, top, .. } => {
                let follow_stop = stop.clone();
                std::thread::Builder::new()
                    .name("smx-window".into())
                    .spawn(move || follow_window(id, size, top, &follow_stop))
                    .ok()
            }
            Origin::Fixed(..) => None,
        };

        let cursor_stop = stop.clone();
        let cursor = std::thread::Builder::new()
            .name("smx-cursor".into())
            .spawn(move || sample_cursor(&cursor_stop))
            .ok();

        let (click_tx, click_rx) = channel::<(f64, f64, f64)>();
        let state = Arc::new(TapState {
            events: Mutex::new(Vec::new()),
            clicks: Mutex::new(click_tx),
            left_out: window::left_out_processes(),
            dropped: [AtomicBool::new(false), AtomicBool::new(false)],
        });

        let focus_thread =
            std::thread::Builder::new().name("smx-focus".into()).spawn(move || resolve_focus(click_rx)).ok();

        let (loop_tx, loop_rx) = std::sync::mpsc::sync_channel::<Option<SendBox<CFRetained<CFRunLoop>>>>(1);
        let tap_state = state.clone();
        let tap_thread = std::thread::Builder::new()
            .name("smx-input".into())
            .spawn(move || run_tap(tap_state, loop_tx))
            .ok();
        let tap_loop = loop_rx.recv_timeout(Duration::from_secs(2)).ok().flatten();

        Self { stop, origin, places, cursor, tap_loop, tap_thread, focus_thread, state }
    }

    /// Stops every thread and returns telemetry in microseconds from `zero` (the first video frame), in points
    /// relative to the captured area.
    pub fn stop(mut self, zero: f64) -> (Vec<CursorSample>, Vec<InputEvent>) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(run_loop) = self.tap_loop.take() {
            run_loop.0.stop();
        }
        if let Some(thread) = self.tap_thread.take() {
            let _ = thread.join();
        }
        // Closing the click channel ends the focus worker.
        if let Ok(mut sender) = self.state.clicks.lock() {
            *sender = channel().0;
        }
        let focus = self.focus_thread.take().and_then(|t| t.join().ok()).unwrap_or_default();
        let path = self.cursor.take().and_then(|t| t.join().ok()).unwrap_or_default();
        let places = self.places.take().and_then(|t| t.join().ok()).unwrap_or_default();
        let (start, size) = match self.origin {
            Origin::Fixed(x, y) => ((x, y), None),
            Origin::Window { start, size, .. } => (start, Some(size)),
        };
        // How points land in the take at `at`: by the last place seen by then, or, before the window first showed,
        // the first place it did.
        let fit_at = |at: f64| {
            let place = match places.iter().rposition(|(t, _)| *t <= at) {
                Some(i) => Some(places[i].1),
                None => places.first().map(|&(_, area)| area),
            };
            match (place, size) {
                (Some(area), Some(size)) => Fit::new(area, size),
                _ => Fit::at(start),
            }
        };
        let t_us = |at: f64| ((at - zero).max(0.0) * 1e6) as u64;
        // The pointer's resting place when the first frame arrived counts as its position at 0.
        let before = path.iter().rposition(|(at, _, _)| *at < zero);
        let cursor = path
            .into_iter()
            .enumerate()
            .filter(|(i, (at, _, _))| *at >= zero || Some(*i) == before)
            .map(|(_, (at, x, y))| {
                let (x, y) = fit_at(at).point(x, y);
                CursorSample { t_us: t_us(at), x, y, shape: CursorShape::Arrow }
            })
            .collect();

        let mut raw = self.state.events.lock().map(|e| e.clone()).unwrap_or_default();
        raw.extend(focus);
        raw.sort_by(|a, b| at_of(a).total_cmp(&at_of(b)));
        let events = raw
            .into_iter()
            .filter(|r| at_of(r) >= zero)
            .map(|r| (fit_at(at_of(&r)), r))
            .map(|(fit, r)| match r {
                Raw::Down { at, x, y, button, clicks, source } => {
                    let (x, y) = fit.point(x, y);
                    InputEvent::MouseDown { t_us: t_us(at), x, y, button, clicks, source }
                }
                Raw::Up { at, x, y, button, source } => {
                    let (x, y) = fit.point(x, y);
                    InputEvent::MouseUp { t_us: t_us(at), x, y, button, source }
                }
                Raw::Scroll { at, x, y, dx, dy } => {
                    let (x, y) = fit.point(x, y);
                    InputEvent::Scroll { t_us: t_us(at), x, y, dx, dy }
                }
                Raw::Key { at, label, source } => InputEvent::KeyDown {
                    t_us: t_us(at),
                    modifiers_only: false,
                    redacted: label.is_none(),
                    label,
                    source,
                },
                Raw::Focus { at, bounds, role, label } => {
                    let (x, y) = fit.point(bounds.x, bounds.y);
                    InputEvent::Focus {
                        t_us: t_us(at),
                        bounds: Rect::new(x, y, bounds.width * fit.scale, bounds.height * fit.scale),
                        role,
                        label,
                    }
                }
            })
            .collect();
        (cursor, events)
    }
}

/// Where a recorded window's area is whenever it changes, read ten times a second until `stop`: the window's frame,
/// with a full-screen app's toolbars `top` points above it, while it's showing. The window server's frame for a
/// window parked in Stage Manager is the thumbnail's (of a window of the take's `size`), so those readings are
/// skipped, as the video skips them; one moved or resized is followed, as the video follows it.
fn follow_window(id: u32, size: (f64, f64), top: f64, stop: &AtomicBool) -> Vec<(f64, CGRect)> {
    let (stage_manager, displays) = (window::stage_manager_on(), window::displays());
    let mut places: Vec<(f64, CGRect)> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        if let Some(placed) = window::placed(id)
            && placed.on_screen
            && !(stage_manager && window::thumbnail_of(placed.bounds, size))
        {
            let area = window::reach(placed.bounds, top, &displays);
            if places.last().is_none_or(|&(_, last)| changed(last, area)) {
                places.push((clock::now(), area));
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    places
}

/// Whether an area has moved or changed size, by more than a quarter of a point.
fn changed(a: CGRect, b: CGRect) -> bool {
    (a.origin.x - b.origin.x).abs() > 0.25
        || (a.origin.y - b.origin.y).abs() > 0.25
        || (a.size.width - b.size.width).abs() > 0.25
        || (a.size.height - b.size.height).abs() > 0.25
}

fn at_of(raw: &Raw) -> f64 {
    match raw {
        Raw::Down { at, .. }
        | Raw::Up { at, .. }
        | Raw::Scroll { at, .. }
        | Raw::Key { at, .. }
        | Raw::Focus { at, .. } => *at,
    }
}

/// Reads the pointer position at up to 120 Hz, keeping only positions that changed.
fn sample_cursor(stop: &AtomicBool) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::with_capacity(4096);
    let mut last: Option<(f64, f64)> = None;
    while !stop.load(Ordering::Relaxed) {
        if let Some(event) = CGEvent::new(None) {
            let p = CGEvent::location(Some(&event));
            if last.is_none_or(|(x, y)| (p.x - x).abs() > 0.05 || (p.y - y).abs() > 0.05) {
                out.push((clock::now(), p.x, p.y));
                last = Some((p.x, p.y));
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    out
}

fn mask(kinds: &[CGEventType]) -> CGEventMask {
    kinds.iter().fold(0, |m, k| m | (1 << k.0))
}

unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: `user_info` is the `TapState` pointer we passed in, kept alive by an Arc for the tap's lifetime, and
    // `event` is valid for the callback.
    let (state, event) = unsafe { (&*(user_info as *const TapState), event.as_ref()) };
    let at = clock::now();
    let p = CGEvent::location(Some(event));
    // Events a take performs carry Shotmaxxing's mark; everything else is the person.
    let source =
        if CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == AGENT_EVENT_MARK {
            InputSource::Agent
        } else {
            InputSource::Person
        };
    let raw = match kind {
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown => {
            let right = kind == CGEventType::RightMouseDown;
            // Shotmaxxing's own windows (the recording pill, its menu-bar icon's menu) are never in a take, so a
            // click on them isn't either: it would ripple on, and be zoomed into, nothing.
            let ours = window::owner_at(p).is_some_and(|pid| state.left_out.contains(&pid));
            state.dropped[usize::from(right)].store(ours, Ordering::Relaxed);
            if ours {
                None
            } else {
                let clicks = CGEvent::integer_value_field(Some(event), CGEventField::MouseEventClickState)
                    .clamp(1, 3) as u8;
                let button = if right { MouseButton::Right } else { MouseButton::Left };
                if let Ok(sender) = state.clicks.lock() {
                    let _ = sender.send((at, p.x, p.y));
                }
                Some(Raw::Down { at, x: p.x, y: p.y, button, clicks, source })
            }
        }
        CGEventType::LeftMouseUp | CGEventType::RightMouseUp => {
            let right = kind == CGEventType::RightMouseUp;
            if state.dropped[usize::from(right)].swap(false, Ordering::Relaxed) {
                None
            } else {
                let button = if right { MouseButton::Right } else { MouseButton::Left };
                Some(Raw::Up { at, x: p.x, y: p.y, button, source })
            }
        }
        CGEventType::ScrollWheel => {
            let dy = CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventPointDeltaAxis1)
                as f64;
            let dx = CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventPointDeltaAxis2)
                as f64;
            Some(Raw::Scroll { at, x: p.x, y: p.y, dx, dy })
        }
        CGEventType::KeyDown => Some(Raw::Key { at, label: shortcut_label(event), source }),
        // macOS turns a tap off if it ever runs slowly; switch it straight back on.
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => None,
        _ => None,
    };
    if let Some(raw) = raw
        && let Ok(mut events) = state.events.lock()
    {
        events.push(raw);
    }
    event as *const CGEvent as *mut CGEvent
}

/// "⌘K" for shortcuts; `None` (redacted) for plain typing.
fn shortcut_label(event: &CGEvent) -> Option<String> {
    let flags = CGEvent::flags(Some(event));
    let command = flags.contains(CGEventFlags::MaskCommand);
    let control = flags.contains(CGEventFlags::MaskControl);
    let keycode = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    let named = match keycode {
        36 => Some("↩"),
        48 => Some("⇥"),
        49 => Some("Space"),
        51 => Some("⌫"),
        53 => Some("esc"),
        123 => Some("←"),
        124 => Some("→"),
        125 => Some("↓"),
        126 => Some("↑"),
        _ => None,
    };
    if !command && !control {
        // Navigation keys are worth showing; letters and digits stay private.
        return named.filter(|n| *n != "Space").map(str::to_string);
    }
    let mut label = String::new();
    if control {
        label.push('⌃');
    }
    if flags.contains(CGEventFlags::MaskAlternate) {
        label.push('⌥');
    }
    if flags.contains(CGEventFlags::MaskShift) {
        label.push('⇧');
    }
    if command {
        label.push('⌘');
    }
    match named {
        Some(name) => label.push_str(name),
        None => label.push_str(&key_name(keycode)?),
    }
    Some(label)
}

/// Letters and digits for the common ANSI key codes, used only for shortcut labels.
fn key_name(keycode: i64) -> Option<String> {
    const KEYS: [(i64, &str); 36] = [
        (0, "A"),
        (11, "B"),
        (8, "C"),
        (2, "D"),
        (14, "E"),
        (3, "F"),
        (5, "G"),
        (4, "H"),
        (34, "I"),
        (38, "J"),
        (40, "K"),
        (37, "L"),
        (46, "M"),
        (45, "N"),
        (31, "O"),
        (35, "P"),
        (12, "Q"),
        (15, "R"),
        (1, "S"),
        (17, "T"),
        (32, "U"),
        (9, "V"),
        (13, "W"),
        (7, "X"),
        (16, "Y"),
        (6, "Z"),
        (29, "0"),
        (18, "1"),
        (19, "2"),
        (20, "3"),
        (21, "4"),
        (23, "5"),
        (22, "6"),
        (26, "7"),
        (28, "8"),
        (25, "9"),
    ];
    KEYS.iter().find(|(code, _)| *code == keycode).map(|(_, name)| (*name).to_string())
}

fn run_tap(state: Arc<TapState>, ready: std::sync::mpsc::SyncSender<Option<SendBox<CFRetained<CFRunLoop>>>>) {
    let kinds = [
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::ScrollWheel,
        CGEventType::KeyDown,
    ];
    let user_info = Arc::as_ptr(&state) as *mut c_void;
    // SAFETY: listen-only tap; the callback only reads the event, and `state` outlives the run loop below.
    let tap: Option<CFRetained<CFMachPort>> = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            mask(&kinds),
            Some(tap_callback),
            user_info,
        )
    };
    let Some(tap) = tap else {
        // No Input Monitoring or Accessibility permission: record the cursor path only.
        tracing::info!("input tap unavailable; recording the cursor path only");
        let _ = ready.send(None);
        return;
    };
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
        let _ = ready.send(None);
        return;
    };
    let Some(run_loop) = CFRunLoop::current() else {
        let _ = ready.send(None);
        return;
    };
    // SAFETY: kCFRunLoopCommonModes is an immutable framework constant.
    let modes = unsafe { kCFRunLoopCommonModes };
    run_loop.add_source(Some(&source), modes);
    CGEvent::tap_enable(&tap, true);
    let _ = ready.send(Some(SendBox(run_loop.clone())));
    CFRunLoop::run();
    CGEvent::tap_enable(&tap, false);
    drop(state);
}

fn friendly_role(role: &str) -> String {
    let plain = role.strip_prefix("AX").unwrap_or(role);
    match plain {
        "Button" | "PopUpButton" | "MenuButton" => "button".into(),
        "TextField" | "TextArea" | "SearchField" | "ComboBox" => "text field".into(),
        "Link" => "link".into(),
        "CheckBox" => "checkbox".into(),
        "RadioButton" => "option".into(),
        "MenuItem" | "MenuBarItem" => "menu item".into(),
        "Tab" | "TabGroup" => "tab".into(),
        "Cell" | "Row" => "row".into(),
        "Slider" => "slider".into(),
        "Image" => "image".into(),
        other => other.to_lowercase(),
    }
}

fn resolve_focus(clicks: Receiver<(f64, f64, f64)>) -> Vec<Raw> {
    let mut out = Vec::new();
    if !AXIsProcessTrusted() {
        // Without Accessibility, clicks still zoom; they just can't be named.
        while clicks.recv().is_ok() {}
        return out;
    }
    // SAFETY: creating the system-wide element; released at the end.
    let system = unsafe { AXUIElementCreateSystemWide() };
    // SAFETY: a short timeout so a frozen app can't stall the worker.
    unsafe { AXUIElementSetMessagingTimeout(system, 0.25) };
    while let Ok((at, x, y)) = clicks.recv() {
        let mut element: *const c_void = std::ptr::null();
        // SAFETY: valid system element; the out pointer is ours.
        let status = unsafe { AXUIElementCopyElementAtPosition(system, x as f32, y as f32, &mut element) };
        if status != 0 || element.is_null() {
            continue;
        }
        let role = string_attribute(element, "AXRole")
            .map(|r| friendly_role(&r))
            .unwrap_or_else(|| "control".into());
        let label = string_attribute(element, "AXTitle")
            .or_else(|| string_attribute(element, "AXDescription"))
            .or_else(|| string_attribute(element, "AXPlaceholderValue"))
            .or_else(|| string_attribute(element, "AXHelp"));
        let position: Option<Pair> = geometry_attribute(element, "AXPosition", AX_VALUE_CGPOINT);
        let size: Option<Pair> = geometry_attribute(element, "AXSize", AX_VALUE_CGSIZE);
        // SAFETY: releasing the element we copied.
        unsafe { CFRelease(element) };
        if let (Some(p), Some(s)) = (position, size)
            && s.a > 0.0
            && s.b > 0.0
        {
            out.push(Raw::Focus { at, bounds: Rect::new(p.a, p.b, s.a, s.b), role, label });
        }
    }
    // SAFETY: releasing the system-wide element.
    unsafe { CFRelease(system) };
    out
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGSize};

    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn points_are_measured_from_the_area_at_the_takes_size() {
        let fit = Fit::new(rect(300.0, 200.0, 900.0, 450.0), (900.0, 450.0));
        assert_eq!(fit.point(450.0, 300.0), (150.0, 100.0));
        // The window server's rounding isn't a resize.
        assert_eq!(Fit::new(rect(300.0, 200.0, 901.0, 450.5), (900.0, 450.0)).scale, 1.0);
    }

    #[test]
    fn a_window_that_grew_is_scaled_into_the_take() {
        // A take that started on a window still coming out of Stage Manager's strip, at 399 × 201 points, of a
        // window of 895 × 450: its clicks land where the video shows them, not hundreds of points outside it.
        let fit = Fit::new(rect(-300.0, 100.0, 798.0, 402.0), (399.0, 201.0));
        assert_eq!(fit.scale, 0.5);
        assert_eq!(fit.point(-300.0, 100.0), (0.0, 0.0));
        assert_eq!(fit.point(498.0, 502.0), (399.0, 201.0));
        // Another shape is fitted and centred: twice as wide, so it's shown with bands above and below.
        let fit = Fit::new(rect(0.0, 0.0, 1600.0, 400.0), (800.0, 400.0));
        assert_eq!(fit.scale, 0.5);
        assert_eq!(fit.point(0.0, 0.0), (0.0, 100.0));
        assert_eq!(fit.point(1600.0, 400.0), (800.0, 300.0));
    }
}
