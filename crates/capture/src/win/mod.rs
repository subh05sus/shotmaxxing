//! Windows: the displays and windows there are to record, and recording them (see [`record`]).
//!
//! Sizes are in points, as on macOS: Windows' device-independent pixels (96 per inch), with `scale` pixels per point.
//! The process is made per-monitor DPI aware first, so the system reports real pixels on every display.

use std::collections::HashMap;
use std::ffi::{OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::Once;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, MONITORINFOF_PRIMARY, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

use crate::{CaptureError, Device, DisplayInfo, Sources, Target, WindowInfo};

mod audio;
mod camera;
mod clock;
mod input;
mod preview;
mod record;

pub use audio::Voice;
pub use record::Recorder;

/// Classes of the shell's own windows: the desktop, the taskbars and the shell's pop-ups.
const SHELL_CLASSES: [&str; 5] =
    ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Windows.UI.Core.CoreWindow"];

/// Real pixels on every display. The app's manifest already asks for this; a command-line tool gets it here.
fn dpi_aware() {
    static ONCE: Once = Once::new();
    // SAFETY: a process-wide setting with no pointers; failing because it's already set is fine.
    ONCE.call_once(|| unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    });
}

pub fn prepare() {
    dpi_aware();
}

/// Windows asks nobody's permission to capture the screen.
pub fn has_permission() -> bool {
    true
}

pub fn sources() -> Result<Sources, CaptureError> {
    dpi_aware();
    let mut monitors = monitors();
    // The main display first, as on macOS.
    monitors.sort_by_key(|m| !m.primary);
    let displays = monitors
        .iter()
        .enumerate()
        .map(|(i, m)| DisplayInfo {
            id: m.number,
            name: if i == 0 { "Main display".into() } else { format!("Display {}", i + 1) },
            x: f64::from(m.bounds.left) / m.scale,
            y: f64::from(m.bounds.top) / m.scale,
            width: f64::from(m.bounds.right - m.bounds.left) / m.scale,
            height: f64::from(m.bounds.bottom - m.bounds.top) / m.scale,
        })
        .collect();
    Ok(Sources { displays, windows: windows() })
}

/// One display, as Windows describes it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Monitor {
    pub handle: HMONITOR,
    /// The number Windows gives it (`\\.\DISPLAY2` is 2): stable while it stays connected.
    pub number: u32,
    pub primary: bool,
    /// In pixels, in the virtual screen's coordinates.
    pub bounds: RECT,
    /// Pixels per point.
    pub scale: f64,
}

pub(crate) fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, found: LPARAM) -> BOOL {
        // SAFETY: `found` is the Vec handed to EnumDisplayMonitors below, alive for the whole enumeration.
        unsafe { &mut *(found.0 as *mut Vec<HMONITOR>) }.push(monitor);
        true.into()
    }
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only adds to `handles`, which outlives the call.
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(each), LPARAM(&raw mut handles as isize)) };
    handles
        .into_iter()
        .filter_map(|handle| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: `info` is the extended form its size says it is.
            unsafe { GetMonitorInfoW(handle, &mut info.monitorInfo) }.ok().ok()?;
            let (mut dpi, mut dpi_y) = (96u32, 96u32);
            // SAFETY: reads the display's effective DPI into two integers.
            let _ = unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) };
            let device = wide_text(&info.szDevice);
            Some(Monitor {
                handle,
                number: device.trim_start_matches(r"\\.\DISPLAY").parse().unwrap_or(0),
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                bounds: info.monitorInfo.rcMonitor,
                scale: f64::from(dpi.max(48)) / 96.0,
            })
        })
        .collect()
}

/// The display to record: the one with this number, or the main display.
pub(crate) fn monitor(number: Option<u32>) -> Result<Monitor, CaptureError> {
    let monitors = monitors();
    let found = match number {
        Some(n) => monitors.iter().find(|m| m.number == n),
        None => monitors.iter().find(|m| m.primary).or(monitors.first()),
    };
    found.copied().ok_or_else(|| {
        CaptureError::SourceNotFound(number.map_or("a display".into(), |n| format!("display {n}")))
    })
}

fn windows() -> Vec<WindowInfo> {
    unsafe extern "system" fn each(window: HWND, found: LPARAM) -> BOOL {
        // SAFETY: `found` is the Vec handed to EnumWindows below, alive for the whole enumeration.
        unsafe { &mut *(found.0 as *mut Vec<HWND>) }.push(window);
        true.into()
    }
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: the callback only adds to `handles`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(each), LPARAM(&raw mut handles as isize)) };
    let own = std::process::id();
    let mut apps = HashMap::new();
    handles.into_iter().filter_map(|window| window_info(window, own, &mut apps)).collect()
}

/// A window worth offering: an app's own visible, uncloaked, top-level window of a useful size.
fn window_info(window: HWND, own: u32, apps: &mut HashMap<PathBuf, String>) -> Option<WindowInfo> {
    // SAFETY: questions about a window handle; one that closes meanwhile just answers no.
    unsafe {
        if !IsWindowVisible(window).as_bool() || IsIconic(window).as_bool() {
            return None;
        }
        // Dialogs and palettes belong to their main window.
        if GetWindow(window, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
            return None;
        }
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if style & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return None;
        }
        // Windows on other desktops, and hidden app frames, are cloaked: there, but not shown.
        let mut cloaked = 0u32;
        let asked = DwmGetWindowAttribute(window, DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4);
        if asked.is_ok() && cloaked != 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == own {
            return None;
        }
        let mut class = [0u16; 256];
        let len = GetClassNameW(window, &mut class).max(0) as usize;
        if SHELL_CLASSES.contains(&String::from_utf16_lossy(&class[..len]).as_str()) {
            return None;
        }
        let mut title = [0u16; 512];
        let len = GetWindowTextW(window, &mut title).max(0) as usize;
        let title = String::from_utf16_lossy(&title[..len]);
        if title.trim().is_empty() {
            return None;
        }
        let bounds = window_bounds(window)?;
        let scale = f64::from(GetDpiForWindow(window).max(48)) / 96.0;
        let (width, height) =
            (f64::from(bounds.right - bounds.left) / scale, f64::from(bounds.bottom - bounds.top) / scale);
        if width < 80.0 || height < 60.0 {
            return None;
        }
        let exe = process_path(pid);
        let file = exe.as_deref().and_then(Path::file_name).map(|f| f.to_string_lossy().to_lowercase());
        // Store apps all run inside the frame host; their window title is the app's name.
        let app = if file.as_deref() == Some("applicationframehost.exe") {
            title.clone()
        } else {
            exe.as_deref().map(|path| app_name(path, apps)).unwrap_or_default()
        };
        Some(WindowInfo {
            id: window.0 as usize as u32,
            title,
            app,
            bundle_id: file.unwrap_or_default(),
            x: f64::from(bounds.left) / scale,
            y: f64::from(bounds.top) / scale,
            width,
            height,
            full_screen: false,
            away: false,
        })
    }
}

/// Where a window is on screen, in pixels, without the invisible resize border.
pub(crate) fn window_bounds(window: HWND) -> Option<RECT> {
    let mut bounds = RECT::default();
    // SAFETY: writes one RECT, the size given.
    unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut bounds).cast(),
            size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    Some(bounds)
}

/// The program a process runs.
fn process_path(pid: u32) -> Option<PathBuf> {
    // SAFETY: opens the process with the least access that reads its file name, then closes it.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut name = [0u16; 1024];
        let mut len = name.len() as u32;
        let named =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(name.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        named.ok()?;
        Some(PathBuf::from(OsString::from_wide(&name[..len as usize])))
    }
}

/// An app's name as its program describes itself ("Google Chrome" for chrome.exe), or the program's own name.
fn app_name(path: &Path, known: &mut HashMap<PathBuf, String>) -> String {
    known
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            file_description(path).unwrap_or_else(|| {
                path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
            })
        })
        .clone()
}

fn file_description(path: &Path) -> Option<String> {
    let file = wide(path.as_os_str());
    // SAFETY: the version resource is read into a buffer of the size Windows asked for, and values are only read
    // inside it, at the lengths Windows reports.
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
        // The first language and code page the file lists.
        let (mut value, mut len): (*mut c_void, u32) = (null_mut(), 0);
        let query = wide(r"\VarFileInfo\Translation".as_ref());
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut value, &mut len).as_bool()
            || len < 4
        {
            return None;
        }
        let [language, code_page] = value.cast::<[u16; 2]>().read_unaligned();
        let key = format!(r"\StringFileInfo\{language:04x}{code_page:04x}\FileDescription");
        let query = wide(key.as_ref());
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut value, &mut len).as_bool()
            || len == 0
        {
            return None;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(value.cast::<u16>(), len as usize));
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

/// A NUL-terminated wide string, for Windows.
pub(crate) fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// A wide string from a fixed buffer, up to its first zero.
fn wide_text(buffer: &[u16]) -> String {
    String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len())])
}

pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<crate::Thumbnail>>, CaptureError> {
    preview::thumbnails(targets, max_width)
}

pub fn screenshot(target: Target) -> Result<crate::Screenshot, CaptureError> {
    preview::screenshot(target)
}

pub fn microphones() -> Vec<Device> {
    audio::microphones()
}

pub fn cameras() -> Vec<Device> {
    camera::cameras()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen is shared: tests that record it or click on it take turns, so one's window doesn't cover another's.
    static SCREEN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn screen() -> std::sync::MutexGuard<'static, ()> {
        SCREEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn lists_the_displays_with_the_main_one_first() {
        let sources = sources().unwrap();
        let Some(main) = sources.displays.first() else {
            eprintln!("skipped: this session has no display");
            return;
        };
        assert_eq!(main.name, "Main display");
        assert!(main.width >= 320.0 && main.height >= 200.0, "{main:?}");
        assert!(monitor(None).is_ok());
        assert!(monitor(Some(main.id)).is_ok_and(|m| m.number == main.id));
        // This process has no windows, and nothing listed is this process's.
        assert!(sources.windows.iter().all(|w| w.width >= 80.0 && !w.title.trim().is_empty()));
    }

    #[test]
    fn lists_microphones_with_the_default_first() {
        let microphones = microphones();
        // A computer may have none; any it has are named, and only the first can be the default.
        assert!(microphones.iter().all(|m| !m.id.is_empty() && !m.name.is_empty()));
        assert!(microphones.iter().skip(1).all(|m| !m.is_default));
    }

    /// Records the main display for a moment. It needs a desktop session that can be captured and Media Foundation's
    /// H.264 encoder, so CI runs it on its own: `cargo test -p shotmaxxing-capture -- --ignored`.
    #[test]
    #[ignore = "records the screen"]
    fn records_the_main_display() {
        let _screen = screen();
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("screen.mp4");
        let mut options = crate::RecordOptions::new(Target::Display(None), &output);
        options.system_audio = Some(dir.path().join("system.m4a"));
        let recorder = Recorder::start(options).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(recorder.elapsed() >= 1.4);
        let recording = recorder.stop().unwrap();
        eprintln!("{} frames, {} µs, {:?}", recording.frames, recording.duration_us, recording.warnings);
        // The computer's sound is recorded, or the recording says why not (a server may have no sound output).
        match &recording.system_audio {
            Some(track) => eprintln!("sound: {} µs from {} µs", track.duration_us, track.offset_us),
            None => eprintln!("no sound recorded"),
        }
        assert!(recording.frames >= 1, "no frames arrived");
        assert!(recording.duration_us >= 1_000_000, "{} µs", recording.duration_us);
        assert!(recording.width > 0.0 && recording.scale >= 1.0);
        let bytes = std::fs::metadata(&output).unwrap().len();
        assert!(bytes > 1000, "the file has {bytes} bytes");
    }

    /// A window of this test's own that repaints in a new colour about every 8 ms, as a video or a scrolling page
    /// changes, until `stop` is set. Gives its id.
    fn animated_window(
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> (u32, std::thread::JoinHandle<()>) {
        use windows::Win32::Foundation::{COLORREF, LPARAM, LRESULT, WPARAM};
        use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, GetDC, ReleaseDC};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, MSG, PM_REMOVE,
            PeekMessageW, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE,
        };
        use windows::core::w;

        unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            // SAFETY: hands the message on as Windows gave it.
            unsafe { DefWindowProcW(window, message, w, l) }
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            // SAFETY: a window this thread creates, paints through its own device context and destroys.
            unsafe {
                let instance = GetModuleHandleW(None).unwrap();
                let class = w!("ShotmaxxingFrameRateTest");
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    ..Default::default()
                });
                let window = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    class,
                    w!("Shotmaxxing frame rate test"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    80,
                    80,
                    800,
                    500,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .unwrap();
                sender.send(window.0 as usize as u32).unwrap();
                let mut tick = 0u32;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let mut message = MSG::default();
                    while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    let mut area = RECT::default();
                    let _ = GetClientRect(window, &mut area);
                    let colour = (tick * 7 % 256) | ((tick * 3 % 256) << 8) | ((255 - tick * 5 % 256) << 16);
                    let brush = CreateSolidBrush(COLORREF(colour));
                    let dc = GetDC(Some(window));
                    FillRect(dc, &area, brush);
                    ReleaseDC(Some(window), dc);
                    let _ = DeleteObject(brush.into());
                    tick = tick.wrapping_add(1);
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
                let _ = DestroyWindow(window);
            }
        });
        (receiver.recv().unwrap(), handle)
    }

    /// Records a window that changes all the time, then the display it's on, and says how many frames a second each
    /// kept. Recordings came out at 4 a second on some computers: this keeps them at the screen's pace.
    #[test]
    #[ignore = "records the screen"]
    fn keeps_up_with_a_screen_that_changes_all_the_time() {
        let _screen = screen();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (window, painter) = animated_window(stop.clone());
        std::thread::sleep(std::time::Duration::from_millis(500));
        let dir = tempfile::tempdir().unwrap();
        let mut rates = Vec::new();
        for (name, target) in [("window", Target::Window(window)), ("display", Target::Display(None))] {
            let output = dir.path().join(format!("{name}.mp4"));
            let recorder = Recorder::start(crate::RecordOptions::new(target, &output)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(3));
            let recording = recorder.stop().unwrap();
            let seconds = recording.duration_us as f64 / 1e6;
            let fps = recording.frames as f64 / seconds.max(1e-3);
            eprintln!(
                "{name}: {} frames in {seconds:.2} s = {fps:.1} fps, {} dropped, {:?}",
                recording.frames, recording.dropped, recording.warnings
            );
            rates.push((name, fps));
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        painter.join().unwrap();
        for (name, fps) in rates {
            assert!(fps >= 24.0, "the {name} recorded at {fps:.1} frames a second");
        }
    }

    /// Clicks the left button at `(x, y)`, in pixels on the virtual screen, as a person would.
    fn click_at(x: i32, y: i32) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
        };
        use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
        let press = |flags| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, ..Default::default() } },
        };
        // SAFETY: moves the pointer and sends two complete mouse inputs.
        unsafe {
            let _ = SetCursorPos(x, y);
            SendInput(&[press(MOUSEEVENTF_LEFTDOWN), press(MOUSEEVENTF_LEFTUP)], size_of::<INPUT>() as i32);
        }
    }

    /// Films Notepad the way the app records a window by default: with the input hooks, the computer's sound and the
    /// microphone, into a project, while clicking in it. A runner without sound devices or a microphone still runs
    /// the capture, the hooks and Windows 11's process loopback, which records without a default output device.
    #[test]
    #[ignore = "records a window and clicks in it"]
    fn records_a_window_as_the_app_does() {
        record_notepad(true);
    }

    /// The same without any sound, to tell the picture's part from the sound's.
    #[test]
    #[ignore = "records a window and clicks in it"]
    fn records_a_window_with_clicks_and_no_sound() {
        record_notepad(false);
    }

    fn record_notepad(sound: bool) {
        let _screen = screen();
        let _ = std::process::Command::new("notepad.exe").spawn();
        let mut found = None;
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            found = sources().unwrap().windows.into_iter().find(|w| w.bundle_id == "notepad.exe");
            if found.is_some() {
                break;
            }
        }
        let quit_notepad = || {
            let _ = std::process::Command::new("taskkill").args(["/IM", "notepad.exe", "/F"]).status();
        };
        let Some(window) = found else {
            quit_notepad();
            eprintln!("skipped: Notepad's window didn't show");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let mut options = crate::ProjectOptions::new(Target::Window(window.id));
        if sound {
            options.system_audio = true;
            options.microphone = crate::DeviceChoice::Default;
        }
        let recording = crate::ProjectRecording::start(dir.path(), "Notepad", options);
        let recording = match recording {
            Ok(recording) => recording,
            Err(error) => {
                quit_notepad();
                panic!("the recording didn't start: {error}");
            }
        };
        let bounds = window_bounds(HWND(window.id as usize as *mut c_void)).unwrap();
        let (x, y) = ((bounds.left + bounds.right) / 2, (bounds.top + bounds.bottom) / 2);
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            click_at(x, y);
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        let finished = recording.stop();
        quit_notepad();
        let finished = finished.unwrap();
        let tracks: Vec<_> = finished.project.tracks.iter().map(|t| (t.kind, t.duration_us)).collect();
        eprintln!("{} frames, tracks {tracks:?}, warnings {:?}", finished.frames, finished.warnings);
        let events = std::fs::read_to_string(finished.root.join(crate::EVENTS_FILE)).unwrap();
        let clicks = events.lines().filter(|line| line.contains("mouse_down")).count();
        eprintln!("{clicks} clicks in the telemetry");
        assert!(finished.frames >= 1, "no frames arrived");
        assert!(clicks >= 3, "the input hooks saw {clicks} of 3 clicks:\n{events}");
    }
}
