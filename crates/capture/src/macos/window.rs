//! Where a window is, and Stage Manager.
//!
//! With Stage Manager on, only the active app's windows are on the stage, and the rest wait in a strip at the side
//! of the screen as small tilted pictures. A window parked there reports the thumbnail's frame (a Finder window
//! reads as about 124 × 111 points, off the left edge), ScreenCaptureKit pictures it small, tilted and soft, and
//! while it isn't changing it isn't drawn at all, so a picture of it may never come. Nothing says how big it really
//! is: a picture's frame info would, but window pictures carry none (macOS 27), and a stream of a still, parked window
//! sends no frames. So a window is brought back to the stage before it's pictured or recorded ([`bring_forward`]):
//! its app is asked to come forward, and the window slides back to its place in about half a second, where its frame
//! is its own again. The picker, which mustn't move windows about, tells a parked window by where it is
//! ([`in_strip`]).

use std::time::{Duration, Instant};

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFPreferencesCopyAppValue, CFRetained, CFString, CFType, CGRect,
};
use objc2_core_graphics::{
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowBounds,
    kCGWindowIsOnscreen, kCGWindowOwnerPID,
};

/// A window as the window server has it now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its frame on the desktop, in points from the top left.
    pub bounds: CGRect,
    /// Showing on this desktop: not minimised, hidden, or on another desktop.
    pub on_screen: bool,
    /// The process that owns it.
    pub pid: i32,
}

/// A value in one of the window server's window dictionaries.
///
/// # Safety
/// `info` must be a dictionary from `CGWindowListCopyWindowInfo`.
unsafe fn entry<'a>(info: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    // SAFETY: the dictionary's values are CF objects, valid while it is.
    unsafe { (info.value((key as *const CFString).cast()) as *const CFType).as_ref() }
}

/// Where the window is, whether it shows, and whose it is, as the window server has it now. `None` once it has
/// closed.
pub fn placed(id: u32) -> Option<Placed> {
    let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionIncludingWindow, id)?;
    if list.count() < 1 {
        return None;
    }
    // SAFETY: the window server answers with an array of dictionaries keyed by its CFString constants: the bounds a
    // CGRect's dictionary form, the owner a number, and on-screen a boolean that's left out when it isn't.
    unsafe {
        let info = (list.value_at_index(0) as *const CFDictionary).as_ref()?;
        let found =
            entry(info, kCGWindowBounds).map(|value| &*(value as *const CFType).cast::<CFDictionary>());
        let mut bounds = CGRect::default();
        if !CGRectMakeWithDictionaryRepresentation(found, &mut bounds) {
            return None;
        }
        let pid = entry(info, kCGWindowOwnerPID)?.downcast_ref::<CFNumber>()?.as_i32()?;
        let on_screen = entry(info, kCGWindowIsOnscreen)
            .and_then(|value| value.downcast_ref::<CFBoolean>())
            .is_some_and(CFBoolean::as_bool);
        Some(Placed { bounds, on_screen, pid })
    }
}

/// A window's frame on the desktop, in points from the top left, as the window server has it now.
pub fn bounds(id: u32) -> Option<CGRect> {
    placed(id).map(|placed| placed.bounds)
}

/// One of another app's settings, as the person has it.
fn preference(domain: &'static str, key: &'static str) -> Option<CFRetained<CFType>> {
    CFPreferencesCopyAppValue(&CFString::from_static_str(key), &CFString::from_static_str(domain))
}

/// Whether Stage Manager is on.
pub fn stage_manager_on() -> bool {
    let Some(value) = preference("com.apple.WindowManager", "GloballyEnabled") else { return false };
    // A boolean, or a number when it was set by hand.
    match value.downcast_ref::<CFBoolean>() {
        Some(flag) => flag.as_bool(),
        None => value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64).is_some_and(|n| n != 0),
    }
}

/// Whether Stage Manager keeps its strip on the right of the screen: it does when the Dock is on the left.
pub fn strip_on_right() -> bool {
    preference("com.apple.dock", "orientation")
        .and_then(|value| value.downcast_ref::<CFString>().map(ToString::to_string))
        .is_some_and(|side| side == "left")
}

/// The most a strip thumbnail measures either way, in points. Those seen were 92 × 107 to 147 × 97.
const THUMBNAIL_MOST: f64 = 300.0;
/// How far from the screen's edge a strip thumbnail's middle can be, in points, either way: they sit a few dozen
/// points in, and a tilted one can hang off the edge.
const STRIP_BAND: f64 = 260.0;

/// Whether a window with this frame is in Stage Manager's strip, going by where it is: small, with its middle in
/// the band along the strip's side (the right when `right`) of one of `displays` (their frames). Only meaningful with
/// Stage Manager on, where windows on the stage keep out of that band.
pub fn in_strip(frame: CGRect, displays: &[CGRect], right: bool) -> bool {
    let (w, h) = (frame.size.width, frame.size.height);
    if w > THUMBNAIL_MOST || h > THUMBNAIL_MOST {
        return false;
    }
    let (x, y) = (frame.origin.x + w / 2.0, frame.origin.y + h / 2.0);
    displays.iter().any(|display| {
        let edge = if right { display.origin.x + display.size.width } else { display.origin.x };
        (x - edge).abs() <= STRIP_BAND && y >= display.origin.y && y <= display.origin.y + display.size.height
    })
}

/// Brings a window that Stage Manager parked in its strip back to the stage, so it's pictured or recorded upright at
/// its real size, and waits (at most two seconds) for it to settle there. Its app is asked to come forward, which
/// macOS allows while Shotmaxxing is the active app, as it is when someone has just clicked Record or picked a
/// window. Does nothing with Stage Manager off or the app already in front, so nothing moves without need.
pub fn bring_forward(id: u32) {
    if !stage_manager_on() {
        return;
    }
    let Some(before) = placed(id) else { return };
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(before.pid) else { return };
    if app.isActive() {
        return;
    }
    if !app.activateWithOptions(NSApplicationActivationOptions::empty()) {
        tracing::debug!(id, "the window's app didn't come forward");
        return;
    }
    settle(id, before.bounds, Duration::from_secs(2));
}

/// Waits for a window to come to rest after its app was asked forward: once it has started to move, until its frame
/// reads the same three times in a row 30 ms apart; and when it hasn't moved within 0.4 s, it was already in place.
fn settle(id: u32, before: CGRect, most: Duration) {
    let start = Instant::now();
    let (mut last, mut same, mut moved) = (Some(before), 0, false);
    while start.elapsed() < most {
        std::thread::sleep(Duration::from_millis(30));
        let now = bounds(id);
        if now == last {
            same += 1;
        } else {
            moved = true;
            (last, same) = (now, 0);
        }
        if (moved && same >= 3) || (!moved && start.elapsed() >= Duration::from_millis(400)) {
            return;
        }
    }
}

/// Whether `frame` is a Stage Manager strip thumbnail of a window whose real size is `size`. A thumbnail is a sixth
/// of the window or so and never big, where a window someone resized rarely shrinks below half both ways.
pub fn thumbnail_of(frame: CGRect, size: (f64, f64)) -> bool {
    let (w, h) = (frame.size.width, frame.size.height);
    w < size.0 * 0.5 && h < size.1 * 0.5 && w <= THUMBNAIL_MOST && h <= THUMBNAIL_MOST
}

/// Whether `frame` is the window at its real size, not a Stage Manager thumbnail or a frame mid-animation.
pub fn at_real_size(frame: CGRect, size: (f64, f64)) -> bool {
    (frame.size.width - size.0).abs() <= 2.0 && (frame.size.height - size.1).abs() <= 2.0
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGSize};

    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn a_thumbnail_frame_is_not_the_real_size() {
        assert!(at_real_size(rect(-201.0, 419.0, 860.0, 501.5), (860.0, 500.0)));
        assert!(!at_real_size(rect(-201.0, 419.0, 147.0, 97.0), (860.0, 500.0)), "a Stage Manager thumbnail");
    }

    #[test]
    fn strip_thumbnails_are_told_from_resized_windows() {
        let frame = |w: f64, h: f64| rect(32.0, 661.0, w, h);
        // Thumbnails seen in the strip, for windows of 600 × 412, 860 × 500 and 920 × 464 points.
        assert!(thumbnail_of(frame(92.0, 114.0), (600.0, 412.0)));
        assert!(thumbnail_of(frame(147.0, 97.0), (860.0, 500.0)));
        assert!(thumbnail_of(frame(124.0, 111.0), (920.0, 464.0)));
        // A window made smaller, narrower or shorter is followed instead.
        assert!(!thumbnail_of(frame(500.0, 300.0), (920.0, 464.0)));
        assert!(!thumbnail_of(frame(300.0, 460.0), (920.0, 464.0)));
        assert!(
            !thumbnail_of(rect(32.0, 661.0, 400.0, 180.0), (920.0, 464.0)),
            "under half both ways, but too big for one"
        );
        // A small window is never mistaken for its own thumbnail.
        assert!(!thumbnail_of(frame(200.0, 150.0), (240.0, 180.0)));
    }

    #[test]
    fn strip_thumbnails_are_found_by_where_they_are() {
        let laptop = rect(0.0, 0.0, 1470.0, 956.0);
        let beside = rect(1470.0, 0.0, 1920.0, 1080.0);
        let displays = [laptop, beside];
        // Thumbnails seen in the strip on a laptop's display, one of them tilted off its edge.
        for parked in [
            rect(32.0, 661.0, 92.0, 114.0),
            rect(15.0, 661.0, 95.0, 114.0),
            rect(32.0, 589.0, 92.0, 107.0),
            rect(-201.0, 419.0, 147.0, 97.0),
        ] {
            assert!(in_strip(parked, &displays, false), "{parked:?}");
        }
        // Windows on the stage: a normal one, a small one in the middle, a small one on the side with no strip.
        assert!(!in_strip(rect(319.0, 304.0, 600.0, 412.0), &displays, false));
        assert!(!in_strip(rect(600.0, 400.0, 200.0, 150.0), &displays, false));
        assert!(!in_strip(rect(1300.0, 400.0, 150.0, 120.0), &[laptop], false));
        // With the Dock on the left, the strip is on the right.
        assert!(in_strip(rect(1300.0, 400.0, 150.0, 120.0), &[laptop], true));
        assert!(!in_strip(rect(32.0, 661.0, 92.0, 114.0), &[laptop], true));
        // Each display has its own strip.
        assert!(in_strip(rect(1500.0, 500.0, 100.0, 100.0), &displays, false));
        // Below every display isn't in a strip.
        assert!(!in_strip(rect(20.0, 1200.0, 100.0, 100.0), &[laptop], false));
    }
}
