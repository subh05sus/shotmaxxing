//! Input telemetry on Windows: the pointer's path, clicks, scrolls and shortcuts.
//!
//! Two small threads, idle most of the time:
//! - a sampler that reads the pointer position at up to 120 Hz and keeps only changes;
//! - low-level mouse and keyboard hooks on a thread of their own, which observe input and always pass it on
//!   unchanged.
//!
//! Privacy, as on macOS: typed characters are never stored. Plain typing is recorded as "a key was pressed", so auto
//! zoom can notice typing; only shortcuts with Ctrl or the Windows key keep a label such as "Ctrl+K". Ctrl with Alt
//! is AltGr on many keyboards, which types characters such as @, so it counts as typing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use shotmaxxing_core::telemetry::{
    AGENT_EVENT_MARK, CursorSample, CursorShape, InputEvent, InputSource, MouseButton,
};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetDoubleClickTime, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, GetSystemMetrics, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_USER,
};

use super::clock;

/// Points scrolled per notch of a mouse wheel, close to what one notch moves in most apps.
const NOTCH: f64 = 40.0;

/// A raw observation, in pixels and system-clock seconds.
#[derive(Clone, Debug)]
enum Raw {
    Down { at: f64, x: f64, y: f64, button: MouseButton, clicks: u8, source: InputSource },
    Up { at: f64, x: f64, y: f64, button: MouseButton, source: InputSource },
    Scroll { at: f64, x: f64, y: f64, dx: f64, dy: f64 },
    Key { at: f64, label: Option<String>, source: InputSource },
}

fn at_of(raw: &Raw) -> f64 {
    match raw {
        Raw::Down { at, .. } | Raw::Up { at, .. } | Raw::Scroll { at, .. } | Raw::Key { at, .. } => *at,
    }
}

/// What the hooks write into. Hooks get no pointer of their own, so the recording in progress is global.
static HOOKED: Mutex<Option<Arc<Hooked>>> = Mutex::new(None);

struct Hooked {
    events: Mutex<Vec<Raw>>,
    /// The last press, to count double and triple clicks the way Windows does.
    last_press: Mutex<Option<(f64, POINT, MouseButton, u8)>>,
}

pub struct Telemetry {
    stop: Arc<AtomicBool>,
    /// The captured area's top left, in pixels.
    origin: (f64, f64),
    scale: f64,
    cursor: Option<JoinHandle<Vec<(f64, f64, f64)>>>,
    hooks: Option<(u32, JoinHandle<()>)>,
    state: Arc<Hooked>,
}

impl Telemetry {
    /// Starts recording input over an area whose top left is `origin` (pixels), at `scale` pixels per point.
    pub fn start(origin: (i32, i32), scale: f64) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let state =
            Arc::new(Hooked { events: Mutex::new(Vec::with_capacity(1024)), last_press: Mutex::new(None) });
        if let Ok(mut hooked) = HOOKED.lock() {
            *hooked = Some(state.clone());
        }
        let cursor = {
            let stop = stop.clone();
            std::thread::Builder::new().name("cursor".into()).spawn(move || sample_cursor(&stop)).ok()
        };
        let (ready, id) = channel();
        let hooks = std::thread::Builder::new()
            .name("input hooks".into())
            .spawn(move || {
                let mut message = MSG::default();
                // SAFETY: makes this thread's message queue, so a quit message can reach it; then reports its id.
                unsafe {
                    let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
                    let _ = ready.send(GetCurrentThreadId());
                }
                run_hooks();
            })
            .ok()
            .and_then(|thread| id.recv().ok().map(|id| (id, thread)));
        Self { stop, origin: (f64::from(origin.0), f64::from(origin.1)), scale, cursor, hooks, state }
    }

    /// Ends recording and returns the pointer path and events, timed from `zero` (the first frame, in system-clock
    /// seconds) and placed in points from the captured area's top left.
    pub fn stop(mut self, zero: f64) -> (Vec<CursorSample>, Vec<InputEvent>) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some((id, thread)) = self.hooks.take() {
            // SAFETY: asks the hook thread's message loop to end.
            let _ = unsafe { PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = thread.join();
        }
        if let Ok(mut hooked) = HOOKED.lock() {
            *hooked = None;
        }
        let path = self.cursor.take().and_then(|t| t.join().ok()).unwrap_or_default();

        let ((ox, oy), scale) = (self.origin, self.scale);
        let at_point = |x: f64, y: f64| ((x - ox) / scale, (y - oy) / scale);
        let t_us = |at: f64| ((at - zero).max(0.0) * 1e6) as u64;
        // The pointer's resting place when the first frame arrived counts as its position at 0.
        let before = path.iter().rposition(|(at, _, _)| *at < zero);
        let cursor = path
            .into_iter()
            .enumerate()
            .filter(|(i, (at, _, _))| *at >= zero || Some(*i) == before)
            .map(|(_, (at, x, y))| {
                let (x, y) = at_point(x, y);
                CursorSample { t_us: t_us(at), x, y, shape: CursorShape::Arrow }
            })
            .collect();

        let mut raw = self.state.events.lock().map(|e| e.clone()).unwrap_or_default();
        raw.sort_by(|a, b| at_of(a).total_cmp(&at_of(b)));
        let events = raw
            .into_iter()
            .filter(|r| at_of(r) >= zero)
            .map(|r| match r {
                Raw::Down { at, x, y, button, clicks, source } => {
                    let (x, y) = at_point(x, y);
                    InputEvent::MouseDown { t_us: t_us(at), x, y, button, clicks, source }
                }
                Raw::Up { at, x, y, button, source } => {
                    let (x, y) = at_point(x, y);
                    InputEvent::MouseUp { t_us: t_us(at), x, y, button, source }
                }
                Raw::Scroll { at, x, y, dx, dy } => {
                    let (x, y) = at_point(x, y);
                    InputEvent::Scroll { t_us: t_us(at), x, y, dx, dy }
                }
                Raw::Key { at, label, source } => InputEvent::KeyDown {
                    t_us: t_us(at),
                    modifiers_only: false,
                    redacted: label.is_none(),
                    label,
                    source,
                },
            })
            .collect();
        (cursor, events)
    }
}

/// Reads the pointer position at up to 120 Hz, keeping only positions that changed.
fn sample_cursor(stop: &AtomicBool) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::with_capacity(4096);
    let mut last = None;
    while !stop.load(Ordering::Relaxed) {
        let mut p = POINT::default();
        // SAFETY: writes one POINT.
        if unsafe { GetCursorPos(&mut p) }.is_ok() && last != Some((p.x, p.y)) {
            out.push((clock::now(), f64::from(p.x), f64::from(p.y)));
            last = Some((p.x, p.y));
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    out
}

/// Hooks mouse and keyboard input until this thread is asked to quit.
fn run_hooks() {
    // SAFETY: the hooks belong to this thread's message loop and are removed before it ends. They only observe.
    unsafe {
        let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module, 0);
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module, 0);
        let mut message = MSG::default();
        // The hooks are called from inside this wait.
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {}
        if let Ok(hook) = mouse {
            let _ = UnhookWindowsHookEx(hook);
        }
        if let Ok(hook) = keyboard {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for mouse actions, `lparam` points at the event's MSLLHOOKSTRUCT for the length of the call.
        record_mouse(wparam.0 as u32, unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) });
    }
    // SAFETY: passes the event on, unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        // SAFETY: for key actions, `lparam` points at the event's KBDLLHOOKSTRUCT for the length of the call.
        record_key(unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) });
    }
    // SAFETY: passes the event on, unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Events a take performs carry Shotmaxxing's mark; everything else is the person.
fn source_of(extra: usize) -> InputSource {
    if extra as i64 == AGENT_EVENT_MARK { InputSource::Agent } else { InputSource::Person }
}

fn record_mouse(message: u32, info: &MSLLHOOKSTRUCT) {
    let button = match message {
        WM_LBUTTONDOWN | WM_LBUTTONUP => MouseButton::Left,
        WM_RBUTTONDOWN | WM_RBUTTONUP => MouseButton::Right,
        WM_MBUTTONDOWN | WM_MBUTTONUP => MouseButton::Middle,
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => MouseButton::Middle,
        // Moves come from the sampler.
        _ => return,
    };
    let Some(state) = HOOKED.lock().ok().and_then(|hooked| hooked.clone()) else { return };
    let at = clock::now();
    let (x, y) = (f64::from(info.pt.x), f64::from(info.pt.y));
    let source = source_of(info.dwExtraInfo);
    let raw = match message {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            Raw::Down { at, x, y, button, clicks: count_click(&state, at, info.pt, button), source }
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => Raw::Up { at, x, y, button, source },
        _ => {
            // The wheel's turn is the high word: 120 for each notch, forward (up) positive.
            let notches = f64::from((info.mouseData >> 16) as u16 as i16) / 120.0;
            let (dx, dy) =
                if message == WM_MOUSEWHEEL { (0.0, notches * NOTCH) } else { (notches * NOTCH, 0.0) };
            Raw::Scroll { at, x, y, dx, dy }
        }
    };
    if let Ok(mut events) = state.events.lock() {
        events.push(raw);
    }
}

/// 1, 2 or 3, counting presses of the same button close in time and place, by the system's double-click settings.
fn count_click(state: &Hooked, at: f64, point: POINT, button: MouseButton) -> u8 {
    // SAFETY: reads plain system settings.
    let (time, near_x, near_y) = unsafe {
        (
            f64::from(GetDoubleClickTime()) / 1000.0,
            GetSystemMetrics(SM_CXDOUBLECLK) / 2,
            GetSystemMetrics(SM_CYDOUBLECLK) / 2,
        )
    };
    let Ok(mut last) = state.last_press.lock() else { return 1 };
    let clicks = match *last {
        Some((t, p, b, n))
            if b == button
                && at - t <= time
                && (point.x - p.x).abs() <= near_x
                && (point.y - p.y).abs() <= near_y =>
        {
            n % 3 + 1
        }
        _ => 1,
    };
    *last = Some((at, point, button, clicks));
    clicks
}

fn record_key(info: &KBDLLHOOKSTRUCT) {
    let key = info.vkCode as u16;
    // Shift, Ctrl, Alt, the Windows keys and Caps Lock on their own aren't keystrokes.
    if matches!(key, 0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0xA0..=0xA5) {
        return;
    }
    let Some(state) = HOOKED.lock().ok().and_then(|hooked| hooked.clone()) else { return };
    let raw = Raw::Key { at: clock::now(), label: shortcut_label(key), source: source_of(info.dwExtraInfo) };
    if let Ok(mut events) = state.events.lock() {
        events.push(raw);
    }
}

fn held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads one key's state; the high bit is set while it's held.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

/// "Ctrl+K" for shortcuts; `None` (redacted) for plain typing.
fn shortcut_label(key: u16) -> Option<String> {
    let (control, alt, shift, win) =
        (held(VK_CONTROL), held(VK_MENU), held(VK_SHIFT), held(VK_LWIN) || held(VK_RWIN));
    let named = match key {
        0x0D => Some("Enter"),
        0x09 => Some("Tab"),
        0x08 => Some("Backspace"),
        0x1B => Some("Esc"),
        0x2E => Some("Delete"),
        0x20 => Some("Space"),
        0x25 => Some("←"),
        0x26 => Some("↑"),
        0x27 => Some("→"),
        0x28 => Some("↓"),
        _ => None,
    };
    // AltGr arrives as Ctrl and Alt together, and types characters.
    let shortcut = (control && !alt) || win;
    if !shortcut {
        // Navigation keys are worth showing; letters and digits stay private.
        return named.filter(|n| *n != "Space").map(str::to_string);
    }
    let name = match named {
        Some(name) => name.to_string(),
        None => key_name(key)?,
    };
    let mut label = String::new();
    for (on, prefix) in [(control, "Ctrl+"), (alt, "Alt+"), (shift, "Shift+"), (win, "Win+")] {
        if on {
            label.push_str(prefix);
        }
    }
    label.push_str(&name);
    Some(label)
}

/// Letters, digits, function keys and punctuation on a US layout, used only for shortcut labels.
fn key_name(key: u16) -> Option<String> {
    let name = match key {
        0x30..=0x39 | 0x41..=0x5A => char::from(key as u8).to_string(),
        0x70..=0x87 => format!("F{}", key - 0x6F),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => return None,
    };
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_keys_have_names_and_letters_stay_private() {
        assert_eq!(key_name(0x4B).as_deref(), Some("K"));
        assert_eq!(key_name(0x35).as_deref(), Some("5"));
        assert_eq!(key_name(0x74).as_deref(), Some("F5"));
        assert_eq!(key_name(0xBF).as_deref(), Some("/"));
        assert_eq!(key_name(0xFF), None);
        // Nothing is held while tests run: plain keys are redacted, navigation keys named.
        assert_eq!(shortcut_label(0x4B), None);
        assert_eq!(shortcut_label(0x0D).as_deref(), Some("Enter"));
        assert_eq!(shortcut_label(0x20), None);
    }
}
