//! The Windows desk: input through SendInput, controls through UI Automation, and the recording around a take.
//!
//! The desk works in points, as the Mac's does: pixels divided by the target window's scale, so its numbers match
//! what the window list and the recorder say. Every event it sends is marked as the agent's, so the recorder files
//! the take's clicks and keys apart from the person's.
//!
//! Windows asks no permission to read controls or send input, but it keeps a program from controlling one that
//! runs as administrator: a take refuses such an app up front rather than send input that would vanish.

mod keys;
mod uia;

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use shotmaxxing_capture::{ProjectOptions, ProjectRecording, Target, WindowInfo};
use shotmaxxing_core::geometry::Point;
use shotmaxxing_core::keys::KeyChord;
use shotmaxxing_core::project::AgentInfo;
use shotmaxxing_core::refs::ControlSnapshot;
use shotmaxxing_core::take::{Framing, TakeScript};
use shotmaxxing_core::telemetry::{AGENT_EVENT_MARK, MouseButton};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, POINT};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, GetCurrentProcess,
    GetCurrentThreadId, INFINITE, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    SetWaitableTimer, TIMER_ALL_ACCESS, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW, SendInput,
    VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN, VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
    IsIconic, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
    SW_SHOWNORMAL, SetCursorPos, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, w};

use crate::{AgentError, Desk, Prepared, Progress, Take, TakeOptions};

/// How far a wheel notch scrolls, in points: three lines of about 16.
const POINTS_PER_NOTCH: f64 = 48.0;
/// What Windows counts as one notch of the wheel.
const NOTCH: f64 = 120.0;

/// Joins this thread to the process's shared COM apartment, which UI Automation works best from. A thread that
/// already has an apartment keeps it.
fn com() {
    // SAFETY: a start-up call without pointers.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

/// Pixels per point for a window, or for the system when there's none.
fn scale_of(window: Option<HWND>) -> f64 {
    // SAFETY: questions about a window handle and the system.
    let dpi = window
        .map(|w| unsafe { GetDpiForWindow(w) })
        .filter(|&d| d > 0)
        .unwrap_or(unsafe { GetDpiForSystem() });
    f64::from(dpi.max(48)) / 96.0
}

fn pointer(scale: f64) -> Point {
    let mut at = POINT::default();
    // SAFETY: writes one point.
    let _ = unsafe { GetCursorPos(&mut at) };
    Point::new(f64::from(at.x) / scale, f64::from(at.y) / scale)
}

fn window_handle(window: &WindowInfo) -> HWND {
    HWND(window.id as usize as *mut c_void)
}

/// Whether a process runs as administrator. `None` when that can't be told.
fn elevated(pid: Option<u32>) -> Option<bool> {
    // SAFETY: opens a process and its token only to ask one question, and closes both.
    unsafe {
        let process = match pid {
            Some(pid) => OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?,
            None => GetCurrentProcess(),
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token);
        if pid.is_some() {
            let _ = CloseHandle(process);
        }
        opened.ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let asked = GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);
        asked.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

/// Refuses an app this process can't control: one that runs as administrator when this one doesn't.
fn check_reachable(window: HWND, app: &str) -> Result<(), AgentError> {
    let mut pid = 0u32;
    // SAFETY: a question about a window handle.
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if elevated(Some(pid)) == Some(true) && elevated(None) == Some(false) {
        return Err(AgentError::App(format!(
            "{app} runs as administrator, and Windows doesn't let other apps control it. Start {app} normally, or \
             run Shotmaxxing as administrator too"
        )));
    }
    Ok(())
}

/// Brings a window forward. Windows only lets the app in front hand the foreground on, so when asking isn't enough,
/// this thread shares the front app's input for a moment, which counts.
fn raise(window: HWND) {
    // SAFETY: calls on a window handle and thread ids; joining another thread's input is undone at once.
    unsafe {
        if IsIconic(window).as_bool() {
            let _ = ShowWindow(window, SW_RESTORE);
        }
        if GetForegroundWindow() == window {
            return;
        }
        let _ = SetForegroundWindow(window);
        if GetForegroundWindow() == window {
            return;
        }
        let front = GetForegroundWindow();
        let theirs = GetWindowThreadProcessId(front, None);
        let ours = GetCurrentThreadId();
        if theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, true).as_bool() {
            let _ = BringWindowToTop(window);
            let _ = SetForegroundWindow(window);
            let _ = AttachThreadInput(ours, theirs, false);
        }
    }
}

/// The shortcuts in the Start menu, for this user and for everyone.
fn start_menu() -> Vec<PathBuf> {
    fn collect(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                if depth < 3 {
                    collect(&path, depth + 1, found);
                }
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    for base in ["APPDATA", "ProgramData"].into_iter().filter_map(std::env::var_os) {
        collect(&Path::new(&base).join(r"Microsoft\Windows\Start Menu\Programs"), 0, &mut found);
    }
    found
}

/// Where a program that registers itself (as Chrome and Edge do) is installed.
fn app_path(root: HKEY, exe: &str) -> Option<PathBuf> {
    let key: Vec<u16> = OsStr::new(&format!(r"Software\Microsoft\Windows\CurrentVersion\App Paths\{exe}"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: reads a string value into `buffer`, whose size in bytes is given.
    let read = unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return None;
    }
    let text = String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap_or(0)]);
    let path = PathBuf::from(text.trim().trim_matches('"'));
    path.exists().then_some(path)
}

/// A program by its name: registered ones, Windows' own (Notepad, Paint) and Store apps' aliases. The current
/// folder is never searched.
fn program(name: &str) -> Option<PathBuf> {
    let exe = if name.to_lowercase().ends_with(".exe") { name.to_string() } else { format!("{name}.exe") };
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        if let Some(path) = app_path(root, &exe) {
            return Some(path);
        }
    }
    let system = std::env::var_os("SystemRoot").map(|r| Path::new(&r).join("System32").join(&exe));
    let aliases =
        std::env::var_os("LOCALAPPDATA").map(|r| Path::new(&r).join(r"Microsoft\WindowsApps").join(&exe));
    // Aliases are special links that can't be followed, so only their own entry is looked at.
    [system, aliases].into_iter().flatten().find(|p| std::fs::symlink_metadata(p).is_ok())
}

fn open(file: &OsStr) -> bool {
    let file: Vec<u16> = file.encode_wide().chain(Some(0)).collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: the strings outlive the call.
    unsafe { ShellExecuteExW(&mut info) }.is_ok()
}

/// Starts an app by the name it has in the Start menu, or by its program's name. Only names are taken: something
/// that looks like a path or an address isn't opened.
fn launch(app: &str) -> bool {
    if app.contains(['/', '\\', ':', '%']) || app.starts_with('.') {
        return false;
    }
    let wanted = app.to_lowercase();
    let shortcut = start_menu()
        .into_iter()
        .filter_map(|path| Some((path.file_stem()?.to_string_lossy().to_lowercase(), path)))
        .filter(|(name, _)| name.contains(&wanted))
        .min_by_key(|(name, _)| (*name != wanted, name.len()))
        .map(|(_, path)| path);
    match shortcut.or_else(|| program(app)) {
        Some(path) => open(path.as_os_str()),
        None => false,
    }
}

/// Whether a window belongs to the app an agent named: by its name (“Chrome” finds Google Chrome) or its program.
fn is_app(window: &WindowInfo, needle: &str) -> bool {
    let program = window.bundle_id.trim_end_matches(".exe");
    window.app.to_lowercase().contains(needle) || program == needle.trim_end_matches(".exe")
}

/// Starts the app (or finds it open) and its window: the largest, or the one whose title contains `title`.
fn locate(app: Option<&str>, title: Option<&str>) -> Result<(Option<HWND>, Option<WindowInfo>), AgentError> {
    shotmaxxing_capture::prepare();
    let Some(app) = app.map(str::trim).filter(|a| !a.is_empty()) else {
        // SAFETY: a question without arguments.
        let front = unsafe { GetForegroundWindow() };
        return Ok(((!front.is_invalid()).then_some(front), None));
    };
    let needle = app.to_lowercase();
    let title = title.map(str::to_lowercase);
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut started = false;
    loop {
        let windows: Vec<WindowInfo> =
            shotmaxxing_capture::sources()?.windows.into_iter().filter(|w| is_app(w, &needle)).collect();
        // An app that's already open is brought forward rather than started again.
        if windows.is_empty() && !started {
            started = true;
            if !launch(app) {
                return Err(AgentError::App(format!(
                    "couldn’t open {app}. Use the name it has in the Start menu, or its program’s name"
                )));
            }
        }
        let window = windows
            .into_iter()
            .filter(|w| title.as_ref().is_none_or(|t| w.title.to_lowercase().contains(t)))
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)));
        if let Some(window) = window {
            let handle = window_handle(&window);
            check_reachable(handle, &window.app)?;
            raise(handle);
            // Give the window a moment to come forward before anything looks at it.
            std::thread::sleep(Duration::from_millis(350));
            return Ok((Some(handle), Some(window)));
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
    com();
    let (handle, window) = locate(app, window)?;
    let handle = handle.ok_or_else(|| AgentError::App("couldn’t tell which app is in front".into()))?;
    let automation =
        uia::automation().ok_or_else(|| AgentError::App("UI Automation isn’t available".into()))?;
    let mut tree = uia::Tree::new(automation, handle, scale_of(Some(handle)))
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

/// Waits precisely. Windows' ordinary sleep rounds up to its 15.6 ms tick, which would make the pointer stutter.
struct Sleeper(Option<HANDLE>);

impl Sleeper {
    fn new() -> Self {
        // SAFETY: creates a timer this struct owns and closes.
        let timer = unsafe {
            CreateWaitableTimerExW(
                None,
                PCWSTR::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )
        };
        Self(timer.ok())
    }

    fn sleep(&self, ms: f64) {
        if ms <= 0.0 {
            return;
        }
        if let Some(timer) = self.0 {
            // Negative: from now, in 100-nanosecond units.
            let due = -((ms * 10_000.0).round() as i64).max(1);
            // SAFETY: arms the timer this struct owns, then waits for it.
            unsafe {
                if SetWaitableTimer(timer, &due, 0, None, None, false).is_ok() {
                    WaitForSingleObject(timer, INFINITE);
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_secs_f64(ms / 1000.0));
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        if let Some(timer) = self.0.take() {
            // SAFETY: closes the timer this struct owns.
            let _ = unsafe { CloseHandle(timer) };
        }
    }
}

fn send(inputs: &[INPUT]) {
    // SAFETY: the inputs are complete and the size is theirs.
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
}

/// A mouse event, marked as the agent's.
fn mouse(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: AGENT_EVENT_MARK as usize,
            },
        },
    }
}

/// A key going down or up, marked as the agent's. The scan code goes too, for apps that read it.
fn key_event(key: VIRTUAL_KEY, down: bool) -> INPUT {
    // SAFETY: a lookup in the keyboard layout.
    let scan = unsafe { MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP };
    if keys::extended(key) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    keyboard(key, scan, flags)
}

/// A character typed as itself, whatever the layout.
fn unicode(unit: u16, down: bool) -> INPUT {
    let flags = if down { KEYEVENTF_UNICODE } else { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP };
    keyboard(VIRTUAL_KEY(0), unit, flags)
}

fn keyboard(key: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: AGENT_EVENT_MARK as usize,
            },
        },
    }
}

fn held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: a question about one key; the top bit says it's down.
    unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 }
}

struct WinDesk<'a> {
    clock: Instant,
    sleeper: Sleeper,
    recording: &'a ProjectRecording,
    automation: Option<windows::Win32::UI::Accessibility::IUIAutomation>,
    tree: Option<uia::Tree>,
    /// Pixels per point.
    scale: f64,
    /// The take card's Hand back button.
    hand_back: Arc<AtomicBool>,
    /// Wheel movement too small to send yet, across and down, in Windows' units.
    wheel: (f64, f64),
}

impl WinDesk<'_> {
    /// A point on the desk in SendInput's terms: 0 to 65,535 across the whole virtual screen.
    fn absolute(&self, to: Point) -> (i32, i32) {
        // SAFETY: plain questions about the screen.
        let (left, top, width, height) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            )
        };
        // Rounded up, so Windows' rounding down lands on the very pixel.
        let scaled = |value: f64, origin: i32, size: i32| {
            let pixel = i64::from((value * self.scale).round() as i32 - origin);
            ((pixel * 65_536 + i64::from(size) - 1) / i64::from(size)).clamp(0, 65_535) as i32
        };
        (scaled(to.x, left, width), scaled(to.y, top, height))
    }

    fn tap(&self, key: VIRTUAL_KEY, modifiers: &[VIRTUAL_KEY]) {
        let mut inputs: Vec<INPUT> = modifiers.iter().map(|&m| key_event(m, true)).collect();
        inputs.push(key_event(key, true));
        inputs.push(key_event(key, false));
        inputs.extend(modifiers.iter().rev().map(|&m| key_event(m, false)));
        send(&inputs);
    }
}

impl Desk for WinDesk<'_> {
    fn now_ms(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    fn sleep_ms(&mut self, ms: f64) {
        self.sleeper.sleep(ms);
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
        pointer(self.scale)
    }

    fn move_pointer(&mut self, to: Point, _held: Option<MouseButton>) {
        // Windows keeps track of held buttons itself, so a drag is a move while one is down.
        let (x, y) = self.absolute(to);
        send(&[mouse(x, y, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)]);
    }

    fn press(&mut self, at: Point, button: MouseButton, down: bool, _count: u8) {
        // Windows counts double clicks itself, from how quickly presses follow each other.
        let flag = match (button, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
            (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
            (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        };
        let (x, y) = self.absolute(at);
        send(&[mouse(x, y, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK | flag)]);
    }

    fn type_char(&mut self, ch: char) {
        match ch {
            '\n' | '\r' => self.tap(VK_RETURN, &[]),
            '\t' => self.tap(VK_TAB, &[]),
            _ => {
                let mut units = [0u16; 2];
                let inputs: Vec<INPUT> = ch
                    .encode_utf16(&mut units)
                    .iter()
                    .flat_map(|&u| [unicode(u, true), unicode(u, false)])
                    .collect();
                send(&inputs);
            }
        }
    }

    fn key(&mut self, chord: &KeyChord) {
        match keys::chord(chord) {
            Some(press) => self.tap(press.key, &press.modifiers),
            None => tracing::warn!(?chord, "no key for this chord on this layout"),
        }
    }

    fn scroll(&mut self, at: Point, dx: f64, dy: f64) {
        // Windows' wheel runs the other way: positive scrolls up. Steps smaller than a notch go as they are, which
        // today's apps follow smoothly; what rounding leaves over waits for the next step.
        self.wheel.0 += dx * NOTCH / POINTS_PER_NOTCH;
        self.wheel.1 -= dy * NOTCH / POINTS_PER_NOTCH;
        let (across, down) = (self.wheel.0.trunc(), self.wheel.1.trunc());
        self.wheel.0 -= across;
        self.wheel.1 -= down;
        let (x, y) = self.absolute(at);
        let mut inputs =
            vec![mouse(x, y, 0, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK)];
        if down != 0.0 {
            inputs.push(mouse(0, 0, down as i32, MOUSEEVENTF_WHEEL));
        }
        if across != 0.0 {
            inputs.push(mouse(0, 0, across as i32, MOUSEEVENTF_HWHEEL));
        }
        send(&inputs);
    }

    fn secure_input(&self) -> bool {
        self.automation.as_ref().is_some_and(uia::password_focused)
    }

    fn escape_down(&self) -> bool {
        held(VK_ESCAPE)
    }

    /// Ctrl+Alt+Enter on Windows, where the Mac uses ⌥⌘Return.
    fn hand_back_down(&self) -> bool {
        if self.hand_back.swap(false, Ordering::SeqCst) {
            return true;
        }
        held(VK_RETURN) && held(VK_CONTROL) && held(VK_MENU)
    }

    fn modifiers_held(&self) -> bool {
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].into_iter().any(held)
    }
}

/// Films a take: opens the app, records its window (or the screen), performs the script and saves the project.
pub fn run_take(
    script: &TakeScript,
    options: TakeOptions,
    progress: &mut dyn FnMut(Progress),
) -> Result<Take, AgentError> {
    crate::check_script(script)?;
    com();
    progress(Progress { done: 0.0, message: "Getting the app ready".into(), ..Progress::default() });
    let (handle, window) = locate(script.app.as_deref(), script.window.as_deref())?;
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
    let scale = scale_of(handle);
    let home = pointer(scale);
    let recording = ProjectRecording::start(&options.folder, &name, project_options)?;
    let automation = uia::automation();
    let mut desk = WinDesk {
        clock: Instant::now(),
        sleeper: Sleeper::new(),
        recording: &recording,
        tree: automation.clone().zip(handle).and_then(|(a, h)| uia::Tree::new(a, h, scale)),
        automation,
        scale,
        hand_back: options.hand_back.clone(),
        wheel: (0.0, 0.0),
    };
    // Refs are checked against fresh elements, so the first walk happens before the clock starts.
    if let Some(tree) = desk.tree.as_mut() {
        tree.snapshot();
    }
    let zero_ms = desk.now_ms() - recording.elapsed() * 1000.0;
    let performed = crate::perform(script, &options.planned, &mut desk, &options.cancel, progress);
    drop(desk);
    let mut finished = recording.stop()?;
    // A finished take puts the pointer back. One the person stopped leaves it with them.
    if performed.stopped.is_none() {
        // SAFETY: moves the pointer to a point on the screen.
        let _ = unsafe { SetCursorPos((home.x * scale).round() as i32, (home.y * scale).round() as i32) };
    }
    if let Some(project) = crate::finish_project(&finished.root, &performed, zero_ms)? {
        finished.project = project;
    }
    Ok(Take { finished, performed })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(app: &str, program: &str) -> WindowInfo {
        WindowInfo {
            id: 1,
            title: "Untitled".into(),
            app: app.into(),
            bundle_id: program.into(),
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        }
    }

    #[test]
    fn apps_are_found_by_name_or_program() {
        assert!(is_app(&window("Google Chrome", "chrome.exe"), "chrome"));
        assert!(is_app(&window("Microsoft Edge", "msedge.exe"), "msedge"));
        assert!(is_app(&window("Microsoft Edge", "msedge.exe"), "msedge.exe"));
        assert!(!is_app(&window("Notepad", "notepad.exe"), "chrome"));
    }

    #[test]
    fn only_names_are_launched() {
        assert!(!launch(r"C:\Windows\System32\cmd.exe"));
        assert!(!launch("https://example.com"));
        assert!(!launch("..\\something"));
    }

    #[test]
    fn this_process_is_not_refused() {
        assert!(elevated(None).is_some());
        assert!(scale_of(None) >= 0.5);
    }

    /// Lists Notepad's controls through UI Automation. It opens Notepad, so it only runs when asked for.
    #[test]
    #[ignore = "opens Notepad"]
    fn lists_notepads_controls() {
        let prepared = prepare(Some("Notepad"), None).unwrap();
        eprintln!("{} controls in {}", prepared.controls.len(), prepared.app);
        for control in prepared.controls.iter().take(20) {
            eprintln!("  {} {:?}", control.id, control.label);
        }
        assert!(!prepared.controls.is_empty());
    }
}
