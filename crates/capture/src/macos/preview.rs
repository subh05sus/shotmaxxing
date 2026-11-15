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

use super::{display_frames, shareable_content, window};
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

/// A JPEG of each target, at most `max_width` pixels wide, in the same order, or for a window parked in Stage
/// Manager's strip its app's icon. Targets that can't be pictured (gone, or not on screen) get `None`. All pictures
/// are taken at once and the whole call waits at most about two seconds.
pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<Thumbnail>>, CaptureError> {
    let content = shareable_content()?;
    let displays = display_frames(&content);
    let (stage_manager, strip_on_right) = (window::stage_manager_on(), window::strip_on_right());
    // Each window's frame and owner, to tell a parked one.
    // SAFETY: read-only property access on objects owned by `content`.
    let windows: Vec<Option<(CGRect, i32)>> = unsafe {
        targets
            .iter()
            .map(|target| match *target {
                Target::Display(_) => None,
                Target::Window(id) => content.windows().iter().find(|w| w.windowID() == id).and_then(|w| {
                    let pid = w.owningApplication()?.processID();
                    Some((w.frame(), pid))
                }),
            })
            .collect()
    };
    // SAFETY: read-only use of `content`; each filter is made from one of its displays or windows.
    let filters: Vec<Option<Retained<SCContentFilter>>> =
        unsafe {
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
                    Target::Window(id) => content.windows().iter().find(|w| w.windowID() == id).map(|w| {
                        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &w)
                    }),
                })
                .collect()
        };
    // A window parked in Stage Manager's strip can only be pictured small and tilted, and a still one not at all: its
    // card shows its app's icon instead.
    let mut out: Vec<Option<Thumbnail>> = vec![None; targets.len()];
    let mut icons: HashMap<i32, Option<Vec<u8>>> = HashMap::new();
    if stage_manager {
        for (index, found) in windows.iter().enumerate() {
            let Some((frame, pid)) = *found else { continue };
            if window::in_strip(frame, &displays, strip_on_right) {
                let icon = icons.entry(pid).or_insert_with(|| app_icon(pid, 128.0)).clone();
                out[index] = Some(Thumbnail::Parked { icon });
            }
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
/// whatever covers it doesn't show; one Stage Manager parked comes back to the stage first. Waits at most five
/// seconds for the picture.
pub fn screenshot(target: Target) -> Result<Screenshot, CaptureError> {
    if let Target::Window(id) = target {
        window::bring_forward(id);
    }
    let content = shareable_content()?;
    let (tx, rx) = channel::<Option<Vec<u8>>>();
    // SAFETY: read-only use of `content`; the filter, configuration and handler live until the request is made,
    // and the framework keeps what it needs until it answers.
    let (scale, app_name, window_title) = unsafe {
        let (filter, app_name, window_title) = match target {
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
                (filter, None, None)
            }
            Target::Window(id) => {
                let window = content
                    .windows()
                    .iter()
                    .find(|w| w.windowID() == id)
                    .ok_or_else(|| CaptureError::SourceNotFound("that window".into()))?;
                // Still in Stage Manager's strip, it would come out small and tilted.
                if window::stage_manager_on()
                    && window::in_strip(window.frame(), &display_frames(&content), window::strip_on_right())
                {
                    return Err(CaptureError::Parked);
                }
                let app = window.owningApplication().map(|a| a.applicationName().to_string());
                let title = window.title().map(|t| t.to_string()).filter(|t| !t.is_empty());
                let filter =
                    SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window);
                (filter, app, title)
            }
        };
        let rect = filter.contentRect();
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
        // The styled copy is drawn in sRGB, so the picture comes in it too.
        config.setColorSpaceName(kCGColorSpaceSRGB);
        let handler = RcBlock::new(move |image: *mut CGImage, _error: *mut NSError| {
            let png = if image.is_null() { None } else { encode(image as *const c_void, "public.png") };
            let _ = tx.send(png);
        });
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config,
            Some(&handler),
        );
        (scale, app_name, window_title)
    };
    let png = rx.recv_timeout(Duration::from_secs(5)).ok().flatten().ok_or_else(|| {
        CaptureError::System("no picture came back. The screen may be asleep or locked.".into())
    })?;
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
