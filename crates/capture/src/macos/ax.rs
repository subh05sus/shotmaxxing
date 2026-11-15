//! Accessibility: what apps say about their own windows and controls. It needs the person's permission (Privacy &
//! Security › Accessibility), which is optional, so everything that asks has a way to do without.

use std::ffi::{c_char, c_void};
use std::sync::OnceLock;

use objc2_core_foundation::{CFRetained, CFString, CGPoint, CGRect, CGSize};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub(super) safe fn AXIsProcessTrusted() -> bool;
    pub(super) fn AXUIElementCreateSystemWide() -> *const c_void;
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    pub(super) fn AXUIElementCopyElementAtPosition(
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
    pub(super) fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(super) fn CFRelease(object: *const c_void);
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// `RTLD_DEFAULT`: look in every image already loaded.
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

pub(super) const AX_VALUE_CGPOINT: u32 = 1;
pub(super) const AX_VALUE_CGSIZE: u32 = 2;

/// Two numbers an AXValue holds: a CGPoint's x and y, or a CGSize's width and height.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub(super) struct Pair {
    pub a: f64,
    pub b: f64,
}

/// Copies one attribute. The caller releases the result.
pub(super) fn copy_attribute(element: *const c_void, name: &str) -> Option<*const c_void> {
    let attribute = CFString::from_str(name);
    let mut value: *const c_void = std::ptr::null();
    let attribute_ptr = CFRetained::as_ptr(&attribute).as_ptr() as *const c_void;
    // SAFETY: valid element and attribute; the out pointer is ours.
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute_ptr, &mut value) };
    (status == 0 && !value.is_null()).then_some(value)
}

/// A text attribute, trimmed, when it's short enough to be a label.
pub(super) fn string_attribute(element: *const c_void, name: &str) -> Option<String> {
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

/// A point or size attribute, as `kind` says.
pub(super) fn geometry_attribute<T: Default>(element: *const c_void, name: &str, kind: u32) -> Option<T> {
    let value = copy_attribute(element, name)?;
    let mut out = T::default();
    // SAFETY: AXValueGetValue writes a CGPoint or CGSize (matching `kind` and `T`) into `out`.
    let ok = unsafe { AXValueGetValue(value, kind, &mut out as *mut T as *mut c_void) };
    // SAFETY: releasing the value we copied.
    unsafe { CFRelease(value) };
    ok.then_some(out)
}

/// Which window-server window an app's window element is.
type GetWindow = unsafe extern "C" fn(element: *const c_void, id: *mut u32) -> i32;

/// HIServices' `_AXUIElementGetWindow`, the only way to match an app's window to the window server's. It isn't
/// public (window managers have used it for years), so it's looked up rather than linked: should a macOS drop it,
/// nothing fails, a window's own frame just isn't known.
fn get_window() -> Option<GetWindow> {
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    let address = *FOUND.get_or_init(|| {
        // SAFETY: a lookup by name among the images already loaded; ApplicationServices is, by the link above.
        let symbol = unsafe { dlsym(RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr()) };
        (!symbol.is_null()).then_some(symbol as usize)
    });
    // SAFETY: the function has had this signature since it appeared (macOS 10.4 era): an element and an out
    // pointer for the window number, returning an AXError.
    address.map(|address| unsafe { std::mem::transmute::<usize, GetWindow>(address) })
}

/// Window `id`'s frame as its app has it, on the desktop in points from the top left, when Accessibility is allowed
/// and the window is on this Space (an app lists only those). Under Stage Manager the window server can report a
/// window smaller than this (a strip thumbnail, or shrunk while its app is behind); its app always knows its size.
pub fn own_frame(id: u32, pid: i32) -> Option<CGRect> {
    if !AXIsProcessTrusted() {
        return None;
    }
    let get_window = get_window()?;
    // SAFETY: an element for the app; released below.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    // SAFETY: a short timeout, so a busy app can't hold up a take.
    unsafe { AXUIElementSetMessagingTimeout(app, 0.25) };
    let found = copy_attribute(app, "AXWindows").and_then(|windows| {
        // SAFETY: `windows` is ours to release; it's read as an array only when it is one, whose values are window
        // elements, valid while it is.
        unsafe {
            let mut found = None;
            if CFGetTypeID(windows) == CFArrayGetTypeID() {
                for index in 0..CFArrayGetCount(windows) {
                    let window = CFArrayGetValueAtIndex(windows, index);
                    let mut number = 0u32;
                    if get_window(window, &mut number) != 0 || number != id {
                        continue;
                    }
                    let position: Option<Pair> = geometry_attribute(window, "AXPosition", AX_VALUE_CGPOINT);
                    let size: Option<Pair> = geometry_attribute(window, "AXSize", AX_VALUE_CGSIZE);
                    if let (Some(p), Some(s)) = (position, size) {
                        found = Some(CGRect::new(CGPoint::new(p.a, p.b), CGSize::new(s.a, s.b)));
                    }
                    break;
                }
            }
            CFRelease(windows);
            found
        }
    });
    // SAFETY: releasing the app element.
    unsafe { CFRelease(app) };
    found
}
