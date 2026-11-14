//! Small pictures of screens and windows for the source picker, and the microphones and cameras on this Mac.

use std::ffi::c_void;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo};
use objc2_core_foundation::{CFRetained, CFString};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{SCContentFilter, SCScreenshotManager, SCStreamConfiguration};

use super::shareable_content;
use crate::{CaptureError, Device, Target};

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

/// Encodes a picture as JPEG.
fn jpeg(image: *const c_void) -> Option<Vec<u8>> {
    let kind = CFString::from_static_str("public.jpeg");
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

/// A JPEG of each target, at most `max_width` pixels wide, in the same order. Targets that can't be pictured (gone,
/// or not on screen) get `None`. All pictures are taken at once and the whole call waits at most about two seconds.
pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<Vec<u8>>>, CaptureError> {
    let content = shareable_content()?;
    let (tx, rx) = channel::<(usize, Option<Vec<u8>>)>();
    let mut pending = 0;
    // SAFETY: read-only use of `content`; each request gets its own filter, configuration and handler block.
    unsafe {
        for (index, target) in targets.iter().enumerate() {
            let filter: Option<Retained<SCContentFilter>> =
                match *target {
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
                };
            let Some(filter) = filter else { continue };
            let rect = filter.contentRect();
            if rect.size.width < 1.0 || rect.size.height < 1.0 {
                continue;
            }
            let width = f64::from(max_width).min(rect.size.width * f64::from(filter.pointPixelScale()));
            let height = width * rect.size.height / rect.size.width;
            let config = SCStreamConfiguration::new();
            config.setWidth(width.round().max(1.0) as usize);
            config.setHeight(height.round().max(1.0) as usize);
            config.setShowsCursor(false);
            let tx = tx.clone();
            let handler = RcBlock::new(move |image: *mut CGImage, _error: *mut NSError| {
                let bytes = if image.is_null() { None } else { jpeg(image as *const c_void) };
                let _ = tx.send((index, bytes));
            });
            SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                &filter,
                &config,
                Some(&handler),
            );
            pending += 1;
        }
    }
    drop(tx);
    let mut out = vec![None; targets.len()];
    let deadline = Instant::now() + Duration::from_secs(2);
    while pending > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((index, bytes)) => {
                out[index] = bytes;
                pending -= 1;
            }
            Err(_) => break,
        }
    }
    Ok(out)
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
