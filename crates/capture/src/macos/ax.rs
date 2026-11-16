//! Accessibility: what apps say about their own windows and controls. It needs the person's permission (Privacy &
//! Security › Accessibility), which is optional, so everything that asks has a way to do without.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use objc2_core_foundation::{CFBoolean, CFData, CFRetained, CFString, CGPoint, CGRect, CGSize};

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
    fn AXUIElementSetAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *const c_void,
    ) -> i32;
    fn AXUIElementPerformAction(element: *const c_void, action: *const c_void) -> i32;
    pub(super) fn AXUIElementSetMessagingTimeout(element: *const c_void, seconds: f32) -> i32;
    fn AXValueGetValue(value: *const c_void, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(super) fn CFRelease(object: *const c_void);
    fn CFRetain(object: *const c_void) -> *const c_void;
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
}

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
    // ApplicationServices is loaded, by the link above.
    let address = *FOUND.get_or_init(|| super::symbols::find(&[c"_AXUIElementGetWindow"]));
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
    let window = listed(id, pid, get_window()?)?;
    let position: Pair = geometry_attribute(window.0, "AXPosition", AX_VALUE_CGPOINT)?;
    let size: Pair = geometry_attribute(window.0, "AXSize", AX_VALUE_CGSIZE)?;
    Some(CGRect::new(CGPoint::new(position.a, position.b), CGSize::new(size.a, size.b)))
}

/// One of an app's windows as Accessibility has it, released when dropped.
pub struct Window(*const c_void);

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the element is ours, from a Create rule call or retained, and released only here.
        unsafe { CFRelease(self.0) };
    }
}

impl Window {
    /// Asks its app to make it the main window and raise it above the app's other windows, as clicking it would.
    /// With its app in front, macOS then shows the Space it's on. Whether the app took the raise.
    pub fn raise(&self) -> bool {
        let (main, raise) = (CFString::from_str("AXMain"), CFString::from_str("AXRaise"));
        let yes: *const CFBoolean = CFBoolean::new(true);
        // SAFETY: a valid element, an attribute and an action name that live for the calls, and the shared true
        // value.
        unsafe {
            AXUIElementSetAttributeValue(self.0, cf(&main), yes.cast());
            AXUIElementPerformAction(self.0, cf(&raise)) == 0
        }
    }
}

/// A CF string as the C calls take it.
fn cf(string: &CFRetained<CFString>) -> *const c_void {
    CFRetained::as_ptr(string).as_ptr() as *const c_void
}

/// Window `id` of app `pid`, wherever it is, when Accessibility is allowed. An app lists only its windows on this
/// Space, so one on another (a full-screen app's, or on another desktop) is looked for among the app's elements by
/// number, as window switchers do ([`numbered`]).
pub fn window(id: u32, pid: i32) -> Option<Window> {
    if !AXIsProcessTrusted() {
        return None;
    }
    let get_window = get_window()?;
    listed(id, pid, get_window).or_else(|| numbered(id, pid, get_window))
}

/// Window `id` among the windows app `pid` lists, those on this Space.
fn listed(id: u32, pid: i32, get_window: GetWindow) -> Option<Window> {
    // SAFETY: an element for the app; released below.
    let app = unsafe { AXUIElementCreateApplication(pid) };
    if app.is_null() {
        return None;
    }
    // SAFETY: a short timeout, so a busy app can't hold up a take.
    unsafe { AXUIElementSetMessagingTimeout(app, 0.25) };
    let found = copy_attribute(app, "AXWindows").and_then(|windows| {
        // SAFETY: `windows` is ours to release; it's read as an array only when it is one, whose values are window
        // elements, valid while it is, and the one kept is retained first.
        unsafe {
            let mut found = None;
            if CFGetTypeID(windows) == CFArrayGetTypeID() {
                for index in 0..CFArrayGetCount(windows) {
                    let window = CFArrayGetValueAtIndex(windows, index);
                    let mut number = 0u32;
                    if get_window(window, &mut number) == 0 && number == id {
                        found = Some(Window(CFRetain(window)));
                        break;
                    }
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

/// How long an app's elements are searched for a window that isn't on this Space.
const SEARCH: Duration = Duration::from_millis(400);
/// The most element numbers tried. An app numbers its elements as Accessibility first asks about them, so a window of
/// an app that has run a long while can have a high one; [`SEARCH`] usually ends the search first.
const ELEMENTS: u64 = 50_000;
/// How long one element has to answer, so an app that doesn't can't hold the search up.
const ELEMENT_TIMEOUT: f32 = 0.1;
/// The tag in the tokens of AppKit's elements: "coco".
const COCOA: u32 = 0x636f_636f;

/// Makes an app's element from its token: HIServices' `_AXUIElementCreateWithRemoteToken`. It isn't public (window
/// switchers have used it for years), so it's looked up rather than linked: should a macOS drop it, a window on
/// another Space just isn't found this way.
type FromToken = unsafe extern "C" fn(token: *const c_void) -> *const c_void;

fn from_token() -> Option<FromToken> {
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    let address = *FOUND.get_or_init(|| super::symbols::find(&[c"_AXUIElementCreateWithRemoteToken"]));
    // SAFETY: the signature it has always had: a CFData token in, a new element (Create rule) or null out.
    address.map(|address| unsafe { std::mem::transmute::<usize, FromToken>(address) })
}

/// The token that names element number `element` of app `pid`: the process, four zero bytes, AppKit's tag and the
/// number, in the Mac's byte order.
fn token(pid: i32, element: u64) -> [u8; 20] {
    let mut token = [0u8; 20];
    token[0..4].copy_from_slice(&pid.to_ne_bytes());
    token[8..12].copy_from_slice(&COCOA.to_ne_bytes());
    token[12..20].copy_from_slice(&element.to_ne_bytes());
    token
}

/// Window `id` among app `pid`'s elements, tried by number for at most [`SEARCH`].
fn numbered(id: u32, pid: i32, get_window: GetWindow) -> Option<Window> {
    let from_token = from_token()?;
    let start = Instant::now();
    for element in 0..ELEMENTS {
        if start.elapsed() >= SEARCH {
            break;
        }
        let token = CFData::from_bytes(&token(pid, element));
        // SAFETY: the token is a CFData that lives for the call; what comes back is ours (Create rule) or null.
        let made = unsafe { from_token(CFRetained::as_ptr(&token).as_ptr() as *const c_void) };
        if made.is_null() {
            continue;
        }
        let candidate = Window(made);
        // SAFETY: a short timeout on the element just made.
        unsafe { AXUIElementSetMessagingTimeout(candidate.0, ELEMENT_TIMEOUT) };
        let mut number = 0u32;
        // SAFETY: a valid element and an out pointer of ours.
        let in_window = unsafe { get_window(candidate.0, &mut number) } == 0 && number == id;
        // A window's controls say they're in it too: only the window itself is kept.
        if in_window && string_attribute(candidate.0, "AXRole").as_deref() == Some("AXWindow") {
            let ms = start.elapsed().as_millis() as u64;
            tracing::debug!(id, element, ms, "found a window among its app's elements");
            return Some(candidate);
        }
    }
    let ms = start.elapsed().as_millis() as u64;
    tracing::debug!(id, ms, "a window wasn't found among its app's elements");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_endian = "little")]
    fn an_element_token_names_the_process_appkit_and_the_number() {
        let token = token(0x0102_0304, 0x1122_3344_5566_7788);
        assert_eq!(&token[0..4], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&token[4..8], &[0, 0, 0, 0]);
        assert_eq!(&token[8..12], b"ococ");
        assert_eq!(&token[12..20], &[0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]);
    }
}
