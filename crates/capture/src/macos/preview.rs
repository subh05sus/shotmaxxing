//! Small pictures of screens and windows for the source picker, and the microphones and cameras on this Mac.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo};
use objc2_core_foundation::{CFRetained, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGImage, kCGColorSpaceSRGB};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{SCContentFilter, SCScreenshotManager, SCStreamConfiguration};

use super::{crop_for, display_frames, not_showing, shareable_content, window};
use crate::{CaptureError, Device, Screenshot, Target, Thumbnail};

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageDestinationCreateWithData(
        data: *mut c_void,
        kind: *const c_void,
        count: usize,
        options: *const c_void,
    ) -> *mut c_void;
    fn CGImageDestinationAddImage(destination: *mut c_void, image: *const c_void, properties: *const c_void);
    fn CGImageDestinationFinalize(destination: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreateMutable(allocator: *const c_void, capacity: isize) -> *mut c_void;
    fn CFDataGetLength(data: *const c_void) -> isize;
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    fn CFRelease(object: *const c_void);
}

/// Encodes a picture as `kind`, a type such as `public.jpeg` or `public.png`.
fn encode(image: *const c_void, kind: &'static str) -> Option<Vec<u8>> {
    let kind = CFString::from_static_str(kind);
    let kind = CFRetained::as_ptr(&kind).as_ptr() as *const c_void;
    // SAFETY: standard ImageIO calls; every object created here is released before returning.
    unsafe {
        let data = CFDataCreateMutable(std::ptr::null(), 0);
        if data.is_null() {
            return None;
        }
        let destination = CGImageDestinationCreateWithData(data, kind, 1, std::ptr::null());
        if destination.is_null() {
            CFRelease(data);
            return None;
        }
        CGImageDestinationAddImage(destination, image, std::ptr::null());
        let ok = CGImageDestinationFinalize(destination);
        CFRelease(destination);
        let bytes = ok.then(|| {
            let length = CFDataGetLength(data).max(0) as usize;
            std::slice::from_raw_parts(CFDataGetBytePtr(data), length).to_vec()
        });
        CFRelease(data);
        bytes
    }
}

/// An app's icon as a PNG `px` pixels square, for a picker card.
fn app_icon(pid: i32, px: f64) -> Option<Vec<u8>> {
    let icon = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?.icon()?;
    let mut rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(px, px));
    // SAFETY: a valid rect to fill in, with no context or hints.
    let image = unsafe { icon.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    encode(Retained::as_ptr(&image) as *const c_void, "public.png")
}

/// A JPEG of each target, at most `max_width` pixels wide, in the same order, or its app's icon for a window
/// parked in Stage Manager's strip or on another Space (a full-screen app's, which can't be pictured from this one).
/// Targets that can't be pictured (gone) get `None`. All pictures are taken at once and the whole call waits at
/// most about two seconds.
pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<Thumbnail>>, CaptureError> {
    // Every window, wherever it is, so one on another Space is found.
    let content = shareable_content(false)?;
    let displays = display_frames(&content);
    let (stage_manager, strip_on_right) = (window::stage_manager_on(), window::strip_on_right());
    // Each window's frame, owner, and whether it's showing here, to tell a parked or away one.
    // SAFETY: read-only property access on objects owned by `content`.
    let windows: Vec<Option<(CGRect, i32, bool)>> = unsafe {
        targets
            .iter()
            .map(|target| match *target {
                Target::Display(_) => None,
                Target::Window(id) => content.windows().iter().find(|w| w.windowID() == id).and_then(|w| {
                    let pid = w.owningApplication()?.processID();
                    Some((w.frame(), pid, w.isOnScreen()))
                }),
            })
            .collect()
    };
    // SAFETY: read-only use of `content`; each filter is made from one of its displays or windows.
    let filters: Vec<Option<Retained<SCContentFilter>>> = unsafe {
        targets
            .iter()
            .map(|target| match *target {
                Target::Display(id) => {
                    content.displays().iter().find(|d| id.is_none_or(|id| d.displayID() == id)).map(|d| {
                        SCContentFilter::initWithDisplay_excludingWindows(
                            SCContentFilter::alloc(),
                            &d,
                            &NSArray::new(),
                        )
                    })
                }
                Target::Window(id) => {
                    content.windows().iter().find(|w| w.windowID() == id && w.isOnScreen()).map(|w| {
                        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &w)
                    })
                }
            })
            .collect()
    };
    // A window parked in Stage Manager's strip can only be pictured small and tilted, and a still one not at all, and
    // one on another Space can't be pictured from this one: their cards show their app's icon instead.
    let mut out: Vec<Option<Thumbnail>> = vec![None; targets.len()];
    let mut icons: HashMap<i32, Option<Vec<u8>>> = HashMap::new();
    for (index, (target, found)) in targets.iter().zip(&windows).enumerate() {
        let (Target::Window(id), Some((frame, pid, on_screen))) = (*target, *found) else { continue };
        let away = !on_screen;
        // Asking its app is only worth it for a window in the strip's band.
        if away
            || (stage_manager
                && window::in_strip(frame, &displays, strip_on_right)
                && window::parked(id, frame, pid, &displays))
        {
            let icon = icons.entry(pid).or_insert_with(|| app_icon(pid, 128.0)).clone();
            out[index] = Some(if away { Thumbnail::Away { icon } } else { Thumbnail::Parked { icon } });
        }
    }

    let (tx, rx) = channel::<(usize, Option<Vec<u8>>)>();
    let mut pending = 0;
    // SAFETY: each request gets its own filter, configuration and handler block.
    unsafe {
        for (index, filter) in filters.iter().enumerate() {
            let Some(filter) = filter else { continue };
            if out[index].is_some() {
                continue;
            }
            let rect = filter.contentRect();
            let (w, h) = (rect.size.width, rect.size.height);
            if w < 1.0 || h < 1.0 {
                continue;
            }
            let width = f64::from(max_width).min(w * f64::from(filter.pointPixelScale()));
            let height = width * h / w;
            let config = SCStreamConfiguration::new();
            config.setWidth(width.round().max(1.0) as usize);
            config.setHeight(height.round().max(1.0) as usize);
            config.setShowsCursor(false);
            let tx = tx.clone();
            let handler = RcBlock::new(move |image: *mut CGImage, _error: *mut NSError| {
                let bytes =
                    if image.is_null() { None } else { encode(image as *const c_void, "public.jpeg") };
                let _ = tx.send((index, bytes));
            });
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                filter,
                &config,
                Some(&handler),
            );
            pending += 1;
        }
    }
    drop(tx);
    let deadline = Instant::now() + Duration::from_secs(2);
    while pending > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((index, bytes)) => {
                out[index] = bytes.map(Thumbnail::Picture);
                pending -= 1;
            }
            Err(_) => break,
        }
    }
    Ok(out)
}

/// A full-size PNG of `target` in sRGB, without the pointer. A window comes alone, its rounded corners clear, and
/// whatever covers it doesn't show; one Stage Manager parked comes back to the stage first, and a full-screen app's
/// on its own Space brings the Mac there, and is pictured with its toolbars. Waits at most five seconds for the
/// picture.
pub fn screenshot(target: Target) -> Result<Screenshot, CaptureError> {
    let target = match target {
        Target::Window(id) => {
            window::bring_forward(id);
            Target::Window(window::stand_in(id))
        }
        display => display,
    };
    match picture(target) {
        // Just after a window comes from another Space, the first picture can fail while the switch settles.
        Err(CaptureError::System(_)) if matches!(target, Target::Window(_)) => {
            std::thread::sleep(Duration::from_millis(400));
            picture(target)
        }
        taken => taken,
    }
}

/// One go at [`screenshot`]'s picture, of a window that's ready.
fn picture(target: Target) -> Result<Screenshot, CaptureError> {
    let content = shareable_content(true)?;
    let (tx, rx) = channel::<Result<Vec<u8>, String>>();
    // SAFETY: read-only use of `content`; the filter, configuration and handler live until the request is made,
    // and the framework keeps what it needs until it answers.
    let (scale, app_name, window_title) = unsafe {
        let (filter, app_name, window_title, area) = match target {
            Target::Display(id) => {
                let display = content
                    .displays()
                    .iter()
                    .find(|d| id.is_none_or(|id| d.displayID() == id))
                    .ok_or_else(|| CaptureError::SourceNotFound("that screen".into()))?;
                let filter = SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &NSArray::new(),
                );
                (filter, None, None, None)
            }
            Target::Window(id) => {
                let window =
                    content.windows().iter().find(|w| w.windowID() == id).ok_or_else(|| not_showing(id))?;
                // Still in Stage Manager's strip, it would come out small and tilted.
                let pid = window.owningApplication().map_or(0, |a| a.processID());
                if window::parked(id, window.frame(), pid, &display_frames(&content)) {
                    return Err(CaptureError::Parked);
                }
                let app = window.owningApplication().map(|a| a.applicationName().to_string());
                let title = window.title().map(|t| t.to_string()).filter(|t| !t.is_empty());
                // A full-screen app's toolbars are windows of their own: it's pictured from its display with them.
                match crop_for(&content, &window).filter(|crop| crop.top > 0.0) {
                    Some(crop) => (crop.filter(), app, title, Some((crop.rect, crop.source()))),
                    None => {
                        let filter = SCContentFilter::initWithDesktopIndependentWindow(
                            SCContentFilter::alloc(),
                            &window,
                        );
                        (filter, app, title, None)
                    }
                }
            }
        };
        let rect = area.map_or_else(|| filter.contentRect(), |(rect, _)| rect);
        let scale = f64::from(filter.pointPixelScale());
        let (width, height) = ((rect.size.width * scale).round(), (rect.size.height * scale).round());
        if width < 1.0 || height < 1.0 {
            return Err(CaptureError::System(
                "there's nothing to picture there. Is the window minimised?".into(),
            ));
        }
        let config = SCStreamConfiguration::new();
        config.setWidth(width as usize);
        config.setHeight(height as usize);
        config.setShowsCursor(false);
        if let Some((_, source)) = area {
            config.setSourceRect(source);
        }
        // The styled copy is drawn in sRGB, so the picture comes in it too.
        config.setColorSpaceName(kCGColorSpaceSRGB);
        let handler = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
            let png = if image.is_null() {
                Err(super::error_text(error))
            } else {
                encode(image as *const c_void, "public.png")
                    .ok_or_else(|| "it couldn't be saved as a PNG".into())
            };
            let _ = tx.send(png);
        });
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&handler),
        );
        (scale, app_name, window_title)
    };
    let png = match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(png)) => png,
        Ok(Err(reason)) => {
            // A window that has left this Space says so.
            if let Target::Window(id) = target
                && window::placed(id).is_some_and(|w| !w.on_screen)
            {
                return Err(CaptureError::Away);
            }
            return Err(CaptureError::System(format!("no picture came back: {reason}")));
        }
        Err(_) => {
            return Err(CaptureError::System(
                "no picture came back. The screen may be asleep or locked.".into(),
            ));
        }
    };
    // The size is read from the picture by the caller.
    Ok(Screenshot { png, width: 0, height: 0, scale, app_name, window_title })
}

fn devices(audio: bool) -> Vec<Device> {
    // SAFETY: AVFoundation's media type constants are immutable statics; listing devices needs no permission.
    unsafe {
        let Some(kind) = (if audio { AVMediaTypeAudio } else { AVMediaTypeVideo }) else { return Vec::new() };
        let default = AVCaptureDevice::defaultDeviceWithMediaType(kind).map(|d| d.uniqueID().to_string());
        #[allow(deprecated)]
        let found = AVCaptureDevice::devicesWithMediaType(kind);
        found
            .iter()
            .map(|d| {
                let id = d.uniqueID().to_string();
                Device {
                    is_default: default.as_deref() == Some(id.as_str()),
                    id,
                    name: d.localizedName().to_string(),
                }
            })
            .collect()
    }
}

pub fn microphones() -> Vec<Device> {
    devices(true)
}

pub fn cameras() -> Vec<Device> {
    devices(false)
}
