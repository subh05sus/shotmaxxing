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

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CFString, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use shotmaxxing_core::geometry::Rect;
use shotmaxxing_core::telemetry::{CursorSample, CursorShape, InputEvent, InputSource, MouseButton};

use super::{SendBox, clock};

/// A raw observation, stamped with host seconds.
#[derive(Clone, Debug)]
enum Raw {
    Down { at: f64, x: f64, y: f64, button: MouseButton, clicks: u8 },
    Up { at: f64, x: f64, y: f64, button: MouseButton },
    Scroll { at: f64, x: f64, y: f64, dx: f64, dy: f64 },
    Key { at: f64, label: Option<String> },
    Focus { at: f64, bounds: Rect, role: String, label: Option<String> },
}

struct TapState {
    events: Mutex<Vec<Raw>>,
    clicks: Mutex<Sender<(f64, f64, f64)>>,
}

pub struct Telemetry {
    stop: Arc<AtomicBool>,
    origin: (f64, f64),
    cursor: Option<JoinHandle<Vec<(f64, f64, f64)>>>,
    tap_loop: Option<SendBox<CFRetained<CFRunLoop>>>,
    tap_thread: Option<JoinHandle<()>>,
    focus_thread: Option<JoinHandle<Vec<Raw>>>,
    state: Arc<TapState>,
}

impl Telemetry {
    pub fn start(origin: (f64, f64), _started: f64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));

        let cursor_stop = stop.clone();
        let cursor = std::thread::Builder::new()
            .name("smx-cursor".into())
            .spawn(move || sample_cursor(&cursor_stop))
            .ok();

        let (click_tx, click_rx) = channel::<(f64, f64, f64)>();
        let state = Arc::new(TapState { events: Mutex::new(Vec::new()), clicks: Mutex::new(click_tx) });

        let focus_thread =
            std::thread::Builder::new().name("smx-focus".into()).spawn(move || resolve_focus(click_rx)).ok();

        let (loop_tx, loop_rx) = std::sync::mpsc::sync_channel::<Option<SendBox<CFRetained<CFRunLoop>>>>(1);
        let tap_state = state.clone();
        let tap_thread = std::thread::Builder::new()
            .name("smx-input".into())
            .spawn(move || run_tap(tap_state, loop_tx))
            .ok();
        let tap_loop = loop_rx.recv_timeout(Duration::from_secs(2)).ok().flatten();

        Self { stop, origin, cursor, tap_loop, tap_thread, focus_thread, state }
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

        let (ox, oy) = self.origin;
        let t_us = |at: f64| ((at - zero).max(0.0) * 1e6) as u64;
        let cursor = path
            .into_iter()
            .filter(|(at, _, _)| *at >= zero)
            .map(|(at, x, y)| CursorSample {
                t_us: t_us(at),
                x: x - ox,
                y: y - oy,
                shape: CursorShape::Arrow,
            })
            .collect();

        let mut raw = self.state.events.lock().map(|e| e.clone()).unwrap_or_default();
        raw.extend(focus);
        raw.sort_by(|a, b| at_of(a).total_cmp(&at_of(b)));
        let events = raw
            .into_iter()
            .filter(|r| at_of(r) >= zero)
            .map(|r| match r {
                Raw::Down { at, x, y, button, clicks } => InputEvent::MouseDown {
                    t_us: t_us(at),
                    x: x - ox,
                    y: y - oy,
                    button,
                    clicks,
                    source: InputSource::Person,
                },
                Raw::Up { at, x, y, button } => InputEvent::MouseUp {
                    t_us: t_us(at),
                    x: x - ox,
                    y: y - oy,
                    button,
                    source: InputSource::Person,
                },
                Raw::Scroll { at, x, y, dx, dy } => {
                    InputEvent::Scroll { t_us: t_us(at), x: x - ox, y: y - oy, dx, dy }
                }
                Raw::Key { at, label } => InputEvent::KeyDown {
                    t_us: t_us(at),
                    modifiers_only: false,
                    redacted: label.is_none(),
                    label,
                    source: InputSource::Person,
                },
                Raw::Focus { at, bounds, role, label } => InputEvent::Focus {
                    t_us: t_us(at),
                    bounds: Rect::new(bounds.x - ox, bounds.y - oy, bounds.width, bounds.height),
                    role,
                    label,
                },
            })
            .collect();
        (cursor, events)
    }
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
    let mut last = (f64::NAN, f64::NAN);
    while !stop.load(Ordering::Relaxed) {
        if let Some(event) = CGEvent::new(None) {
            let p = CGEvent::location(Some(&event));
            if (p.x - last.0).abs() > 0.05 || (p.y - last.1).abs() > 0.05 {
                out.push((clock::now(), p.x, p.y));
                last = (p.x, p.y);
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
    let raw = match kind {
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown => {
            let clicks = CGEvent::integer_value_field(Some(event), CGEventField::MouseEventClickState)
                .clamp(1, 3) as u8;
            let button =
                if kind == CGEventType::LeftMouseDown { MouseButton::Left } else { MouseButton::Right };
            if let Ok(sender) = state.clicks.lock() {
                let _ = sender.send((at, p.x, p.y));
            }
            Some(Raw::Down { at, x: p.x, y: p.y, button, clicks })
        }
        CGEventType::LeftMouseUp | CGEventType::RightMouseUp => {
            let button =
                if kind == CGEventType::LeftMouseUp { MouseButton::Left } else { MouseButton::Right };
            Some(Raw::Up { at, x: p.x, y: p.y, button })
        }
        CGEventType::ScrollWheel => {
            let dy = CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventPointDeltaAxis1)
                as f64;
            let dx = CGEvent::integer_value_field(Some(event), CGEventField::ScrollWheelEventPointDeltaAxis2)
                as f64;
            Some(Raw::Scroll { at, x: p.x, y: p.y, dx, dy })
        }
        CGEventType::KeyDown => Some(Raw::Key { at, label: shortcut_label(event) }),
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

// Accessibility: which control is under a click.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> *const c_void;
    fn AXUIElementCopyElementAtPosition(
        app: *const c_void,
        x: f32,
        y: f32,
        element: *mut *const c_void,
    ) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
}

const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;

/// Copies one attribute. The caller releases the result.
fn copy_attribute(element: *const c_void, name: &str) -> Option<*const c_void> {
    let attribute = CFString::from_str(name);
    let mut value: *const c_void = std::ptr::null();
    let attribute_ptr = CFRetained::as_ptr(&attribute).as_ptr() as *const c_void;
    // SAFETY: valid element and attribute; the out pointer is ours.
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute_ptr, &mut value) };
    (status == 0 && !value.is_null()).then_some(value)
}

fn string_attribute(element: *const c_void, name: &str) -> Option<String> {
    let value = copy_attribute(element, name)?;
    // SAFETY: we own `value` (a Copy rule result) and release it after reading.
    unsafe {
        let text = if CFGetTypeID(value) == CFStringGetTypeID() {
            let string: &CFString = &*(value as *const CFString);
            Some(string.to_string())
        } else {
            None
        };
        CFRelease(value);
        text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && t.len() < 120)
    }
}

fn geometry_attribute<T: Default>(element: *const c_void, name: &str, kind: u32) -> Option<T> {
    let value = copy_attribute(element, name)?;
    let mut out = T::default();
    // SAFETY: AXValueGetValue writes a CGPoint or CGSize (matching `kind` and `T`) into `out`.
    let ok = unsafe { AXValueGetValue(value, kind, &mut out as *mut T as *mut c_void) };
    // SAFETY: releasing the value we copied.
    unsafe { CFRelease(value) };
    ok.then_some(out)
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Pair {
    a: f64,
    b: f64,
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
