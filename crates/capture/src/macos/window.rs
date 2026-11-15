//! A window's real size and place.
//!
//! A window's frame isn't always the window. With Stage Manager on, a window parked in the strip at the side of the
//! screen reports the thumbnail's frame: a Finder window reads as about 124 × 111 points, off the left edge.
//! Recording at that size gave a tiny video with the window shrunk into its top (a gap below it), blown up again by
//! the export. ScreenCaptureKit still draws the window itself, at its real size, so a picture's content rect and
//! scale say how big the window really is. Where it is comes from the window server, trusted only while the window
//! shows at that size.

use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use block2::RcBlock;
use objc2_core_foundation::{CFDictionary, CFString, CGRect, CGSize};
use objc2_core_graphics::{
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_core_media::CMSampleBuffer;
use objc2_foundation::{NSError, NSNumber, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCStreamConfiguration, SCStreamFrameInfoContentRect,
    SCStreamFrameInfoContentScale, SCStreamFrameInfoScaleFactor,
};

/// A window's real size in points, and its pixels per point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measured {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// What ScreenCaptureKit attached to a frame: where the content sits in it (in points), how much it was scaled to
/// fit, and pixels per point.
///
/// # Safety
/// `sample` must be a frame from ScreenCaptureKit, valid for the call.
unsafe fn frame_info(sample: &CMSampleBuffer) -> Option<(CGRect, f64, f64)> {
    // SAFETY: the attachments are an array of dictionaries keyed by ScreenCaptureKit's NSString constants (toll-free
    // bridged to CFString); the rect is a CGRect's dictionary form and the scales are numbers.
    unsafe {
        let attachments = sample.sample_attachments_array(false)?;
        if attachments.count() < 1 {
            return None;
        }
        let info = (attachments.value_at_index(0) as *const CFDictionary).as_ref()?;
        let get = |key: &NSString| info.value((key as *const NSString).cast());
        let mut rect = CGRect::default();
        let rect_info = (get(SCStreamFrameInfoContentRect) as *const CFDictionary).as_ref();
        if !CGRectMakeWithDictionaryRepresentation(rect_info, &mut rect) {
            return None;
        }
        let number = |key: &NSString| (get(key) as *const NSNumber).as_ref().map(NSNumber::as_f64);
        Some((
            rect,
            number(SCStreamFrameInfoContentScale)?,
            number(SCStreamFrameInfoScaleFactor).unwrap_or(1.0),
        ))
    }
}

/// Asks for one picture of the window `filter` shows, in a frame of `room` points, and sends its real size (or
/// `None`, when no picture comes) with `index` once it arrives. As OBS does: the content rect divided by the content
/// scale is the window's size in points.
pub fn request_measure(
    filter: &SCContentFilter,
    room: CGSize,
    index: usize,
    answer: Sender<(usize, Option<Measured>)>,
) {
    // SAFETY: the configuration is made and set here; the handler only reads the frame it's given and sends.
    unsafe {
        let scale = f64::from(filter.pointPixelScale()).max(1.0);
        let config = SCStreamConfiguration::new();
        config.setWidth(((room.width * scale).round() as usize).max(2));
        config.setHeight(((room.height * scale).round() as usize).max(2));
        config.setShowsCursor(false);
        // Drawn at its own size when it fits, scaled down only when it doesn't: either way the info says.
        config.setScalesToFit(false);
        let handler = RcBlock::new(move |sample: *mut CMSampleBuffer, _error: *mut NSError| {
            // A frame ScreenCaptureKit just made, valid for the call.
            let info = sample.as_ref().and_then(|sample| frame_info(sample));
            let measured = info.and_then(|(rect, content_scale, scale)| {
                let (width, height) = (rect.size.width / content_scale, rect.size.height / content_scale);
                (content_scale > 0.0 && width >= 1.0 && height >= 1.0).then_some(Measured {
                    width,
                    height,
                    scale,
                })
            });
            let _ = answer.send((index, measured));
        });
        SCScreenshotManager::captureSampleBufferWithFilter_configuration_completionHandler(
            filter,
            &config,
            Some(&handler),
        );
    }
}

/// The real size of the window `filter` shows, from one picture of it in a frame of `room` points (big enough that
/// the window needn't be scaled down, for an exact size). `None` when no picture comes back: the window closed, or
/// the screen is asleep or locked.
pub fn measure(filter: &SCContentFilter, room: CGSize) -> Option<Measured> {
    let (tx, rx) = channel();
    request_measure(filter, room, 0, tx);
    rx.recv_timeout(Duration::from_secs(3)).ok().and_then(|(_, measured)| measured)
}

/// A window's frame on the desktop, in points from the top left, as the window server has it now.
pub fn bounds(id: u32) -> Option<CGRect> {
    let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionIncludingWindow, id)?;
    if list.count() < 1 {
        return None;
    }
    // SAFETY: the window server answers with an array of dictionaries keyed by its CFString constants, the bounds
    // being a CGRect's dictionary form.
    unsafe {
        let info = (list.value_at_index(0) as *const CFDictionary).as_ref()?;
        let key: &CFString = objc2_core_graphics::kCGWindowBounds;
        let found = (info.value((key as *const CFString).cast()) as *const CFDictionary).as_ref();
        let mut rect = CGRect::default();
        CGRectMakeWithDictionaryRepresentation(found, &mut rect).then_some(rect)
    }
}

/// Whether `frame` is the window at its real size, not a Stage Manager thumbnail or a frame mid-animation.
pub fn at_real_size(frame: CGRect, size: (f64, f64)) -> bool {
    (frame.size.width - size.0).abs() <= 2.0 && (frame.size.height - size.1).abs() <= 2.0
}

#[cfg(test)]
mod tests {
    use objc2::AllocAnyThread;

    use super::*;

    #[test]
    fn a_thumbnail_frame_is_not_the_real_size() {
        let frame = |w: f64, h: f64| {
            CGRect::new(objc2_core_foundation::CGPoint::new(-201.0, 419.0), CGSize::new(w, h))
        };
        assert!(at_real_size(frame(860.0, 501.5), (860.0, 500.0)));
        assert!(!at_real_size(frame(147.0, 97.0), (860.0, 500.0)), "a Stage Manager thumbnail");
    }

    /// Run by hand on a Mac with Screen Recording allowed for the terminal, ideally with Stage Manager on and a
    /// window parked in its strip: every window measures at least its frame, and a parked one its real size.
    #[test]
    #[ignore = "pictures the windows on screen"]
    fn windows_measure_at_least_their_frame() {
        super::super::ensure_window_server();
        if !super::super::has_permission() {
            eprintln!("Screen Recording isn't allowed for this process: skipped");
            return;
        }
        let content = super::super::shareable_content().expect("the windows");
        let room = super::super::room(&content);
        // SAFETY: read-only use of `content`, and a filter made from one of its windows.
        unsafe {
            for w in content.windows().iter().filter(|w| w.windowLayer() == 0 && w.isOnScreen()) {
                let Some(app) = w.owningApplication() else { continue };
                let frame = w.frame();
                if frame.size.width < 80.0 || frame.size.height < 60.0 {
                    continue;
                }
                let filter = SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &w);
                let real = measure(&filter, room);
                eprintln!(
                    "{} #{}: frame {:.0}×{:.0} at ({:.0}, {:.0}), measured {:?}, window server {:?}",
                    app.applicationName(),
                    w.windowID(),
                    frame.size.width,
                    frame.size.height,
                    frame.origin.x,
                    frame.origin.y,
                    real.map(|r| (r.width.round(), r.height.round(), r.scale)),
                    bounds(w.windowID()).map(|b| (b.origin.x, b.origin.y, b.size.width, b.size.height)),
                );
                if let Some(real) = real {
                    assert!(real.width >= frame.size.width - 2.0 && real.height >= frame.size.height - 2.0);
                }
            }
        }
    }
}
