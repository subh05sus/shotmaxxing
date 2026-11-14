//! macOS capture: ScreenCaptureKit for frames, AVAssetWriter for encoding, CoreGraphics for input telemetry.

mod clock;
mod input;
mod writer;

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamOutput, SCStreamOutputType,
    SCWindow,
};

use crate::{CaptureError, DisplayInfo, RecordOptions, Recording, Sources, Target, WindowInfo};
use writer::Writer;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    safe fn CGPreflightScreenCaptureAccess() -> bool;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    safe fn NSApplicationLoad() -> bool;
}

/// Connects to the window server. The app already is connected; a command-line process isn't until something
/// asks, and CoreGraphics asserts if an input or cursor call comes first from a background thread.
fn ensure_window_server() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        NSApplicationLoad();
    });
}

/// Moves a value to another thread. Used only for framework objects that are documented as safe to hand over.
struct SendBox<T>(T);
// SAFETY: see each use; the wrapped objects are only handed from a completion handler to the waiting thread.
unsafe impl<T> Send for SendBox<T> {}

pub fn has_permission() -> bool {
    CGPreflightScreenCaptureAccess()
}

fn error_text(error: *mut NSError) -> String {
    // SAFETY: the framework passes either null or a valid NSError for the duration of the callback.
    unsafe { error.as_ref() }
        .map(|e| e.localizedDescription().to_string())
        .unwrap_or_else(|| "unknown error".into())
}

fn shareable_content() -> Result<Retained<SCShareableContent>, CaptureError> {
    if !has_permission() {
        return Err(CaptureError::PermissionDenied);
    }
    let (tx, rx) = sync_channel::<Result<SendBox<Retained<SCShareableContent>>, String>>(1);
    let handler = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        // SAFETY: a non-null content pointer is a valid object for the callback; retaining keeps it alive.
        let result = match unsafe { Retained::retain(content) } {
            Some(content) => Ok(SendBox(content)),
            None => Err(error_text(error)),
        };
        let _ = tx.send(result);
    });
    // SAFETY: the handler is a valid block; ScreenCaptureKit copies it.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true, true, &handler,
        );
    }
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(content)) => Ok(content.0),
        Ok(Err(message)) => Err(CaptureError::System(message)),
        Err(_) => Err(CaptureError::System("listing screens and windows timed out".into())),
    }
}

pub fn sources() -> Result<Sources, CaptureError> {
    let content = shareable_content()?;
    let own_pid = std::process::id() as i32;
    // SAFETY: read-only property access on objects owned by `content`.
    unsafe {
        let displays = content
            .displays()
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let frame = d.frame();
                DisplayInfo {
                    id: d.displayID(),
                    name: if i == 0 { "Main display".into() } else { format!("Display {}", i + 1) },
                    x: frame.origin.x,
                    y: frame.origin.y,
                    width: frame.size.width,
                    height: frame.size.height,
                }
            })
            .collect();
        let windows = content
            .windows()
            .iter()
            .filter(|w| w.windowLayer() == 0 && w.isOnScreen())
            .filter_map(|w| {
                let app = w.owningApplication()?;
                if app.processID() == own_pid {
                    return None;
                }
                let title = w.title().map(|t| t.to_string()).unwrap_or_default();
                let frame = w.frame();
                if frame.size.width < 80.0 || frame.size.height < 60.0 {
                    return None;
                }
                Some(WindowInfo {
                    id: w.windowID(),
                    title,
                    app: app.applicationName().to_string(),
                    bundle_id: app.bundleIdentifier().to_string(),
                    x: frame.origin.x,
                    y: frame.origin.y,
                    width: frame.size.width,
                    height: frame.size.height,
                })
            })
            .collect();
        Ok(Sources { displays, windows })
    }
}

/// What the stream output shares with the recorder.
struct Shared {
    writer: Mutex<Option<Writer>>,
}

struct OutputIvars {
    shared: Arc<Shared>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "SMXStreamOutput"]
    #[ivars = OutputIvars]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            if let Ok(mut guard) = self.ivars().shared.writer.lock()
                && let Some(writer) = guard.as_mut()
            {
                writer.append(sample);
            }
        }
    }
);

impl StreamOutput {
    fn new(shared: Arc<Shared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { shared });
        // SAFETY: plain NSObject initialisation.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Recorder {
    stream: Retained<SCStream>,
    _output: Retained<StreamOutput>,
    _queue: dispatch2::DispatchRetained<DispatchQueue>,
    shared: Arc<Shared>,
    input: Option<input::Telemetry>,
    started: f64,
    options: RecordOptions,
    size: (f64, f64),
    scale: f64,
    app_name: Option<String>,
    window_title: Option<String>,
}

impl Recorder {
    pub fn start(options: RecordOptions) -> Result<Self, CaptureError> {
        ensure_window_server();
        let content = shareable_content()?;
        let own_pid = std::process::id() as i32;

        // SAFETY: ScreenCaptureKit objects are created and configured before the stream starts; all arguments are
        // valid objects from `content`.
        unsafe {
            let (filter, origin, app_name, window_title) = match options.target {
                Target::Display(id) => {
                    let displays = content.displays();
                    let display = match id {
                        Some(id) => displays.iter().find(|d| d.displayID() == id),
                        None => displays.iter().next(),
                    }
                    .ok_or_else(|| CaptureError::SourceNotFound("that display".into()))?;
                    // Leave our own windows (the recording pill, the take card) out of the video.
                    let own: Vec<Retained<SCWindow>> = content
                        .windows()
                        .iter()
                        .filter(|w| w.owningApplication().is_some_and(|a| a.processID() == own_pid))
                        .collect();
                    let excluded = NSArray::from_retained_slice(&own);
                    let filter = SCContentFilter::initWithDisplay_excludingWindows(
                        SCContentFilter::alloc(),
                        &display,
                        &excluded,
                    );
                    (filter, display.frame().origin, None, None)
                }
                Target::Window(id) => {
                    let window = content
                        .windows()
                        .iter()
                        .find(|w| w.windowID() == id)
                        .ok_or_else(|| CaptureError::SourceNotFound("that window".into()))?;
                    let app = window.owningApplication().map(|a| a.applicationName().to_string());
                    let title = window.title().map(|t| t.to_string());
                    let filter =
                        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window);
                    (filter, window.frame().origin, app, title)
                }
            };

            let rect = filter.contentRect();
            let scale = f64::from(filter.pointPixelScale()).max(1.0);
            let (width_pt, height_pt) = (rect.size.width, rect.size.height);
            // Encoders want even dimensions.
            let width_px = ((width_pt * scale).round() as usize) & !1;
            let height_px = ((height_pt * scale).round() as usize) & !1;

            let config = SCStreamConfiguration::new();
            config.setWidth(width_px);
            config.setHeight(height_px);
            config
                .setMinimumFrameInterval(CMTime::with_seconds(1.0 / f64::from(options.max_fps.max(1)), 600));
            // '420v': the encoder's native format, so frames go in without conversion.
            config.setPixelFormat(0x3432_3076);
            config.setShowsCursor(false);
            config.setQueueDepth(6);

            let writer = Writer::new(&options.output, width_px, height_px, options.max_fps)?;
            let shared = Arc::new(Shared { writer: Mutex::new(Some(writer)) });
            let output = StreamOutput::new(shared.clone());
            let stream =
                SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None);
            let queue = DispatchQueue::new("dev.shotmaxxing.capture", None);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| CaptureError::System(e.localizedDescription().to_string()))?;

            let (tx, rx) = sync_channel::<Option<String>>(1);
            let handler = RcBlock::new(move |error: *mut NSError| {
                let _ = tx.send(if error.is_null() { None } else { Some(error_text(error)) });
            });
            stream.startCaptureWithCompletionHandler(Some(&handler));
            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(None) => {}
                Ok(Some(message)) => return Err(CaptureError::System(message)),
                Err(_) => return Err(CaptureError::System("the capture didn't start in time".into())),
            }

            let started = clock::now();
            let input = options.telemetry.then(|| input::Telemetry::start((origin.x, origin.y), started));

            Ok(Self {
                stream,
                _output: output,
                _queue: queue,
                shared,
                input,
                started,
                options,
                size: (width_pt, height_pt),
                scale,
                app_name,
                window_title,
            })
        }
    }

    pub fn elapsed(&self) -> f64 {
        clock::now() - self.started
    }

    pub fn stop(self) -> Result<Recording, CaptureError> {
        let stopped = clock::now();
        let (tx, rx) = sync_channel::<()>(1);
        let handler = RcBlock::new(move |_error: *mut NSError| {
            let _ = tx.send(());
        });
        // SAFETY: stopping a running stream with a valid block.
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&handler)) };
        let _ = rx.recv_timeout(Duration::from_secs(5));

        let writer = self
            .shared
            .writer
            .lock()
            .map_err(|_| CaptureError::System("writer lock poisoned".into()))?
            .take();
        let Some(writer) = writer else {
            return Err(CaptureError::System("the recording was already finished".into()));
        };
        let first = writer.first_pts.unwrap_or(self.started);
        let (frames, dropped) = (writer.frames, writer.dropped);
        writer.finish(stopped)?;

        let (cursor, events) = match self.input {
            Some(input) => input.stop(first),
            None => (Vec::new(), Vec::new()),
        };
        Ok(Recording {
            output: self.options.output,
            duration_us: ((stopped - first).max(0.0) * 1e6) as u64,
            frames,
            dropped,
            width: self.size.0,
            height: self.size.1,
            scale: self.scale,
            app_name: self.app_name,
            window_title: self.window_title,
            cursor,
            events,
        })
    }
}
