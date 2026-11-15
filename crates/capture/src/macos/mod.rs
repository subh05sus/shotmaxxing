//! macOS capture: ScreenCaptureKit for frames, AVAssetWriter for encoding, CoreGraphics for input telemetry.

mod audio;
mod ax;
mod clock;
mod devices;
mod follow;
mod input;
mod preview;
mod spaces;
mod symbols;
mod window;
mod writer;

pub use devices::Voice;
pub use preview::{cameras, microphones, screenshot, thumbnails};

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_foundation::{CFString, CGRect};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCRunningApplication, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamOutput, SCStreamOutputType, SCWindow,
};

use crate::{
    CaptureError, DisplayInfo, RecordOptions, RecordedTrack, Recording, Sources, Target, WindowInfo,
};
use audio::AudioWriter;
use devices::DeviceCapture;
use writer::Writer;

/// '420v': video-range 4:2:0, the encoder's native format, so frames go in without conversion.
pub(crate) const YUV_420V: u32 = 0x3432_3076;

/// A Core Foundation string constant as the NSString it is toll-free bridged to.
pub(crate) fn ns(string: &'static CFString) -> &'static NSString {
    // SAFETY: CFString and NSString are toll-free bridged, so the pointer is a valid NSString.
    unsafe { &*(string as *const CFString).cast::<NSString>() }
}

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
pub fn prepare() {
    ensure_window_server();
}

fn ensure_window_server() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // An app is connected already, and its recordings start off the main thread; only a command-line tool
        // starting from its main thread needs the explicit connection.
        if pthread_main_np() != 0 {
            NSApplicationLoad();
        }
    });
}

unsafe extern "C" {
    safe fn pthread_main_np() -> i32;
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

/// What ScreenCaptureKit can record: the displays, and the windows showing on this Space or, unless
/// `on_screen_only`, every window, wherever it is.
fn shareable_content(on_screen_only: bool) -> Result<Retained<SCShareableContent>, CaptureError> {
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
            true,
            on_screen_only,
            &handler,
        );
    }
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(content)) => Ok(content.0),
        Ok(Err(message)) => Err(CaptureError::System(message)),
        Err(_) => Err(CaptureError::System("listing screens and windows timed out".into())),
    }
}

pub fn sources() -> Result<Sources, CaptureError> {
    // Every window, wherever it is: a full-screen app's is on a Space of its own ([`window::listed`] says which go
    // in the list).
    let content = shareable_content(false)?;
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
        // Apps' ordinary windows, big enough to pick: not Shotmaxxing's, nor parts of macOS.
        let candidates: Vec<(Retained<SCWindow>, Retained<SCRunningApplication>)> = content
            .windows()
            .iter()
            .filter(|w| w.windowLayer() == 0)
            .filter_map(|w| {
                let app = w.owningApplication()?;
                let frame = w.frame();
                let skip = app.processID() == own_pid
                    || is_system_ui(&app.bundleIdentifier().to_string())
                    || frame.size.width < 80.0
                    || frame.size.height < 60.0;
                (!skip).then_some((w, app))
            })
            .collect();
        // Which Space the windows not showing are on, when the window server says.
        let layout = spaces::layout();
        let seen: Vec<window::Seen> = candidates
            .iter()
            .map(|(w, app)| window::Seen {
                pid: app.processID(),
                frame: w.frame(),
                on_screen: w.isOnScreen(),
                space: if w.isOnScreen() {
                    window::Space::Here
                } else {
                    layout
                        .as_ref()
                        .map_or(window::Space::Unknown, |layout| spaces::space_of(w.windowID(), layout))
                },
            })
            .collect();
        let mut windows: Vec<WindowInfo> = window::listed(&seen, &display_frames(&content))
            .into_iter()
            .map(|(index, full_screen)| {
                let (w, app) = &candidates[index];
                let frame = w.frame();
                WindowInfo {
                    id: w.windowID(),
                    title: w.title().map(|t| t.to_string()).unwrap_or_default(),
                    app: app.applicationName().to_string(),
                    bundle_id: app.bundleIdentifier().to_string(),
                    x: frame.origin.x,
                    y: frame.origin.y,
                    width: frame.size.width,
                    height: frame.size.height,
                    full_screen,
                    away: !w.isOnScreen(),
                }
            })
            .collect();
        // This Space's windows first, in the order they came (the record dialog picks the first window), then
        // full-screen apps on other Spaces, so the dialog doesn't start on one that would switch Spaces.
        windows.sort_by_key(|w| w.away);
        Ok(Sources { displays, windows })
    }
}

/// Why window `id` isn't among those showing: it's away (minimised, hidden or on another Space) when the window
/// server still has it, and gone when it doesn't.
fn not_showing(id: u32) -> CaptureError {
    if window::placed(id).is_some() {
        CaptureError::Away
    } else {
        CaptureError::SourceNotFound("that window".into())
    }
}

/// The frames of the displays, on the desktop.
fn display_frames(content: &SCShareableContent) -> Vec<CGRect> {
    // SAFETY: read-only property access on objects owned by `content`.
    unsafe { content.displays().iter().map(|display| display.frame()).collect() }
}

/// How to record a window, when it can be done the better way: its app's windows on the display it's on, cropped to
/// it ([`follow::Crop`] says why). Not when it isn't showing or isn't all on one display: then it's recorded by
/// itself, as before.
///
/// # Safety
/// `window` must be one of `content`'s windows.
unsafe fn crop_for(content: &SCShareableContent, window: &SCWindow) -> Option<follow::Crop> {
    let app = unsafe { window.owningApplication() }?;
    let id = unsafe { window.windowID() };
    let placed = window::placed(id)?;
    if !placed.on_screen {
        return None;
    }
    // SAFETY: read-only property access on objects owned by `content`.
    unsafe {
        let screens: Vec<follow::Screen> = content
            .displays()
            .iter()
            .map(|display| follow::Screen { frame: display.frame(), display })
            .collect();
        let screen = follow::screen_for(&screens, placed.bounds)?;
        let display = screens[screen].frame;
        let pid = app.processID();
        let others: Vec<Retained<SCWindow>> = content
            .windows()
            .iter()
            .filter(|w| {
                w.windowID() != id
                    && w.windowLayer() == 0
                    && w.isOnScreen()
                    && w.owningApplication().is_some_and(|a| a.processID() == pid)
            })
            .collect();
        // A full-screen app's toolbars can be windows of their own above its window (Chrome's tabs and address bar):
        // they're recorded with it.
        let full_screen = window::full_screen_shaped(placed.bounds, display);
        let toolbar = |w: &SCWindow| full_screen && window::toolbar_of(w.frame(), placed.bounds, display);
        let top = others
            .iter()
            .filter(|w| toolbar(w))
            .map(|w| placed.bounds.origin.y - w.frame().origin.y)
            .fold(0.0, f64::max);
        // The app's other windows, as the picker lists them, are left out.
        let excepted = others
            .into_iter()
            .filter(|w| {
                let frame = w.frame();
                frame.size.width >= 80.0 && frame.size.height >= 60.0 && !toolbar(w)
            })
            .collect();
        let rect = window::reach(placed.bounds, top, &[display]);
        Some(follow::Crop { app, excepted, screens, screen, window: placed.bounds, top, rect })
    }
}

/// Parts of macOS itself that own windows but aren't worth recording on their own (overlays, the Dock, menus).
fn is_system_ui(bundle: &str) -> bool {
    const SYSTEM: [&str; 10] = [
        "com.apple.WindowManager",
        "com.apple.dock",
        "com.apple.controlcenter",
        "com.apple.notificationcenterui",
        "com.apple.Spotlight",
        "com.apple.systemuiserver",
        "com.apple.TextInputMenuAgent",
        "com.apple.screencaptureui",
        "com.apple.wallpaper.agent",
        "com.apple.loginwindow",
    ];
    bundle.is_empty() || SYSTEM.contains(&bundle)
}

/// What the stream output shares with the recorder.
struct Shared {
    writer: Mutex<Option<Writer>>,
    /// What the computer plays, when it's being recorded.
    audio: Mutex<Option<AudioWriter>>,
    /// For a window take: holds frames while the window isn't under the crop.
    gate: Option<Arc<follow::Gate>>,
}

/// How a stream is set up, kept so a window take's crop can move without changing the rest.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StreamSettings {
    width_px: usize,
    height_px: usize,
    max_fps: u32,
    /// The computer's sound is recorded too.
    audio: bool,
}

impl StreamSettings {
    /// A configuration with these settings, cropped to `source` (in the display's points) when there is one.
    fn configuration(&self, source: Option<CGRect>) -> Retained<SCStreamConfiguration> {
        // SAFETY: a new configuration, set up before any stream sees it.
        unsafe {
            let config = SCStreamConfiguration::new();
            config.setWidth(self.width_px);
            config.setHeight(self.height_px);
            config.setMinimumFrameInterval(CMTime::with_seconds(1.0 / f64::from(self.max_fps.max(1)), 600));
            config.setPixelFormat(YUV_420V);
            config.setShowsCursor(false);
            config.setQueueDepth(6);
            if self.audio {
                config.setCapturesAudio(true);
                config.setSampleRate(48_000);
                config.setChannelCount(2);
                // This app's own sounds (the countdown) stay out of the take.
                config.setExcludesCurrentProcessAudio(true);
            }
            if let Some(source) = source {
                config.setSourceRect(source);
            }
            config
        }
    }
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
            let shared = &self.ivars().shared;
            if kind == SCStreamOutputType::Screen {
                // SAFETY: a frame from ScreenCaptureKit, valid for the call, and its timestamp.
                let pts = unsafe { sample.presentation_time_stamp().seconds() };
                if shared.gate.as_ref().is_some_and(|gate| !gate.admits(pts)) {
                    return;
                }
                if let Ok(mut guard) = shared.writer.lock()
                    && let Some(writer) = guard.as_mut()
                {
                    writer.append(sample);
                }
            } else if kind == SCStreamOutputType::Audio
                && let Ok(mut guard) = shared.audio.lock()
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
    _audio_queue: Option<dispatch2::DispatchRetained<DispatchQueue>>,
    shared: Arc<Shared>,
    devices: Option<DeviceCapture>,
    input: Option<input::Telemetry>,
    /// Moves a window take's crop with the window.
    follower: Option<follow::Follower>,
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
        // A window Stage Manager parked or shrank comes back to the stage first, to be recorded upright, at its size
        // and in its place, and a full-screen app's on its own Space brings the Mac there. A tabbed window's tab in
        // front is recorded, whichever tab was picked.
        let target = match options.target {
            Target::Window(id) => {
                window::bring_forward(id);
                Target::Window(window::stand_in(id))
            }
            display => display,
        };
        let content = shareable_content(true)?;
        let own_pid = std::process::id() as i32;
        // The microphone and camera start first: a camera takes a moment to wake, and the first words of a take
        // shouldn't be lost to it. If the screen then fails to start, dropping them stops them again.
        let devices = DeviceCapture::start(options.microphone.as_ref(), options.camera.as_ref())?;

        // SAFETY: ScreenCaptureKit objects are created and configured before the stream starts; all arguments are
        // valid objects from `content`.
        unsafe {
            let (filter, origin, app_name, window_title, picked, crop) = match target {
                Target::Display(id) => {
                    let displays = content.displays();
                    let display = match id {
                        Some(id) => displays.iter().find(|d| d.displayID() == id),
                        None => displays.iter().next(),
                    }
                    .ok_or_else(|| CaptureError::SourceNotFound("that display".into()))?;
                    // Leave Shotmaxxing out of the video, including windows it opens later (the recording pill and
                    // the take card): this process, and the app when the command records.
                    let mut left_out: Vec<Retained<SCRunningApplication>> = content
                        .applications()
                        .iter()
                        .filter(|a| {
                            a.processID() == own_pid
                                || a.bundleIdentifier().to_string() == shotmaxxing_core::APP_ID
                        })
                        .collect();
                    let mut kept: Vec<Retained<SCWindow>> = Vec::new();
                    if options.hide_desktop_icons {
                        // Desktop icons are Finder windows below the normal window layer. Finder is left out and
                        // its ordinary windows are kept, so only the icons disappear.
                        let finder = content
                            .applications()
                            .iter()
                            .find(|a| a.bundleIdentifier().to_string() == "com.apple.finder");
                        if let Some(finder) = finder {
                            let finder_pid = finder.processID();
                            kept.extend(content.windows().iter().filter(|w| {
                                w.windowLayer() >= 0
                                    && w.owningApplication().is_some_and(|a| a.processID() == finder_pid)
                            }));
                            left_out.push(finder);
                        }
                    }
                    let filter = if left_out.is_empty() {
                        SCContentFilter::initWithDisplay_excludingWindows(
                            SCContentFilter::alloc(),
                            &display,
                            &NSArray::new(),
                        )
                    } else {
                        SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                            SCContentFilter::alloc(),
                            &display,
                            &NSArray::from_retained_slice(&left_out),
                            &NSArray::from_retained_slice(&kept),
                        )
                    };
                    (filter, display.frame().origin, None, None, None, None)
                }
                Target::Window(id) => {
                    let window = content
                        .windows()
                        .iter()
                        .find(|w| w.windowID() == id)
                        .ok_or_else(|| not_showing(id))?;
                    // Still in Stage Manager's strip, it would come out as a small tilted picture.
                    let pid = window.owningApplication().map_or(0, |a| a.processID());
                    if window::parked(id, window.frame(), pid, &display_frames(&content)) {
                        return Err(CaptureError::Parked);
                    }
                    let app = window.owningApplication().map(|a| a.applicationName().to_string());
                    let title = window.title().map(|t| t.to_string());
                    match crop_for(&content, &window) {
                        Some(crop) => {
                            let filter = crop.filter();
                            let scale = f64::from(filter.pointPixelScale()).max(1.0);
                            let size = (crop.rect.size.width, crop.rect.size.height, scale);
                            (filter, crop.rect.origin, app, title, Some(size), Some(crop))
                        }
                        None => {
                            let alone = SCContentFilter::initWithDesktopIndependentWindow(
                                SCContentFilter::alloc(),
                                &window,
                            );
                            (alone, window.frame().origin, app, title, None, None)
                        }
                    }
                }
            };

            let (width_pt, height_pt, scale) = picked.unwrap_or_else(|| {
                let rect = filter.contentRect();
                (rect.size.width, rect.size.height, f64::from(filter.pointPixelScale()).max(1.0))
            });
            let window_id = match target {
                Target::Window(id) => Some(id),
                Target::Display(_) => None,
            };
            // Encoders want even dimensions.
            let width_px = ((width_pt * scale).round() as usize) & !1;
            let height_px = ((height_pt * scale).round() as usize) & !1;

            let settings = StreamSettings {
                width_px,
                height_px,
                max_fps: options.max_fps,
                audio: options.system_audio.is_some(),
            };
            let config = settings.configuration(crop.as_ref().map(follow::Crop::source));
            let audio = match &options.system_audio {
                Some(path) => Some(AudioWriter::new(path, 2)?),
                None => None,
            };

            let writer = Writer::new(&options.output, width_px, height_px, options.max_fps)?;
            let gate = crop.as_ref().map(|_| Arc::new(follow::Gate::default()));
            let shared = Arc::new(Shared {
                writer: Mutex::new(Some(writer)),
                audio: Mutex::new(audio),
                gate: gate.clone(),
            });
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
            let audio_queue = if options.system_audio.is_some() {
                let queue = DispatchQueue::new("dev.shotmaxxing.capture.audio", None);
                stream
                    .addStreamOutput_type_sampleHandlerQueue_error(
                        ProtocolObject::from_ref(&*output),
                        SCStreamOutputType::Audio,
                        Some(&queue),
                    )
                    .map_err(|e| CaptureError::System(e.localizedDescription().to_string()))?;
                Some(queue)
            } else {
                None
            };

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
            // How far a full-screen app's toolbars reach above its window, for the telemetry to follow the same area.
            let top = crop.as_ref().map_or(0.0, |crop| crop.top);
            // A window take's crop follows the window.
            let follower = match (crop, window_id, gate) {
                (Some(crop), Some(id), Some(gate)) => Some(follow::Follower::start(
                    stream.clone(),
                    crop,
                    settings,
                    id,
                    (width_pt, height_pt),
                    gate,
                )),
                _ => None,
            };
            let input = options.telemetry.then(|| {
                let origin = match window_id {
                    Some(id) => input::Origin::Window {
                        id,
                        start: (origin.x, origin.y),
                        size: (width_pt, height_pt),
                        top,
                    },
                    None => input::Origin::Fixed(origin.x, origin.y),
                };
                input::Telemetry::start(origin, started)
            });

            Ok(Self {
                stream,
                _output: output,
                _queue: queue,
                _audio_queue: audio_queue,
                shared,
                devices,
                input,
                follower,
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

    pub fn frames(&self) -> u64 {
        self.shared.writer.lock().ok().and_then(|w| w.as_ref().map(|w| w.frames)).unwrap_or(0)
    }

    pub fn stop(self) -> Result<Recording, CaptureError> {
        let stopped = clock::now();
        // The crop stops following before the stream stops.
        drop(self.follower);
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

        // Every other track is placed on the screen's timeline: its offset is where it starts relative to the
        // first frame.
        let track = |output: &std::path::Path, (start, end): (f64, f64)| RecordedTrack {
            output: output.to_path_buf(),
            offset_us: ((start - first) * 1e6).round() as i64,
            duration_us: ((end - start).max(0.0) * 1e6).round() as u64,
        };
        let mut warnings = Vec::new();
        let audio = self.shared.audio.lock().ok().and_then(|mut guard| guard.take());
        let system_audio = match (audio.map(AudioWriter::finish), &self.options.system_audio) {
            (Some(Ok(Some(times))), Some(path)) => Some(track(path, times)),
            (Some(Err(error)), _) => {
                warnings.push(format!("The computer's sound couldn't be saved: {error}"));
                None
            }
            _ => None,
        };
        let (mut microphone, mut camera) = (None, None);
        if let Some(devices) = self.devices {
            let recorded = devices.stop(stopped);
            warnings.extend(recorded.warnings);
            microphone = recorded
                .microphone
                .zip(self.options.microphone.as_ref())
                .map(|(times, d)| track(&d.output, times));
            camera =
                recorded.camera.zip(self.options.camera.as_ref()).map(|(times, d)| track(&d.output, times));
        }

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
            system_audio,
            microphone,
            camera,
            warnings,
        })
    }
}
