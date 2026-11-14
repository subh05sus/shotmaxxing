//! Hardware video encoding with AVAssetWriter: HEVC in a fragmented MP4, written as frames arrive.
//!
//! Fragmented means the file is playable up to the last fragment even if the app or the machine dies mid-take.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeVideo,
    AVVideoAllowFrameReorderingKey, AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeHEVC,
    AVVideoCompressionPropertiesKey, AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoWidthKey,
};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::CaptureError;

pub struct Writer {
    writer: Retained<AVAssetWriter>,
    video: Retained<AVAssetWriterInput>,
    started: bool,
    pub first_pts: Option<f64>,
    pub last_pts: f64,
    pub frames: u64,
    pub dropped: u64,
}

// SAFETY: AVAssetWriter and its inputs may be used from any thread as long as calls are not concurrent. The
// recorder keeps the writer behind a mutex, so every call is serialised.
unsafe impl Send for Writer {}

fn key(constant: Option<&'static NSString>, name: &str) -> Result<&'static NSString, CaptureError> {
    constant.ok_or_else(|| CaptureError::System(format!("AVFoundation is missing {name}")))
}

impl Writer {
    pub fn new(
        path: &std::path::Path,
        width: usize,
        height: usize,
        max_fps: u32,
    ) -> Result<Self, CaptureError> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| CaptureError::System(format!("not a valid file path: {}", path.display())))?;

        // Screen content compresses very well; about 0.06 bits per pixel per frame looks clean, capped at 40 Mbit/s.
        let bitrate = ((width * height) as f64 * f64::from(max_fps) * 0.06).clamp(4e6, 40e6);

        // SAFETY: the AVFoundation constants are immutable framework statics.
        let (
            file_type,
            media_video,
            codec_key,
            hevc,
            width_key,
            height_key,
            compression_key,
            bitrate_key,
            fps_key,
            reorder_key,
        ) = unsafe {
            (
                AVFileTypeMPEG4
                    .ok_or_else(|| CaptureError::System("AVFoundation is missing AVFileTypeMPEG4".into()))?,
                AVMediaTypeVideo
                    .ok_or_else(|| CaptureError::System("AVFoundation is missing AVMediaTypeVideo".into()))?,
                key(AVVideoCodecKey, "AVVideoCodecKey")?,
                AVVideoCodecTypeHEVC
                    .ok_or_else(|| CaptureError::System("AVFoundation is missing HEVC".into()))?,
                key(AVVideoWidthKey, "AVVideoWidthKey")?,
                key(AVVideoHeightKey, "AVVideoHeightKey")?,
                key(AVVideoCompressionPropertiesKey, "AVVideoCompressionPropertiesKey")?,
                key(AVVideoAverageBitRateKey, "AVVideoAverageBitRateKey")?,
                key(AVVideoExpectedSourceFrameRateKey, "AVVideoExpectedSourceFrameRateKey")?,
                key(AVVideoAllowFrameReorderingKey, "AVVideoAllowFrameReorderingKey")?,
            )
        };

        let bitrate_value = NSNumber::new_f64(bitrate);
        let fps_value = NSNumber::new_u32(max_fps);
        let no_reorder = NSNumber::new_bool(false);
        let compression = NSDictionary::<NSString, AnyObject>::from_slices(
            &[bitrate_key, fps_key, reorder_key],
            &[bitrate_value.as_ref(), fps_value.as_ref(), no_reorder.as_ref()],
        );
        let width_value = NSNumber::new_usize(width);
        let height_value = NSNumber::new_usize(height);
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(
            &[codec_key, width_key, height_key, compression_key],
            &[hevc.as_ref(), width_value.as_ref(), height_value.as_ref(), compression.as_ref()],
        );

        // SAFETY: plain AVFoundation calls with valid arguments, made before any frame arrives.
        unsafe {
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| CaptureError::System(e.localizedDescription().to_string()))?;
            writer.setMovieFragmentInterval(CMTime::with_seconds(2.0, 600));
            let video = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                media_video,
                Some(&settings),
            );
            video.setExpectsMediaDataInRealTime(true);
            if !writer.canAddInput(&video) {
                return Err(CaptureError::System("the encoder rejected the video settings".into()));
            }
            writer.addInput(&video);
            if !writer.startWriting() {
                let reason = writer.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default();
                return Err(CaptureError::System(format!("couldn't start writing: {reason}")));
            }
            Ok(Self { writer, video, started: false, first_pts: None, last_pts: 0.0, frames: 0, dropped: 0 })
        }
    }

    /// Appends one captured frame. Frames the encoder isn't ready for are counted and skipped rather than queued,
    /// so a busy moment never builds up memory.
    pub fn append(&mut self, sample: &CMSampleBuffer) {
        // SAFETY: the sample buffer comes straight from ScreenCaptureKit and is valid for this call.
        unsafe {
            if !sample.is_valid() || sample.image_buffer().is_none() {
                return;
            }
            let pts = sample.presentation_time_stamp();
            let seconds = pts.seconds();
            if !self.started {
                self.writer.startSessionAtSourceTime(pts);
                self.started = true;
                self.first_pts = Some(seconds);
            }
            if seconds < self.last_pts {
                return;
            }
            if !self.video.isReadyForMoreMediaData() {
                self.dropped += 1;
                return;
            }
            if self.video.appendSampleBuffer(sample) {
                self.frames += 1;
                self.last_pts = seconds;
            } else {
                self.dropped += 1;
            }
        }
    }

    /// Ends the file at `end` (host seconds), so a still screen at the end still counts toward the length.
    pub fn finish(self, end: f64) -> Result<(), CaptureError> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
        // SAFETY: called once, after capture stopped, with no other thread touching the writer.
        unsafe {
            if self.started {
                let end = end.max(self.last_pts);
                self.writer.endSessionAtSourceTime(CMTime::with_seconds(end, 1_000_000_000));
            }
            self.video.markAsFinished();
            let block = block2::RcBlock::new(move || {
                let _ = tx.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&block);
        }
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| CaptureError::System("the encoder didn't finish in time".into()))?;
        // SAFETY: reading status after completion.
        let status = unsafe { self.writer.status() };
        if status == AVAssetWriterStatus::Completed || !self.started {
            Ok(())
        } else {
            // SAFETY: reading the error after completion.
            let reason = unsafe { self.writer.error() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_default();
            Err(CaptureError::System(format!("the video couldn't be finished: {reason}")))
        }
    }
}
