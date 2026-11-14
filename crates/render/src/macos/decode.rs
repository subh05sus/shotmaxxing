//! Reads the recording's frames in order with AVAssetReader, decoded in hardware.
//!
//! An export only ever moves forward through the recording (clips are in order and never overlap), so one pass
//! over the file serves the whole export. Screen recordings only store a frame when something changed, so the frame
//! on screen at a moment is the last one stored at or before it.

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetReader, AVAssetReaderStatus, AVAssetReaderTrackOutput, AVMediaTypeVideo, AVURLAsset,
};
use objc2_core_foundation::{CFRetained, CFString};
use objc2_core_video::{CVPixelBuffer, kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferPixelFormatTypeKey};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::RenderError;

/// '420v': what the hardware decoder produces natively, and what Core Image reads without a conversion pass.
pub const YUV_420V: u32 = 0x3432_3076;

/// A Core Foundation string constant as the toll-free bridged NSString.
pub fn ns(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged; the pointer is to the same object.
    unsafe { &*std::ptr::from_ref(key).cast::<NSString>() }
}

pub fn system(message: impl Into<String>) -> RenderError {
    RenderError::System(message.into())
}

type Decoded = (f64, CFRetained<CVPixelBuffer>);

pub struct Decoder {
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    /// The first frame's timestamp: the recording's time zero.
    base: Option<f64>,
    current: Option<Decoded>,
    next: Option<Decoded>,
    done: bool,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self, RenderError> {
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| system(format!("not a valid file path: {}", path.display())))?;
        // SAFETY: plain AVFoundation calls with valid arguments; the objects live as long as the decoder.
        unsafe {
            let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
            let video = AVMediaTypeVideo.ok_or_else(|| system("AVFoundation is missing AVMediaTypeVideo"))?;
            #[allow(deprecated)]
            let tracks = asset.tracksWithMediaType(video);
            let track = tracks.firstObject().ok_or_else(|| system("the recording has no video in it"))?;
            let reader = AVAssetReader::assetReaderWithAsset_error(&asset)
                .map_err(|e| system(format!("couldn't read the recording: {}", e.localizedDescription())))?;
            let format = NSNumber::new_u32(YUV_420V);
            let surface = NSDictionary::<NSString, AnyObject>::new();
            let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                &[ns(kCVPixelBufferPixelFormatTypeKey), ns(kCVPixelBufferIOSurfacePropertiesKey)],
                &[format.as_ref(), surface.as_ref()],
            );
            let output =
                AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
            output.setAlwaysCopiesSampleData(false);
            if !reader.canAddOutput(&output) {
                return Err(system("the recording's video can't be decoded"));
            }
            reader.addOutput(&output);
            if !reader.startReading() {
                let reason = reader.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default();
                return Err(system(format!("couldn't start reading the recording: {reason}")));
            }
            Ok(Self { reader, output, base: None, current: None, next: None, done: false })
        }
    }

    fn read(&mut self) -> Result<Option<Decoded>, RenderError> {
        if self.done {
            return Ok(None);
        }
        loop {
            // SAFETY: the reader is reading and the output belongs to it.
            let Some(sample) = (unsafe { self.output.copyNextSampleBuffer() }) else {
                self.done = true;
                // SAFETY: reading the reader's state after it stopped handing out samples.
                if unsafe { self.reader.status() } == AVAssetReaderStatus::Failed {
                    let reason = unsafe { self.reader.error() }
                        .map(|e| e.localizedDescription().to_string())
                        .unwrap_or_default();
                    return Err(system(format!("the recording couldn't be decoded: {reason}")));
                }
                return Ok(None);
            };
            // SAFETY: the sample buffer is valid; marker samples without a picture are skipped.
            let Some(buffer) = (unsafe { sample.image_buffer() }) else { continue };
            let pts = unsafe { sample.presentation_time_stamp().seconds() };
            if !pts.is_finite() {
                continue;
            }
            let base = *self.base.get_or_insert(pts);
            return Ok(Some((pts - base, buffer)));
        }
    }

    /// The picture on screen `t` seconds into the recording. Calls must not go back in time.
    pub fn frame_at(&mut self, t: f64) -> Result<Option<CFRetained<CVPixelBuffer>>, RenderError> {
        loop {
            if self.next.is_none() {
                self.next = self.read()?;
            }
            match &self.next {
                Some((pts, _)) if *pts <= t + 1e-4 => self.current = self.next.take(),
                _ => break,
            }
        }
        // Before the first stored frame, show the first frame.
        Ok(self.current.as_ref().or(self.next.as_ref()).map(|(_, buffer)| buffer.clone()))
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: cancelling is allowed in any state and stops the decoder's work.
        unsafe { self.reader.cancelReading() };
    }
}
