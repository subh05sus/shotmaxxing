//! Which Space a window is on. macOS has no public way to ask, so this asks the window server through SkyLight's
//! own calls, as window managers do. They're looked up by name when first needed ([`super::symbols`]), not linked:
//! should a macOS drop one, nothing fails, and the picker goes by windows' shapes alone ([`Space::Unknown`]).
//!
//! What they tell apart that nothing public does: a window on a desktop that isn't showing is on that Space, while
//! one minimised, hidden, a tab behind another or a helper that never shows is ordered out, on no Space at all.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::OnceLock;

use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};

use super::symbols;
use super::window::{Space, entry};

type MainConnection = unsafe extern "C" fn() -> i32;
type SpacesForWindows =
    unsafe extern "C" fn(connection: i32, mask: i32, windows: *const c_void) -> *const c_void;
type ManagedDisplaySpaces = unsafe extern "C" fn(connection: i32) -> *const c_void;

/// The current Space, the others and every user's (`kCGSAllSpacesMask`).
const ALL_SPACES: i32 = 7;
/// A desktop's type among a display's Spaces...
const DESKTOP: i64 = 0;
/// ...and a full-screen app's.
const FULL_SCREEN: i64 = 4;

struct SkyLight {
    connection: i32,
    spaces_for_windows: SpacesForWindows,
    managed_display_spaces: ManagedDisplaySpaces,
}

fn skylight() -> Option<&'static SkyLight> {
    static FOUND: OnceLock<Option<SkyLight>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let main = symbols::find(&[c"SLSMainConnectionID", c"CGSMainConnectionID"])?;
            let spaces_for_windows =
                symbols::find(&[c"SLSCopySpacesForWindows", c"CGSCopySpacesForWindows"])?;
            let managed = symbols::find(&[c"SLSCopyManagedDisplaySpaces", c"CGSCopyManagedDisplaySpaces"])?;
            // SAFETY: the signatures these have had since Spaces came (window managers rely on them): the
            // process's connection to the window server; the Spaces an array of window numbers is on, copied; and
            // every display's Spaces, copied.
            unsafe {
                let main = std::mem::transmute::<usize, MainConnection>(main);
                Some(SkyLight {
                    connection: main(),
                    spaces_for_windows: std::mem::transmute::<usize, SpacesForWindows>(spaces_for_windows),
                    managed_display_spaces: std::mem::transmute::<usize, ManagedDisplaySpaces>(managed),
                })
            }
        })
        .as_ref()
}

/// The Spaces as they are now: those showing (one a display), the desktops, and the full-screen apps'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    showing: Vec<i64>,
    desktops: Vec<i64>,
    full_screen: Vec<i64>,
}

impl Layout {
    /// Where a window on `spaces` is.
    fn place(&self, spaces: &[i64]) -> Space {
        if spaces.iter().any(|s| self.showing.contains(s)) {
            Space::Here
        } else if spaces.iter().any(|s| self.desktops.contains(s)) {
            Space::OtherDesktop
        } else if spaces.iter().any(|s| self.full_screen.contains(s)) {
            Space::OtherFullScreen
        } else {
            Space::Nowhere
        }
    }
}

/// Every display's Spaces, when the window server says.
pub fn layout() -> Option<Layout> {
    let skylight = skylight()?;
    // SAFETY: a Copy-rule call, so whatever it gives is ours to release.
    let displays = unsafe { owned_array((skylight.managed_display_spaces)(skylight.connection)) }?;
    let current = CFString::from_static_str("Current Space");
    let spaces = CFString::from_static_str("Spaces");
    let id = CFString::from_static_str("ManagedSpaceID");
    let kind = CFString::from_static_str("type");
    let mut layout = Layout::default();
    for display in items(&displays).filter_map(|item| item.downcast_ref::<CFDictionary>()) {
        if let Some(showing) = value(display, &current).and_then(|v| v.downcast_ref::<CFDictionary>())
            && let Some(showing) = number(showing, &id)
        {
            layout.showing.push(showing);
        }
        let Some(all) = value(display, &spaces).and_then(|v| v.downcast_ref::<CFArray>()) else { continue };
        for space in items(all).filter_map(|item| item.downcast_ref::<CFDictionary>()) {
            match (number(space, &id), number(space, &kind)) {
                (Some(space), Some(DESKTOP)) => layout.desktops.push(space),
                (Some(space), Some(FULL_SCREEN)) => layout.full_screen.push(space),
                _ => {}
            }
        }
    }
    // A layout with nothing showing is one this code doesn't understand: then it says nothing.
    (!layout.showing.is_empty()).then_some(layout)
}

/// Where window `id` is among the Spaces of `layout`.
pub fn space_of(id: u32, layout: &Layout) -> Space {
    let Some(skylight) = skylight() else { return Space::Unknown };
    let number = CFNumber::new_i64(i64::from(id));
    let windows = CFArray::from_objects(&[&*number]);
    let windows = CFRetained::as_ptr(&windows).as_ptr() as *const c_void;
    // SAFETY: a Copy-rule call with an array of window numbers, valid for the call; what it gives is ours.
    let Some(spaces) =
        (unsafe { owned_array((skylight.spaces_for_windows)(skylight.connection, ALL_SPACES, windows)) })
    else {
        return Space::Unknown;
    };
    let spaces: Vec<i64> =
        items(&spaces).filter_map(|item| item.downcast_ref::<CFNumber>()?.as_i64()).collect();
    layout.place(&spaces)
}

/// An array a Copy-rule call gave, when it's one.
///
/// # Safety
/// `array` must be null or a CF object the caller owns.
unsafe fn owned_array(array: *const c_void) -> Option<CFRetained<CFArray>> {
    let object = NonNull::new(array as *mut CFType)?;
    // SAFETY: the caller owns it; `CFRetained` releases it.
    let object = unsafe { CFRetained::from_raw(object) };
    object.downcast::<CFArray>().ok()
}

/// An array's values.
fn items(array: &CFArray) -> impl Iterator<Item = &CFType> {
    // SAFETY: each value of a CF array is a CF object, valid while the array is.
    (0..array.count())
        .filter_map(move |index| unsafe { (array.value_at_index(index) as *const CFType).as_ref() })
}

fn value<'a>(dictionary: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    // SAFETY: the window server's dictionaries are keyed by strings, with CF values.
    unsafe { entry(dictionary, key) }
}

fn number(dictionary: &CFDictionary, key: &CFString) -> Option<i64> {
    value(dictionary, key)?.downcast_ref::<CFNumber>()?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's Mac: one display with the desktop (3) and two full-screen apps' Spaces, Chrome's (646) and
    /// Terminal's (523, showing).
    fn owners_mac() -> Layout {
        Layout { showing: vec![523], desktops: vec![3], full_screen: vec![646, 523] }
    }

    #[test]
    fn windows_are_placed_by_their_spaces() {
        let layout = owners_mac();
        // Terminal's window in front, on the Space showing.
        assert_eq!(layout.place(&[523]), Space::Here);
        // WhatsApp parked on the desktop, which isn't showing.
        assert_eq!(layout.place(&[3]), Space::OtherDesktop);
        // Chrome in full screen on its own Space.
        assert_eq!(layout.place(&[646]), Space::OtherFullScreen);
        // A tab behind another, a minimised window, a helper: ordered out, on no Space.
        assert_eq!(layout.place(&[]), Space::Nowhere);
        // A window on every Space is here too.
        assert_eq!(layout.place(&[3, 646, 523]), Space::Here);
        // A Space that isn't a desktop or a full-screen app's.
        assert_eq!(layout.place(&[9]), Space::Nowhere);
    }
}
