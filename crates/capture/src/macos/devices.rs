//! The microphone and the camera, recorded by one capture session alongside the screen. Their sample buffers carry
//! host-clock times like the screen's, so every track lines up afterwards.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureAudioDataOutput, AVCaptureAudioDataOutputSampleBufferDelegate,
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession,
    AVCaptureSessionPreset1280x720, AVCaptureSessionPresetHigh, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_core_media::{CMClock, CMSampleBuffer};
use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth, kCVPixelBufferPixelFormatTypeKey};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString, ns_string};

use super::audio::AudioWriter;
use super::writer::Writer;
use crate::{CaptureError, DeviceRecording};

/// Cameras are filmed at up to this rate.
const CAMERA_FPS: u32 = 30;

enum CameraWriter {
    /// Waiting for the first frame, which sets the size.
    Waiting(PathBuf),
    Writing(Writer),
    Failed,
}

struct Shared {
    mic: Mutex<Option<AudioWriter>>,
    camera: Mutex<Option<CameraWriter>>,
    /// The microphone has sent its first sound.
    heard: AtomicBool,
}

struct Ivars {
    shared: Arc<Shared>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "SMXDeviceOutput"]
    #[ivars = Ivars]
    struct DeviceOutput;

    unsafe impl NSObjectProtocol for DeviceOutput {}

    // Audio and video outputs call the same method; a buffer with a picture in it came from the camera.
    unsafe impl AVCaptureAudioDataOutputSampleBufferDelegate for DeviceOutput {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        fn did_output(
            &self,
            _output: &AVCaptureOutput,
            sample: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            let shared = &self.ivars().shared;
            // SAFETY: the sample buffer is valid for this call.
            match unsafe { sample.image_buffer() } {
                Some(picture) => {
                    let Ok(mut guard) = shared.camera.lock() else { return };
                    if let Some(CameraWriter::Waiting(path)) = guard.as_ref() {
                        let (width, height) =
                            (CVPixelBufferGetWidth(&picture) & !1, CVPixelBufferGetHeight(&picture) & !1);
                        *guard = Some(match Writer::new(path, width, height, CAMERA_FPS) {
                            Ok(writer) => CameraWriter::Writing(writer),
                            Err(error) => {
                                tracing::warn!(%error, "couldn't start writing the camera");
                                CameraWriter::Failed
                            }
                        });
                    }
                    if let Some(CameraWriter::Writing(writer)) = guard.as_mut() {
                        writer.append(sample);
                    }
                }
                None => {
                    if let Ok(mut guard) = shared.mic.lock()
                        && let Some(writer) = guard.as_mut()
                    {
                        writer.append(sample);
                        shared.heard.store(true, Ordering::Release);
                    }
                }
            }
        }
    }

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for DeviceOutput {}
);

impl DeviceOutput {
    fn new(shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { shared });
        // SAFETY: plain NSObject initialisation.
        unsafe { msg_send![super(this), init] }
    }
}

/// Whether this app may use the microphone or camera, asking first if nobody has decided yet.
fn allowed(media: &AVMediaType) -> bool {
    // SAFETY: plain AVFoundation class methods with a valid media type.
    unsafe {
        match AVCaptureDevice::authorizationStatusForMediaType(media) {
            AVAuthorizationStatus::Authorized => true,
            AVAuthorizationStatus::NotDetermined => {
                let (tx, rx) = std::sync::mpsc::sync_channel::<bool>(1);
                let block = block2::RcBlock::new(move |granted: Bool| {
                    let _ = tx.send(granted.as_bool());
                });
                AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &block);
                rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
            }
            _ => false,
        }
    }
}

fn device(choice: &DeviceRecording, media: &AVMediaType) -> Option<Retained<AVCaptureDevice>> {
    // SAFETY: plain AVFoundation lookups.
    unsafe {
        choice
            .device
            .as_deref()
            .and_then(|id| AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id)))
            .or_else(|| AVCaptureDevice::defaultDeviceWithMediaType(media))
    }
}

/// What the devices recorded: the first and end host times of each, and what went wrong along the way.
pub struct Stopped {
    pub microphone: Option<(f64, f64)>,
    pub camera: Option<(f64, f64)>,
    pub warnings: Vec<String>,
}

pub struct DeviceCapture {
    session: Retained<AVCaptureSession>,
    _delegate: Retained<DeviceOutput>,
    _queue: DispatchRetained<DispatchQueue>,
    shared: Arc<Shared>,
    /// Host time minus the session clock's time, to put device timestamps on the screen's timeline.
    offset: f64,
    warnings: Vec<String>,
}

impl Drop for DeviceCapture {
    fn drop(&mut self) {
        // SAFETY: stopping a session this object owns; harmless when it already stopped.
        unsafe { self.session.stopRunning() };
    }
}

// SAFETY: AVCaptureSession may be started and stopped from any thread; the recorder owns it and calls it from one
// thread at a time.
unsafe impl Send for DeviceCapture {}

impl DeviceCapture {
    /// Starts recording the chosen devices. Returns `None` when there's nothing to record. A device that can't be
    /// used becomes a warning rather than an error, so the screen recording still goes ahead.
    pub fn start(
        microphone: Option<&DeviceRecording>,
        camera: Option<&DeviceRecording>,
    ) -> Result<Option<Self>, CaptureError> {
        if microphone.is_none() && camera.is_none() {
            return Ok(None);
        }
        let mut warnings = Vec::new();
        let shared = Arc::new(Shared {
            mic: Mutex::new(None),
            camera: Mutex::new(None),
            heard: AtomicBool::new(false),
        });
        let delegate = DeviceOutput::new(shared.clone());
        let queue = DispatchQueue::new("dev.shotmaxxing.devices", None);
        let (audio_type, video_type) = (
            // SAFETY: immutable framework constants.
            unsafe { AVMediaTypeAudio }
                .ok_or_else(|| CaptureError::System("AVFoundation has no audio".into()))?,
            unsafe { AVMediaTypeVideo }
                .ok_or_else(|| CaptureError::System("AVFoundation has no video".into()))?,
        );

        // SAFETY: the session is configured before it runs; every object passed in is valid.
        unsafe {
            let session = AVCaptureSession::new();
            session.beginConfiguration();
            let mut used = false;

            if let Some(choice) = microphone {
                match add_input(&session, choice, audio_type, "microphone") {
                    Ok(()) => {
                        let output = AVCaptureAudioDataOutput::new();
                        // One channel of 32-bit float at 48 kHz, whatever the device sends: a known shape for the
                        // encoder, and mono is what a voice needs.
                        let lpcm = NSNumber::new_u32(u32::from_be_bytes(*b"lpcm"));
                        let rate = NSNumber::new_f64(48_000.0);
                        let one = NSNumber::new_u32(1);
                        let bits = NSNumber::new_u32(32);
                        let yes = NSNumber::new_bool(true);
                        let no = NSNumber::new_bool(false);
                        let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                            &[
                                ns_string!("AVFormatIDKey"),
                                ns_string!("AVSampleRateKey"),
                                ns_string!("AVNumberOfChannelsKey"),
                                ns_string!("AVLinearPCMBitDepthKey"),
                                ns_string!("AVLinearPCMIsFloatKey"),
                                ns_string!("AVLinearPCMIsNonInterleaved"),
                                ns_string!("AVLinearPCMIsBigEndianKey"),
                            ],
                            &[
                                lpcm.as_ref(),
                                rate.as_ref(),
                                one.as_ref(),
                                bits.as_ref(),
                                yes.as_ref(),
                                no.as_ref(),
                                no.as_ref(),
                            ],
                        );
                        output.setAudioSettings(Some(&settings));
                        if session.canAddOutput(&output) {
                            session.addOutput(&output);
                            output.setSampleBufferDelegate_queue(
                                Some(ProtocolObject::from_ref(&*delegate)),
                                Some(&queue),
                            );
                            *shared.mic.lock().map_err(|_| poisoned())? =
                                Some(AudioWriter::new(&choice.output, 1)?);
                            used = true;
                        } else {
                            warnings.push("The microphone couldn't be recorded alongside the screen.".into());
                        }
                    }
                    Err(warning) => warnings.push(warning),
                }
            }

            if let Some(choice) = camera {
                match add_input(&session, choice, video_type, "camera") {
                    Ok(()) => {
                        for preset in [AVCaptureSessionPreset1280x720, AVCaptureSessionPresetHigh] {
                            if session.canSetSessionPreset(preset) {
                                session.setSessionPreset(preset);
                                break;
                            }
                        }
                        let output = AVCaptureVideoDataOutput::new();
                        let format = NSNumber::new_u32(super::YUV_420V);
                        let settings = NSDictionary::<NSString, AnyObject>::from_slices(
                            &[super::ns(kCVPixelBufferPixelFormatTypeKey)],
                            &[format.as_ref()],
                        );
                        output.setVideoSettings(Some(&settings));
                        output.setAlwaysDiscardsLateVideoFrames(true);
                        if session.canAddOutput(&output) {
                            session.addOutput(&output);
                            output.setSampleBufferDelegate_queue(
                                Some(ProtocolObject::from_ref(&*delegate)),
                                Some(&queue),
                            );
                            *shared.camera.lock().map_err(|_| poisoned())? =
                                Some(CameraWriter::Waiting(choice.output.clone()));
                            used = true;
                        } else {
                            warnings.push("The camera couldn't be recorded alongside the screen.".into());
                        }
                    }
                    Err(warning) => warnings.push(warning),
                }
            }

            session.commitConfiguration();
            if used {
                // Starting takes a moment while the camera wakes up; it happens before the screen starts, so
                // nothing at the beginning is missed.
                session.startRunning();
            }
            let offset = clock_offset(&session);
            Ok(Some(Self { session, _delegate: delegate, _queue: queue, shared, offset, warnings }))
        }
    }

    /// Stops the devices and finishes their files. Returns the microphone's and the camera's first and end host
    /// times, when they recorded anything, and what went wrong along the way.
    pub fn stop(mut self, end: f64) -> Stopped {
        // SAFETY: stopping a session this object owns.
        unsafe { self.session.stopRunning() };
        let offset = self.offset;
        let host = |(first, last): (f64, f64)| (first + offset, last + offset);
        let mic = self.shared.mic.lock().ok().and_then(|mut guard| guard.take());
        let mic = match mic.map(AudioWriter::finish) {
            Some(Ok(times)) => times.map(host),
            Some(Err(error)) => {
                self.warnings.push(format!("The microphone recording couldn't be saved: {error}"));
                None
            }
            None => None,
        };
        let camera = self.shared.camera.lock().ok().and_then(|mut guard| guard.take());
        let camera = match camera {
            Some(CameraWriter::Writing(writer)) => {
                let first = writer.first_pts;
                // The file ends when the screen stopped, in the camera's own clock.
                let end = end - offset;
                match writer.finish(end) {
                    Ok(()) => first.map(|first| host((first, end.max(first)))),
                    Err(error) => {
                        self.warnings.push(format!("The camera recording couldn't be saved: {error}"));
                        None
                    }
                }
            }
            Some(CameraWriter::Waiting(path)) => {
                let _ = std::fs::remove_file(path);
                self.warnings.push("The camera didn't send any picture.".into());
                None
            }
            _ => None,
        };
        Stopped { microphone: mic, camera, warnings: std::mem::take(&mut self.warnings) }
    }
}

/// A microphone recorded on its own, for a voice-over: the same session as beside a screen recording, without one.
pub struct Voice {
    capture: DeviceCapture,
}

impl Voice {
    /// Starts recording `device` (the default microphone when `None`) into `output`, and returns once it sends sound
    /// (or after two seconds), so whatever starts next lines up with the file's beginning.
    pub fn start(device: Option<String>, output: &Path) -> Result<Self, CaptureError> {
        let choice = DeviceRecording { device, output: output.to_path_buf() };
        let capture = DeviceCapture::start(Some(&choice), None)?
            .ok_or_else(|| CaptureError::Device("No microphone was chosen.".into()))?;
        let recording = capture.shared.mic.lock().is_ok_and(|mic| mic.is_some());
        if !recording {
            // Why is in the warnings: not allowed, not found, or busy.
            let why = capture.warnings.first().cloned();
            return Err(CaptureError::Device(
                why.unwrap_or_else(|| "The microphone couldn't be recorded.".into()),
            ));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !capture.shared.heard.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(Self { capture })
    }

    /// Stops and finishes the file. Returns how long it plays, in seconds; fails (and leaves no file) when the
    /// microphone sent nothing.
    pub fn stop(self) -> Result<f64, CaptureError> {
        let stopped = self.capture.stop(0.0);
        match stopped.microphone {
            Some((first, end)) if end > first => Ok(end - first),
            _ => Err(CaptureError::Device(
                stopped
                    .warnings
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| "The microphone didn't send any sound.".into()),
            )),
        }
    }
}

/// How far the session's clock is from the host clock. Capture devices usually stamp with host time already, and
/// then this is zero; an audio device's own clock is the exception.
fn clock_offset(session: &AVCaptureSession) -> f64 {
    // SAFETY: reading the current time of two clocks.
    unsafe {
        let Some(clock) = session.synchronizationClock() else { return 0.0 };
        let offset = CMClock::host_time_clock().time().seconds() - clock.time().seconds();
        if offset.is_finite() && offset.abs() > 1e-3 { offset } else { 0.0 }
    }
}

fn poisoned() -> CaptureError {
    CaptureError::System("a recording lock was poisoned".into())
}

/// Adds the chosen device to the session, or says in plain words why it can't be used.
///
/// # Safety
/// The session must be between `beginConfiguration` and `commitConfiguration`.
unsafe fn add_input(
    session: &AVCaptureSession,
    choice: &DeviceRecording,
    media: &AVMediaType,
    name: &str,
) -> Result<(), String> {
    if !allowed(media) {
        let pane = if name == "camera" { "Camera" } else { "Microphone" };
        return Err(format!(
            "The {name} isn't allowed, so it wasn't recorded. Turn on Shotmaxxing in System Settings › Privacy & Security › {pane}."
        ));
    }
    let device =
        device(choice, media).ok_or_else(|| format!("No {name} was found, so it wasn't recorded."))?;
    // SAFETY: the device is valid; the caller keeps the session in configuration.
    unsafe {
        let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&device)
            .map_err(|e| format!("The {name} couldn't be opened: {}", e.localizedDescription()))?;
        if !session.canAddInput(&input) {
            return Err(format!("The {name} is busy, so it wasn't recorded."));
        }
        session.addInput(&input);
    }
    Ok(())
}
