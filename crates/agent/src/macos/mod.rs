//! The macOS desk: input through CGEvent, controls through the accessibility API, and the recording around a take.

mod ax;
mod keys;

use std::ffi::c_void;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use objc2_core_foundation::{CFRetained, CFString, CGPoint};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit, CGWarpMouseCursorPosition,
};
use shotmaxxing_capture::{ProjectOptions, ProjectRecording, Target, WindowInfo};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::{Point, Rect};
use shotmaxxing_core::keys::KeyChord;
use shotmaxxing_core::project::{AgentInfo, TextOverlay, TimeRange};
use shotmaxxing_core::refs::ControlSnapshot;
use shotmaxxing_core::take::{Framing, Severity, TakeScript};
use shotmaxxing_core::telemetry::{AGENT_EVENT_MARK, MouseButton};

use crate::{AgentError, Desk, Performed, Prepared, Progress, Take, TakeOptions};

const SCREEN_PERMISSION: &str = "Shotmaxxing needs the Screen Recording permission to film a take. Allow it in \
System Settings → Privacy & Security → Screen & System Audio Recording for the app that started Shotmaxxing (your \
terminal or agent app), then quit and reopen that app.";
const ACCESSIBILITY_PERMISSION: &str = "Shotmaxxing needs the Accessibility permission to read controls, move the \
pointer and type. Allow it in System Settings → Privacy & Security → Accessibility for the app that started \
Shotmaxxing (your terminal or agent app), then quit and reopen that app.";

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    safe fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: *const c_void, kind: isize, out: *mut c_void) -> bool;
    fn CFRelease(object: *const c_void);
}

const ON_SCREEN_ONLY: u32 = 1;
const INCLUDING_WINDOW: u32 = 1 << 3;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const CF_NUMBER_SINT32: isize = 3;

/// Reads a whole-number field from the window list's entries, for the windows `option` selects.
fn window_list(option: u32, window: u32) -> Vec<(i32, i32)> {
    let (pid_key, layer_key) =
        (CFString::from_str("kCGWindowOwnerPID"), CFString::from_str("kCGWindowLayer"));
    let key = |s: &CFRetained<CFString>| CFRetained::as_ptr(s).as_ptr() as *const c_void;
    let number = |dict: *const c_void, name: *const c_void| -> Option<i32> {
        let mut out = 0i32;
        // SAFETY: `dict` is a window-info dictionary; the value, when present, is a CFNumber.
        unsafe {
            let value = CFDictionaryGetValue(dict, name);
            (!value.is_null()
                && CFNumberGetValue(value, CF_NUMBER_SINT32, &mut out as *mut i32 as *mut c_void))
            .then_some(out)
        }
    };
    // SAFETY: the window list is an owned CFArray of dictionaries, released at the end.
    unsafe {
        let list = CGWindowListCopyWindowInfo(option, window);
        if list.is_null() {
            return Vec::new();
        }
        let out = (0..CFArrayGetCount(list))
            .map(|i| CFArrayGetValueAtIndex(list, i))
            .filter_map(|dict| {
                Some((number(dict, key(&pid_key))?, number(dict, key(&layer_key)).unwrap_or(0)))
            })
            .collect();
        CFRelease(list);
        out
    }
}

/// The process that owns a window.
fn window_owner(window: u32) -> Option<i32> {
    window_list(INCLUDING_WINDOW, window).first().map(|(pid, _)| *pid)
}

/// The app in front: the owner of the topmost ordinary window.
fn frontmost_pid() -> Option<i32> {
    window_list(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0)
        .into_iter()
        .find(|(_, layer)| *layer == 0)
        .map(|(pid, _)| pid)
}

fn pointer() -> Point {
    CGEvent::new(None).map_or(Point::ZERO, |event| {
        let p = CGEvent::location(Some(&event));
        Point::new(p.x, p.y)
    })
}

fn bounds(window: &WindowInfo) -> Rect {
    Rect::new(window.x, window.y, window.width, window.height)
}

/// Opens the app (or brings it forward) and finds its window: the largest, or the one whose title contains `title`.
fn locate(app: Option<&str>, title: Option<&str>) -> Result<(Option<i32>, Option<WindowInfo>), AgentError> {
    let Some(app) = app.map(str::trim).filter(|a| !a.is_empty()) else {
        return Ok((frontmost_pid(), None));
    };
    let bundle_id = app.contains('.') && !app.contains(' ') && !app.ends_with(".app");
    let opened = Command::new("/usr/bin/open").arg(if bundle_id { "-b" } else { "-a" }).arg(app).output();
    if !opened.is_ok_and(|o| o.status.success()) {
        return Err(AgentError::App(format!(
            "couldn’t open {app}. Use the name it has in the Applications folder, or its bundle id"
        )));
    }
    let needle = app.to_lowercase();
    let title = title.map(str::to_lowercase);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let window = shotmaxxing_capture::sources()?
            .windows
            .into_iter()
            .filter(|w| w.app.to_lowercase().contains(&needle) || w.bundle_id.to_lowercase() == needle)
            .filter(|w| title.as_ref().is_none_or(|t| w.title.to_lowercase().contains(t)))
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)));
        if let Some(window) = window {
            let pid = window_owner(window.id);
            if let Some(tree) = pid.and_then(|pid| ax::Tree::new(pid, Some(bounds(&window)))) {
                tree.raise();
                // Give the window a moment to come forward before anything looks at it.
                std::thread::sleep(Duration::from_millis(350));
            }
            return Ok((pid, Some(window)));
        }
        if Instant::now() > deadline {
            let titled = title.map(|t| format!(" with “{t}” in its title")).unwrap_or_default();
            return Err(AgentError::App(format!("{app} has no open window{titled}")));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Opens the app, finds its window and lists the controls in it, so an agent can plan a take by ref.
pub fn prepare(app: Option<&str>, window: Option<&str>) -> Result<Prepared, AgentError> {
    if !ax::trusted() {
        return Err(AgentError::Permission(ACCESSIBILITY_PERMISSION.into()));
    }
    let (pid, window) = locate(app, window)?;
    let pid = pid.ok_or_else(|| AgentError::App("couldn’t tell which app is in front".into()))?;
    let mut tree = ax::Tree::new(pid, window.as_ref().map(bounds))
        .ok_or_else(|| AgentError::App("couldn’t read that app’s controls".into()))?;
    let controls = tree.snapshot();
    let app = window
        .as_ref()
        .map(|w| w.app.clone())
        .or_else(|| tree.app_name())
        .or(app.map(String::from))
        .unwrap_or_default();
    Ok(Prepared { app, window, controls, truncated: tree.truncated })
}

struct MacDesk<'a> {
    clock: Instant,
    recording: &'a ProjectRecording,
    tree: Option<ax::Tree>,
    source: Option<CFRetained<CGEventSource>>,
    /// The click count of the last press, for its release.
    clicks: i64,
}

impl MacDesk<'_> {
    fn post(&self, event: Option<CFRetained<CGEvent>>) {
        if let Some(event) = event {
            // The recorder files events with this mark as the agent's.
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::EventSourceUserData,
                AGENT_EVENT_MARK,
            );
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }

    fn tap_key(&self, code: u16, flags: CGEventFlags) {
        for down in [true, false] {
            let event = CGEvent::new_keyboard_event(self.source.as_deref(), code, down);
            if let Some(event) = &event {
                CGEvent::set_flags(Some(event), flags);
            }
            self.post(event);
        }
    }
}

impl Desk for MacDesk<'_> {
    fn now_ms(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    fn sleep_ms(&mut self, ms: f64) {
        std::thread::sleep(Duration::from_secs_f64(ms.max(0.0) / 1000.0));
    }

    fn screen_changes(&self) -> u64 {
        self.recording.frames()
    }

    fn control(&mut self, id: &str) -> Option<ControlSnapshot> {
        self.tree.as_mut()?.control(id)
    }

    fn find(&mut self, query: &str) -> Option<ControlSnapshot> {
        self.tree.as_mut()?.find(query)
    }

    fn pointer(&self) -> Point {
        pointer()
    }

    fn move_pointer(&mut self, to: Point, held: Option<MouseButton>) {
        let kind = match held {
            Some(MouseButton::Left) => CGEventType::LeftMouseDragged,
            Some(MouseButton::Right) => CGEventType::RightMouseDragged,
            _ => CGEventType::MouseMoved,
        };
        let button =
            if held == Some(MouseButton::Right) { CGMouseButton::Right } else { CGMouseButton::Left };
        let at = CGPoint { x: to.x, y: to.y };
        self.post(CGEvent::new_mouse_event(self.source.as_deref(), kind, at, button));
    }

    fn press(&mut self, at: Point, button: MouseButton, down: bool, count: u8) {
        let (kind, cg_button) = match (button, down) {
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (_, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (_, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
        };
        if down {
            self.clicks = i64::from(count.max(1));
        }
        let event =
            CGEvent::new_mouse_event(self.source.as_deref(), kind, CGPoint { x: at.x, y: at.y }, cg_button);
        if let Some(event) = &event {
            CGEvent::set_integer_value_field(Some(event), CGEventField::MouseEventClickState, self.clicks);
        }
        self.post(event);
    }

    fn type_char(&mut self, ch: char) {
        match ch {
            '\n' | '\r' => self.tap_key(keys::RETURN, CGEventFlags::empty()),
            '\t' => self.tap_key(keys::TAB, CGEventFlags::empty()),
            _ => {
                let mut buffer = [0u16; 2];
                let units = ch.encode_utf16(&mut buffer);
                for down in [true, false] {
                    let event = CGEvent::new_keyboard_event(self.source.as_deref(), 0, down);
                    if let Some(event) = &event {
                        CGEvent::set_flags(Some(event), CGEventFlags::empty());
                        // SAFETY: the pointer and length describe `units`, which outlives the call.
                        unsafe {
                            CGEvent::keyboard_set_unicode_string(
                                Some(event),
                                units.len() as _,
                                units.as_ptr(),
                            )
                        };
                    }
                    self.post(event);
                }
            }
        }
    }

    fn key(&mut self, chord: &KeyChord) {
        match keys::chord(chord) {
            Some((code, flags)) => self.tap_key(code, flags),
            None => tracing::warn!(?chord, "no key code for this chord"),
        }
    }

    fn scroll(&mut self, at: Point, dx: f64, dy: f64) {
        // Wheel values run the other way: positive scrolls up and left.
        let event = CGEvent::new_scroll_wheel_event2(
            self.source.as_deref(),
            CGScrollEventUnit::Pixel,
            2,
            (-dy).round() as i32,
            (-dx).round() as i32,
            0,
        );
        if let Some(event) = &event {
            CGEvent::set_location(Some(event), CGPoint { x: at.x, y: at.y });
        }
        self.post(event);
    }

    fn secure_input(&self) -> bool {
        IsSecureEventInputEnabled() != 0
    }

    fn escape_down(&self) -> bool {
        CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, keys::ESCAPE)
    }

    fn hand_back_down(&self) -> bool {
        let flags = CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState);
        CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, keys::RETURN)
            && flags.contains(CGEventFlags::MaskCommand | CGEventFlags::MaskAlternate)
    }

    fn modifiers_held(&self) -> bool {
        CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState).intersects(
            CGEventFlags::MaskCommand
                | CGEventFlags::MaskAlternate
                | CGEventFlags::MaskControl
                | CGEventFlags::MaskShift,
        )
    }
}

/// Microseconds on the project's timeline for a moment on the desk clock.
fn project_us(desk_ms: f64, zero_ms: f64, duration_us: u64) -> u64 {
    (((desk_ms - zero_ms).max(0.0) * 1000.0) as u64).min(duration_us)
}

/// Adds the captions as titles, and cuts the stretches the person had control.
fn finish_project(
    root: &Path,
    performed: &Performed,
    zero_ms: f64,
) -> Result<Option<shotmaxxing_core::project::Project>, AgentError> {
    if performed.captions.is_empty() && performed.handoffs.is_empty() {
        return Ok(None);
    }
    let problem = |e: shotmaxxing_core::bundle::BundleError| AgentError::Project(e.to_string());
    let (bundle, mut project) = ProjectBundle::open(root).map_err(problem)?;
    let duration = project.duration_us();
    for (i, (start, end, text)) in performed.captions.iter().enumerate() {
        let range = TimeRange {
            start_us: project_us(*start, zero_ms, duration),
            end_us: project_us(*end, zero_ms, duration),
        };
        if range.end_us > range.start_us {
            project.edits.texts.push(TextOverlay {
                id: format!("say-{}", i + 1),
                range,
                text: text.clone(),
                position: Point::new(0.05, 0.86),
                style: String::new(),
            });
        }
    }
    let mut cuts = project.edits.cuts.clone();
    for (start, end) in &performed.handoffs {
        cuts.push(TimeRange {
            start_us: project_us(*start, zero_ms, duration),
            end_us: project_us(*end, zero_ms, duration),
        });
    }
    cuts.retain(|c| c.end_us > c.start_us);
    cuts.sort_by_key(|c| c.start_us);
    let mut joined: Vec<TimeRange> = Vec::with_capacity(cuts.len());
    for cut in cuts {
        match joined.last_mut() {
            Some(last) if cut.start_us <= last.end_us => last.end_us = last.end_us.max(cut.end_us),
            _ => joined.push(cut),
        }
    }
    project.edits.cuts = joined;
    bundle.save(&mut project).map_err(problem)?;
    Ok(Some(project))
}

/// Films a take: opens the app, records its window (or the screen), performs the script and saves the project.
pub fn run_take(
    script: &TakeScript,
    options: TakeOptions,
    progress: &mut dyn FnMut(Progress),
) -> Result<Take, AgentError> {
    let problems: Vec<String> = script
        .validate()
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| match issue.beat {
            Some(beat) => format!("beat {}: {}", beat + 1, issue.message),
            None => issue.message,
        })
        .collect();
    if !problems.is_empty() {
        return Err(AgentError::Script(problems));
    }
    if !shotmaxxing_capture::has_permission() {
        return Err(AgentError::Permission(SCREEN_PERMISSION.into()));
    }
    if !ax::trusted() {
        return Err(AgentError::Permission(ACCESSIBILITY_PERMISSION.into()));
    }
    progress(Progress { done: 0.0, message: "Getting the app ready".into() });
    let (pid, window) = locate(script.app.as_deref(), script.window.as_deref())?;
    let target = match (&window, script.framing) {
        (Some(window), Framing::Window) => Target::Window(window.id),
        _ => Target::Display(None),
    };
    let mut project_options = ProjectOptions::new(target);
    project_options.max_fps = script.fps.unwrap_or(60).clamp(15, 120);
    project_options.hide_desktop_icons = script.framing == Framing::Display;
    project_options.microphone = options.microphone.clone();
    project_options.system_audio = options.system_audio;
    project_options.agent = Some(AgentInfo {
        client: options.client.clone().unwrap_or_else(|| "An agent".into()),
        take_script: Some(script.to_yaml()),
    });
    let name = script
        .title
        .clone()
        .or_else(|| window.as_ref().map(|w| w.app.clone()))
        .unwrap_or_else(|| "Take".into());
    let home = pointer();
    let recording = ProjectRecording::start(&options.folder, &name, project_options)?;
    let mut desk = MacDesk {
        clock: Instant::now(),
        recording: &recording,
        tree: pid.and_then(|pid| ax::Tree::new(pid, window.as_ref().map(bounds))),
        source: CGEventSource::new(CGEventSourceStateID::HIDSystemState),
        clicks: 1,
    };
    // Refs are checked against fresh element handles, so the first walk happens before the clock starts.
    if let Some(tree) = desk.tree.as_mut() {
        tree.snapshot();
    }
    let zero_ms = desk.now_ms() - recording.elapsed() * 1000.0;
    let performed = crate::perform(script, &options.planned, &mut desk, &options.cancel, progress);
    drop(desk);
    let mut finished = recording.stop()?;
    // A finished take puts the pointer back. One the person stopped leaves it with them.
    if performed.stopped.is_none() {
        let _ = CGWarpMouseCursorPosition(CGPoint { x: home.x, y: home.y });
    }
    if let Some(project) = finish_project(&finished.root, &performed, zero_ms)? {
        finished.project = project;
    }
    Ok(Take { finished, performed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desk_moments_land_on_the_project_timeline() {
        assert_eq!(project_us(1_500.0, 500.0, 10_000_000), 1_000_000);
        assert_eq!(project_us(100.0, 500.0, 10_000_000), 0, "before the first frame");
        assert_eq!(project_us(60_000.0, 0.0, 10_000_000), 10_000_000, "never past the end");
    }
}
