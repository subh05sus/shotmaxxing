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
mod clock;
mod input;
mod record;

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

pub fn thumbnails(targets: &[Target], _max_width: u32) -> Result<Vec<Option<Vec<u8>>>, CaptureError> {
    Ok(vec![None; targets.len()])
}

pub fn microphones() -> Vec<Device> {
    audio::microphones()
}

pub fn cameras() -> Vec<Device> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
