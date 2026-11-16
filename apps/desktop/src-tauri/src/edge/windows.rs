//! Windows: plain Win32 popup windows, topmost, clicked through (layered and transparent to the mouse), never
//! activated, and left out of every screen capture. A strip that can't be left out of captures isn't shown (before
//! Windows 10 version 2004), so the edge never ends up in a video.

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, LWA_ALPHA, RegisterClassW, SW_SHOWNOACTIVATE,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, ShowWindow, WDA_EXCLUDEFROMCAPTURE, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};

use super::THICKNESS;

/// The edge's window class.
const CLASS: PCWSTR = w!("ShotmaxxingEdge");
/// The red, (255, 59, 48) as 0x00bbggrr: the same as macOS's system red.
const RED: COLORREF = COLORREF(0x0030_3BFF);
/// How solid the edge is, out of 255.
const ALPHA: u8 = 230;

thread_local! {
    /// The edge's windows, made and destroyed on the main thread.
    static STRIPS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    /// The window class is registered, once.
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// The strips only paint their background, which the system does by itself.
unsafe extern "system" fn procedure(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: the default handling for a message sent to one of the edge's windows.
    unsafe { DefWindowProcW(window, message, w, l) }
}

/// Registers the edge's window class, painted red, the first time. Whether it's registered.
fn register(instance: HINSTANCE) -> bool {
    REGISTERED.with(|done| {
        if !done.get() {
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                // SAFETY: a solid brush, which the class keeps for the life of the app.
                hbrBackground: unsafe { CreateSolidBrush(RED) },
                lpszClassName: CLASS,
                ..Default::default()
            };
            // SAFETY: the class is described in full above, and its name is static.
            done.set(unsafe { RegisterClassW(&class) } != 0);
        }
        done.get()
    })
}

/// Adds each display, and its rectangle on the desktop in pixels, to the list `data` points to.
unsafe extern "system" fn found(monitor: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the list `show` passes, which outlives the enumeration, and `rect` is the display's
    // rectangle, from the system.
    unsafe {
        let screens = &mut *(data.0 as *mut Vec<(HMONITOR, RECT)>);
        if let Some(rect) = rect.as_ref() {
            screens.push((monitor, *rect));
        }
    }
    true.into()
}

pub fn show() {
    let mut screens: Vec<(HMONITOR, RECT)> = Vec::new();
    // SAFETY: lists the displays into `screens`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(found), LPARAM(&mut screens as *mut _ as isize));
    }
    // SAFETY: this program's own module, which needs no freeing.
    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else { return };
    let instance = HINSTANCE::from(module);
    if !register(instance) {
        tracing::warn!("couldn't show the red edge round the screen: its window class wasn't registered");
        return;
    }
    STRIPS.with_borrow_mut(|strips| {
        for (monitor, r) in screens {
            // The app is aware of each display's scale, so rectangles are in pixels: the edge is scaled to match.
            let (mut dpi, mut unused) = (96u32, 96u32);
            // SAFETY: asks a display the system just listed for its scale.
            if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused) }.is_err() {
                dpi = 96;
            }
            let t = ((THICKNESS * f64::from(dpi) / 96.0).round() as i32).max(1);
            let (x, y, w, h) = (r.left, r.top, r.right - r.left, r.bottom - r.top);
            // Along the top and bottom, then down the sides between them.
            for (x, y, w, h) in [
                (x, y, w, t),
                (x, y + h - t, w, t),
                (x, y + t, t, h - 2 * t),
                (x + w - t, y + t, t, h - 2 * t),
            ] {
                if let Some(strip) = strip(instance, x, y, w, h) {
                    strips.push(strip);
                }
            }
        }
    });
}

/// One side of a display's edge, shown without taking the focus. None when it couldn't be made, or couldn't be
/// left out of screen captures.
fn strip(instance: HINSTANCE, x: i32, y: i32, w: i32, h: i32) -> Option<HWND> {
    // SAFETY: a popup window of the class registered above, made on the main thread, whose message loop serves it.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS,
            w!("Shotmaxxing"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .ok()?;
    // SAFETY: settings on the window just made, from the thread that made it.
    unsafe {
        let ready = SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE).is_ok()
            && SetLayeredWindowAttributes(window, COLORREF(0), ALPHA, LWA_ALPHA).is_ok();
        if !ready {
            let _ = DestroyWindow(window);
            return None;
        }
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
    }
    Some(window)
}

pub fn hide() {
    STRIPS.with_borrow_mut(|strips| {
        for window in strips.drain(..) {
            // SAFETY: a window `show` made on this thread.
            unsafe {
                let _ = DestroyWindow(window);
            }
        }
    });
}
