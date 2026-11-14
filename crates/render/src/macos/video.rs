//! Hardware encoding of the finished frames, and the mixed sound, to an MP4 with AVAssetWriter.

use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo, AVVideoAverageBitRateKey, AVVideoCodecKey,
    AVVideoCodecTypeH264, AVVideoCodecTypeHEVC, AVVideoColorPrimaries_ITU_R_709_2, AVVideoColorPrimariesKey,
    AVVideoColorPropertiesKey, AVVideoCompressionPropertiesKey, AVVideoExpectedSourceFrameRateKey,
    AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey, AVVideoProfileLevelH264HighAutoLevel,
    AVVideoProfileLevelKey, AVVideoTransferFunction_ITU_R_709_2, AVVideoTransferFunctionKey, AVVideoWidthKey,
    AVVideoYCbCrMatrix_ITU_R_709_2, AVVideoYCbCrMatrixKey,
};
use objc2_core_foundation::{CFRetained, CFString, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColorSpace;
use objc2_core_image::{CIContext, CIImage};
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVAttachmentMode, CVPixelBuffer, CVPixelBufferPool, kCVImageBufferColorPrimaries_ITU_R_709_2,
    kCVImageBufferColorPrimariesKey, kCVImageBufferTransferFunction_ITU_R_709_2,
    kCVImageBufferTransferFunctionKey, kCVImageBufferYCbCrMatrix_ITU_R_709_2, kCVImageBufferYCbCrMatrixKey,
    kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelBufferWidthKey,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL, ns_string};

use super::decode::{YUV_420V, ns, system};
use super::sound::{RATE, Sound};
use crate::{Format, RenderError};

fn constant(value: Option<&'static NSString>, name: &str) -> Result<&'static NSString, RenderError> {
    value.ok_or_else(|| system(format!("AVFoundation is missing {name}")))
}

pub struct VideoWriter {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    /// The sound's input, until all the sound has gone in.
    audio: Option<Retained<AVAssetWriterInput>>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    width: usize,
    height: usize,
    fps: i32,
    frames: i64,
}

impl VideoWriter {
    pub fn new(
        path: &Path,
        width: usize,
        height: usize,
        fps: u32,
        format: Format,
        bitrate: f64,
        with_sound: bool,
    ) -> Result<Self, RenderError> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| system(format!("not a valid file path: {}", path.display())))?;
        // SAFETY: the AVFoundation constants are immutable framework statics.
        let settings = unsafe {
            let codec = match format {
                Format::Hevc => constant(AVVideoCodecTypeHEVC, "HEVC")?,
                _ => constant(AVVideoCodecTypeH264, "H.264")?,
            };
            let bitrate = NSNumber::new_f64(bitrate.clamp(500_000.0, 200_000_000.0));
            let rate = NSNumber::new_u32(fps);
            // A keyframe every two seconds keeps scrubbing quick in players and editors.
            let keyframes = NSNumber::new_u32(fps * 2);
            let mut keys = vec![
                constant(AVVideoAverageBitRateKey, "AVVideoAverageBitRateKey")?,
                constant(AVVideoExpectedSourceFrameRateKey, "AVVideoExpectedSourceFrameRateKey")?,
                constant(AVVideoMaxKeyFrameIntervalKey, "AVVideoMaxKeyFrameIntervalKey")?,
            ];
            let mut values: Vec<&AnyObject> = vec![bitrate.as_ref(), rate.as_ref(), keyframes.as_ref()];
            if format == Format::H264 {
                keys.push(constant(AVVideoProfileLevelKey, "AVVideoProfileLevelKey")?);
                values
                    .push(constant(AVVideoProfileLevelH264HighAutoLevel, "the H.264 High profile")?.as_ref());
            }
            let compression = NSDictionary::<NSString, AnyObject>::from_slices(&keys, &values);
            // Tagged as HD video (BT.709) so players show the colours the editor showed.
            let color = NSDictionary::<NSString, AnyObject>::from_slices(
                &[
                    constant(AVVideoColorPrimariesKey, "AVVideoColorPrimariesKey")?,
                    constant(AVVideoTransferFunctionKey, "AVVideoTransferFunctionKey")?,
                    constant(AVVideoYCbCrMatrixKey, "AVVideoYCbCrMatrixKey")?,
                ],
                &[
                    constant(AVVideoColorPrimaries_ITU_R_709_2, "BT.709 primaries")?.as_ref(),
                    constant(AVVideoTransferFunction_ITU_R_709_2, "the BT.709 transfer function")?.as_ref(),
                    constant(AVVideoYCbCrMatrix_ITU_R_709_2, "the BT.709 matrix")?.as_ref(),
                ],
            );
            let (w, h) = (NSNumber::new_usize(width), NSNumber::new_usize(height));
            NSDictionary::<NSString, AnyObject>::from_slices(
                &[
                    constant(AVVideoCodecKey, "AVVideoCodecKey")?,
                    constant(AVVideoWidthKey, "AVVideoWidthKey")?,
                    constant(AVVideoHeightKey, "AVVideoHeightKey")?,
                    constant(AVVideoCompressionPropertiesKey, "AVVideoCompressionPropertiesKey")?,
                    constant(AVVideoColorPropertiesKey, "AVVideoColorPropertiesKey")?,
                ],
                &[codec.as_ref(), w.as_ref(), h.as_ref(), compression.as_ref(), color.as_ref()],
            )
        };
        let pixel_format = NSNumber::new_u32(YUV_420V);
        let (w, h) = (NSNumber::new_usize(width), NSNumber::new_usize(height));
        let surface = NSDictionary::<NSString, AnyObject>::new();
        // SAFETY: the CoreVideo keys are immutable framework statics.
        let attributes = unsafe {
            NSDictionary::<NSString, AnyObject>::from_slices(
                &[
                    ns(kCVPixelBufferPixelFormatTypeKey),
                    ns(kCVPixelBufferWidthKey),
                    ns(kCVPixelBufferHeightKey),
                    ns(kCVPixelBufferIOSurfacePropertiesKey),
                ],
                &[pixel_format.as_ref(), w.as_ref(), h.as_ref(), surface.as_ref()],
            )
        };
        // SAFETY: plain AVFoundation setup with valid arguments, before any frame is written.
        unsafe {
            let file_type =
                AVFileTypeMPEG4.ok_or_else(|| system("AVFoundation is missing AVFileTypeMPEG4"))?;
            let media = AVMediaTypeVideo.ok_or_else(|| system("AVFoundation is missing AVMediaTypeVideo"))?;
            let writer = AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                .map_err(|e| system(e.localizedDescription().to_string()))?;
            // The index goes at the front of the file, so the video starts playing before it fully downloads.
            writer.setShouldOptimizeForNetworkUse(true);
            let input =
                AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(media, Some(&settings));
            input.setExpectsMediaDataInRealTime(false);
            let adaptor =
                AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                    &input,
                    Some(&attributes),
                );
            if !writer.canAddInput(&input) {
                return Err(system("the encoder rejected these video settings"));
            }
            writer.addInput(&input);
            let audio = if with_sound {
                let media =
                    AVMediaTypeAudio.ok_or_else(|| system("AVFoundation is missing AVMediaTypeAudio"))?;
                // AAC stereo at 192 kbit/s: clean for voices and music alike.
                let aac = NSNumber::new_u32(u32::from_be_bytes(*b"aac "));
                let rate = NSNumber::new_f64(f64::from(RATE));
                let two = NSNumber::new_u32(2);
                let bitrate = NSNumber::new_u32(192_000);
                let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[
                        ns_string!("AVFormatIDKey"),
                        ns_string!("AVSampleRateKey"),
                        ns_string!("AVNumberOfChannelsKey"),
                        ns_string!("AVEncoderBitRateKey"),
                    ],
                    &[aac.as_ref(), rate.as_ref(), two.as_ref(), bitrate.as_ref()],
                );
                let audio =
                    AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(media, Some(&settings));
                audio.setExpectsMediaDataInRealTime(false);
                if !writer.canAddInput(&audio) {
                    return Err(system("the encoder rejected the sound settings"));
                }
                writer.addInput(&audio);
                Some(audio)
            } else {
                None
            };
            if !writer.startWriting() {
                let reason = writer.error().map(|e| e.localizedDescription().to_string()).unwrap_or_default();
                return Err(system(format!("couldn't start the encoder: {reason}")));
            }
            writer.startSessionAtSourceTime(CMTime::new(0, fps as i32));
            Ok(Self { writer, input, audio, adaptor, width, height, fps: fps as i32, frames: 0 })
        }
    }

    fn reason(&self) -> String {
        // SAFETY: reading the writer's error is allowed at any time.
        unsafe { self.writer.error() }.map(|e| e.localizedDescription().to_string()).unwrap_or_default()
    }

    /// Hands the encoder sound that starts before `until` (seconds of the export), as much as it takes right now.
    /// Once the sound runs out, its input is closed so the encoder stops waiting for more.
    fn feed(&mut self, sound: &mut Sound, until: f64) -> Result<(), RenderError> {
        let Some(input) = self.audio.clone() else { return Ok(()) };
        // SAFETY: polling readiness and appending buffers in time order, from the only thread using the writer.
        unsafe {
            while input.isReadyForMoreMediaData() {
                match sound.peek()? {
                    Some(start) if start < until => {
                        let Some(sample) = sound.take() else { break };
                        if !input.appendSampleBuffer(&sample) {
                            return Err(system(format!("the encoder refused the sound: {}", self.reason())));
                        }
                    }
                    Some(_) => break,
                    None => {
                        input.markAsFinished();
                        self.audio = None;
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Draws `image` into the next frame and hands it to the encoder, along with the sound up to about then.
    pub fn write(
        &mut self,
        context: &CIContext,
        image: &CIImage,
        space: &CGColorSpace,
        mut sound: Option<&mut Sound>,
    ) -> Result<(), RenderError> {
        // The sound stays a little ahead of the picture, the way the encoder interleaves them.
        let now = self.frames as f64 / f64::from(self.fps);
        if let Some(sound) = sound.as_deref_mut() {
            self.feed(sound, now + 1.0)?;
        }
        // SAFETY: the pool belongs to the adaptor and lives while it does; the buffer it returns is owned here.
        let buffer = unsafe {
            let pool =
                self.adaptor.pixelBufferPool().ok_or_else(|| system("the encoder has no frame pool"))?;
            let mut raw: *mut CVPixelBuffer = std::ptr::null_mut();
            let status = CVPixelBufferPool::create_pixel_buffer(None, &pool, NonNull::from(&mut raw));
            let raw =
                NonNull::new(raw).filter(|_| status == 0).ok_or_else(|| system("out of video memory"))?;
            CFRetained::from_raw(raw)
        };
        let tag = |key: &CFString, value: &CFString| {
            // SAFETY: setting a standard attachment on a buffer this code owns.
            unsafe { buffer.set_attachment(key, value, CVAttachmentMode::ShouldPropagate) };
        };
        // SAFETY: the CoreVideo constants are immutable framework statics.
        unsafe {
            tag(kCVImageBufferYCbCrMatrixKey, kCVImageBufferYCbCrMatrix_ITU_R_709_2);
            tag(kCVImageBufferColorPrimariesKey, kCVImageBufferColorPrimaries_ITU_R_709_2);
            tag(kCVImageBufferTransferFunctionKey, kCVImageBufferTransferFunction_ITU_R_709_2);
        }
        let bounds = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(self.width as f64, self.height as f64));
        // SAFETY: rendering into a buffer of the right size that nothing else uses yet.
        unsafe { context.render_toCVPixelBuffer_bounds_colorSpace(image, &buffer, bounds, Some(space)) };

        let deadline = Instant::now() + Duration::from_secs(20);
        // SAFETY: polling the input's readiness and the writer's status is allowed at any time.
        while !unsafe { self.input.isReadyForMoreMediaData() } {
            // The encoder may be waiting for sound before it takes more picture.
            if let Some(sound) = sound.as_deref_mut() {
                self.feed(sound, f64::INFINITY)?;
            }
            if unsafe { self.writer.status() } == AVAssetWriterStatus::Failed {
                return Err(system(format!("the encoder failed: {}", self.reason())));
            }
            if Instant::now() > deadline {
                return Err(system("the encoder stopped responding"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // SAFETY: the buffer is complete and its time is after every earlier frame.
        if !unsafe {
            self.adaptor.appendPixelBuffer_withPresentationTime(&buffer, CMTime::new(self.frames, self.fps))
        } {
            return Err(system(format!("the encoder refused a frame: {}", self.reason())));
        }
        self.frames += 1;
        Ok(())
    }

    pub fn finish(mut self, sound: Option<&mut Sound>) -> Result<(), RenderError> {
        // SAFETY: the picture is complete; closing its input lets the rest of the sound go in freely.
        unsafe { self.input.markAsFinished() };
        if let Some(sound) = sound {
            let deadline = Instant::now() + Duration::from_secs(60);
            while self.audio.is_some() {
                self.feed(sound, f64::INFINITY)?;
                // SAFETY: reading the writer's status is allowed at any time.
                if unsafe { self.writer.status() } == AVAssetWriterStatus::Failed {
                    return Err(system(format!("the encoder failed: {}", self.reason())));
                }
                if Instant::now() > deadline {
                    return Err(system("the encoder stopped taking the sound"));
                }
                if self.audio.is_some() {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        if let Some(audio) = self.audio.take() {
            // SAFETY: no more sound will come.
            unsafe { audio.markAsFinished() };
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
        // SAFETY: called once, after the last frame, from the only thread using the writer.
        unsafe {
            self.writer.endSessionAtSourceTime(CMTime::new(self.frames, self.fps));
            let block = block2::RcBlock::new(move || {
                let _ = tx.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&block);
        }
        rx.recv_timeout(Duration::from_secs(120)).map_err(|_| system("the encoder didn't finish in time"))?;
        // SAFETY: reading the status after completion.
        if unsafe { self.writer.status() } == AVAssetWriterStatus::Completed {
            Ok(())
        } else {
            Err(system(format!("the video couldn't be finished: {}", self.reason())))
        }
    }

    pub fn cancel(self) {
        // SAFETY: cancelling stops the writer; the partial file is removed by the caller.
        unsafe { self.writer.cancelWriting() };
    }
}
