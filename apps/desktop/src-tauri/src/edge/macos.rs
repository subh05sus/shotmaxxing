//! macOS: borderless windows at the status-bar level, over full-screen apps and the menu bar, on every Space.
//! Recordings leave them out with the rest of Shotmaxxing's windows.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior,
    NSWindowSharingType, NSWindowStyleMask,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};

use super::THICKNESS;

thread_local! {
    /// The edge's windows, made and closed on the main thread.
    static STRIPS: RefCell<Vec<Retained<NSWindow>>> = const { RefCell::new(Vec::new()) };
}

pub fn show() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let red = NSColor::systemRedColor().colorWithAlphaComponent(0.9);
    let screens = NSScreen::screens(mtm);
    STRIPS.with_borrow_mut(|strips| {
        for screen in screens.iter() {
            let frame = screen.frame();
            let (x, y, w, h, t) =
                (frame.origin.x, frame.origin.y, frame.size.width, frame.size.height, THICKNESS);
            // Along the top and bottom, then down the sides between them.
            for rect in [
                CGRect::new(CGPoint::new(x, y + h - t), CGSize::new(w, t)),
                CGRect::new(CGPoint::new(x, y), CGSize::new(w, t)),
                CGRect::new(CGPoint::new(x, y + t), CGSize::new(t, h - 2.0 * t)),
                CGRect::new(CGPoint::new(x + w - t, y + t), CGSize::new(t, h - 2.0 * t)),
            ] {
                strips.push(strip(mtm, rect, &red));
            }
        }
    });
}

/// One side of a screen's edge: borderless, clicked through, over everything including full-screen apps and the
/// menu bar, on every Space, and kept out of other apps' screen captures where the system allows.
fn strip(mtm: MainThreadMarker, rect: CGRect, color: &NSColor) -> Retained<NSWindow> {
    // SAFETY: a borderless window made on the main thread (`mtm`), kept until `hide` closes it, so AppKit mustn't
    // release it on close as well.
    let window = unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window
    };
    window.setOpaque(false);
    window.setBackgroundColor(Some(color));
    window.setHasShadow(false);
    window.setIgnoresMouseEvents(true);
    window.setLevel(NSStatusWindowLevel);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    window.setSharingType(NSWindowSharingType::None);
    window.orderFrontRegardless();
    window
}

pub fn hide() {
    STRIPS.with_borrow_mut(|strips| {
        for strip in strips.drain(..) {
            strip.orderOut(None);
            strip.close();
        }
    });
}
