//! AAC audio files (what the computer plays, the microphone) written as sample buffers arrive.

use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeAppleM4A, AVMediaTypeAudio,
};
use objc2_core_media::CMSampleBuffer;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL, ns_string};

use crate::CaptureError;

/// `kAudioFormatMPEG4AAC`.
const AAC: u32 = u32::from_be_bytes(*b"aac ");

pub struct AudioWriter {
    path: PathBuf,
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    /// Host time of the first sample, in seconds.
    pub first_pts: Option<f64>,
    /// Host time where the last sample ends.
    pub end_pts: f64,
    pub dropped: u64,
}

// SAFETY: AVAssetWriter and its inputs may be used from any thread as long as calls are not concurrent; the writer
// is kept behind a mutex.
unsafe impl Send for AudioWriter {}

impl AudioWriter {
    /// A new file at `path` with `channels` channels: 2 for the computer's sound, 1 for a voice.
    pub fn new(path: &Path, channels: u32) -> Result<Self, CaptureError> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| CaptureError::System(format!("not a valid file path: {}", path.display())))?;
        let format = NSNumber::new_u32(AAC);
        let rate = NSNumber::new_f64(48_000.0);
        let count = NSNumber::new_u32(channels);
        // Clear speech needs about 96 kbit/s per channel in AAC; music and system sounds get the same.
        let bitrate = NSNumber::new_u32(96_000 * channels);
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(
            &[
                ns_string!("AVFormatIDKey"),
                ns_string!("AVSampleRateKey"),
                ns_string!("AVNumberOfChannelsKey"),
                ns_string!("AVEncoderBitRateKey"),
            ],
            &[format.as_ref(), rate.as_ref(), count.as_ref(), bitrate.as_ref()],
        );
        // SAFETY: plain AVFoundation setup with valid arguments, before any sample arrives.
        unsafe {
            let file_type = AVFileTypeAppleM4A
                .ok_or_else(|| CaptureError::System("AVFoundation is missing M4A".into()))?;
            let media = AVMediaTypeAudio
                .ok_or_else(|| CaptureError::System("AVFoundation is missing AVMediaTypeAudio".into()))?;
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| CaptureError::System(e.localizedDescription().to_string()))?;
            let input =
                AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(media, Some(&settings));
            input.setExpectsMediaDataInRealTime(true);
            if !writer.canAddInput(&input) {
                return Err(CaptureError::System("the audio encoder rejected its settings".into()));
            }
            writer.addInput(&input);
            if !writer.startWriting() {
                let reason = writer.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default();
                return Err(CaptureError::System(format!("couldn't start writing audio: {reason}")));
            }
            Ok(Self { path: path.to_path_buf(), writer, input, first_pts: None, end_pts: 0.0, dropped: 0 })
        }
    }

    /// Appends one buffer of audio. Buffers the encoder isn't ready for are counted and skipped.
    pub fn append(&mut self, sample: &CMSampleBuffer) {
        // SAFETY: the sample buffer comes from the capture system and is valid for this call.
        unsafe {
            if !sample.is_valid() || sample.num_samples() <= 0 {
                return;
            }
            let pts = sample.presentation_time_stamp().seconds();
            if !pts.is_finite() || pts < self.end_pts - 1e-3 {
                return;
            }
            if self.first_pts.is_none() {
                self.writer.startSessionAtSourceTime(sample.presentation_time_stamp());
                self.first_pts = Some(pts);
            }
            if !self.input.isReadyForMoreMediaData() || !self.input.appendSampleBuffer(sample) {
                self.dropped += 1;
                return;
            }
            let duration = sample.duration().seconds();
            self.end_pts = pts + if duration.is_finite() { duration } else { 0.0 };
        }
    }

    /// Finishes the file. Returns its first and end host times, or `None` (and no file) if nothing was heard.
    pub fn finish(self) -> Result<Option<(f64, f64)>, CaptureError> {
        let Some(first) = self.first_pts else {
            // SAFETY: cancelling a writer that never received a sample.
            unsafe { self.writer.cancelWriting() };
            let _ = std::fs::remove_file(&self.path);
            return Ok(None);
        };
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
        // SAFETY: called once, after capture stopped, with no other thread touching the writer.
        unsafe {
            self.input.markAsFinished();
            let block = block2::RcBlock::new(move || {
                let _ = tx.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&block);
        }
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|_| CaptureError::System("the audio encoder didn't finish in time".into()))?;
        // SAFETY: reading the status after completion.
        if unsafe { self.writer.status() } == AVAssetWriterStatus::Completed {
            Ok(Some((first, self.end_pts.max(first))))
        } else {
            let reason = unsafe { self.writer.error() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_default();
            Err(CaptureError::System(format!("the audio couldn't be finished: {reason}")))
        }
    }
}
