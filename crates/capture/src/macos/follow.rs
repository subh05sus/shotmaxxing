//! Keeping a window take on its window.
//!
//! A window take records the window's display with only its app's windows drawn, cropped to the window
//! ([`Crop`] says why), so the crop has to go where the window goes. A thread reads the window's frame from the
//! window server about thirty times a second. While the window is anywhere but under the crop (being dragged or
//! resized, parked in Stage Manager's strip, minimised, or on another desktop) the take holds its last picture:
//! frames are let go, and the video shows the window still, as a recording of the window by itself would. Once the
//! window has been still for a moment, the crop moves there and frames flow again.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread::JoinHandle;
use std::time::Duration;

use block2::{DynBlock, RcBlock};
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{SCContentFilter, SCDisplay, SCRunningApplication, SCStream, SCWindow};

use super::{StreamSettings, clock, window};

/// How often the window's frame is read: each reading is a call to the window server of about 80 µs, so this costs a
/// quarter of a percent of one core, and a drag is caught within a frame or two.
const EVERY: Duration = Duration::from_millis(33);
/// Readings in a row the window must hold still (after the first) before the crop moves to it.
const STILL: u32 = 2;
/// Frames captured this long after the stream says the crop moved can still show it where it was (the first frame
/// cropped anew came 26 ms after, in a test), so they're let go too.
const SETTLING: f64 = 0.15;

/// Whether a frame belongs in the take, shared by the follower and the stream's output.
#[derive(Default)]
pub(super) struct Gate {
    /// The window isn't under the crop.
    held: AtomicBool,
    /// Host seconds, as bits: frames captured before this may not show the window where the crop is.
    from: AtomicU64,
}

impl Gate {
    /// Whether the frame captured at `pts` (host seconds) goes in the take.
    pub fn admits(&self, pts: f64) -> bool {
        !self.held.load(Ordering::Acquire) && pts >= f64::from_bits(self.from.load(Ordering::Acquire))
    }

    fn hold(&self) {
        self.held.store(true, Ordering::Release);
    }

    fn is_held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }

    /// Lets frames through again, those captured from `at` on (host seconds).
    fn open_from(&self, at: f64) {
        let from = f64::from_bits(self.from.load(Ordering::Acquire)).max(at);
        self.from.store(from.to_bits(), Ordering::Release);
        self.held.store(false, Ordering::Release);
    }
}

/// A display a window can be on.
pub(super) struct Screen {
    /// On the desktop, in points from the top left of the main display.
    pub frame: CGRect,
    pub display: Retained<SCDisplay>,
}

/// What a window take records: its app's windows on the display it's on, cropped to the window.
///
/// ScreenCaptureKit can record a window by itself, but a menu, popover or other child window that opens over its
/// edge is drawn with it: the picture becomes the window and the menu together, shrunk to fit the video, with black
/// where there's neither. A dropdown near the top of a Finder window did that for as long as it was open. A crop
/// keeps the window still and full size, and shows what opens inside it, cut at its edge.
pub(super) struct Crop {
    pub app: Retained<SCRunningApplication>,
    /// The app's other windows that were open when the take started, left out as they would be from a recording of
    /// the window alone. Menus, sheets and popovers open later, so they show.
    pub excepted: Vec<Retained<SCWindow>>,
    pub screens: Vec<Screen>,
    /// Which of `screens` the window is on.
    pub screen: usize,
    /// Where the window is, on the desktop.
    pub window: CGRect,
    /// For a full-screen app, how far its toolbars (windows of their own) reach above its window, in points; 0 for
    /// every other window.
    pub top: f64,
    /// What's recorded, on the desktop: the window, with its toolbars when it's in full screen ([`window::reach`]).
    pub rect: CGRect,
}

impl Crop {
    /// The app's windows on the display, but for the ones left out.
    pub fn filter(&self) -> Retained<SCContentFilter> {
        filter_for(&self.screens[self.screen].display, &self.app, &self.excepted)
    }

    /// The crop in its display's own points, as a stream's source rect.
    pub fn source(&self) -> CGRect {
        source_rect(self.screens[self.screen].frame, self.rect)
    }
}

fn filter_for(
    display: &SCDisplay,
    app: &Retained<SCRunningApplication>,
    excepted: &[Retained<SCWindow>],
) -> Retained<SCContentFilter> {
    // SAFETY: a new filter from live ScreenCaptureKit objects.
    unsafe {
        SCContentFilter::initWithDisplay_includingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            display,
            &NSArray::from_retained_slice(std::slice::from_ref(app)),
            &NSArray::from_retained_slice(excepted),
        )
    }
}

/// `rect` on the desktop, in the points of a display whose frame is `display`.
fn source_rect(display: CGRect, rect: CGRect) -> CGRect {
    CGRect::new(CGPoint::new(rect.origin.x - display.origin.x, rect.origin.y - display.origin.y), rect.size)
}

/// Which of `screens` holds all of `rect`, if one does (to a point either way, for rounding).
pub(super) fn screen_for(screens: &[Screen], rect: CGRect) -> Option<usize> {
    screens.iter().position(|screen| contains(screen.frame, rect))
}

fn contains(outer: CGRect, inner: CGRect) -> bool {
    const SLACK: f64 = 1.0;
    inner.origin.x >= outer.origin.x - SLACK
        && inner.origin.y >= outer.origin.y - SLACK
        && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + SLACK
        && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + SLACK
}

/// The same frame, to half a point.
fn same(a: CGRect, b: CGRect) -> bool {
    const SLACK: f64 = 0.5;
    (a.origin.x - b.origin.x).abs() <= SLACK
        && (a.origin.y - b.origin.y).abs() <= SLACK
        && (a.size.width - b.size.width).abs() <= SLACK
        && (a.size.height - b.size.height).abs() <= SLACK
}

/// The thread that moves a window take's crop.
pub(super) struct Follower {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// What the thread works with.
struct Following {
    stream: Retained<SCStream>,
    crop: Crop,
    settings: StreamSettings,
    id: u32,
    /// The take's size, the window's real size, to tell a Stage Manager thumbnail from a resized window.
    size: (f64, f64),
    stage_manager: bool,
    gate: Arc<Gate>,
}

// SAFETY: ScreenCaptureKit's stream, filter and shareable-content objects may be used from any thread (their work is
// asynchronous, answered on the framework's queues); the thread is the only user of these references besides the
// recorder, which only starts and stops the stream.
unsafe impl Send for Following {}

impl Follower {
    pub fn start(
        stream: Retained<SCStream>,
        crop: Crop,
        settings: StreamSettings,
        id: u32,
        size: (f64, f64),
        gate: Arc<Gate>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let following =
            Following { stream, crop, settings, id, size, stage_manager: window::stage_manager_on(), gate };
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("smx-window-follow".into())
            .spawn(move || run(following, &flag))
            .map_err(|error| tracing::warn!(%error, "the window take can't follow its window"))
            .ok();
        Self { stop, thread }
    }
}

impl Drop for Follower {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(mut f: Following, stop: &AtomicBool) {
    // The last frame read, and how many readings in a row it has held.
    let (mut last, mut still): (Option<CGRect>, u32) = (None, 0);
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(EVERY);
        let shown = window::placed(f.id).filter(|placed| {
            placed.on_screen && !(f.stage_manager && window::thumbnail_of(placed.bounds, f.size))
        });
        let Some(rect) = shown.map(|placed| placed.bounds) else {
            // Closed, minimised, on another desktop or parked in the strip: hold until it's back.
            if !f.gate.is_held() {
                tracing::debug!(id = f.id, "the window is away: holding the take");
            }
            f.gate.hold();
            (last, still) = (None, 0);
            continue;
        };
        if last.is_some_and(|last| same(last, rect)) {
            still += 1;
        } else {
            (last, still) = (Some(rect), 0);
        }
        if same(rect, f.crop.window) {
            // Under the crop: frames flow, from once it has been still there a moment.
            if f.gate.is_held() && still >= 1 {
                tracing::debug!(id = f.id, "the window is back under the crop");
                f.gate.open_from(clock::now());
            }
            continue;
        }
        if !f.gate.is_held() {
            tracing::debug!(id = f.id, ?rect, crop = ?f.crop.rect, "the window moved: holding the take");
        }
        f.gate.hold();
        if still < STILL {
            continue;
        }
        // Still somewhere new, or another size (shown in the take's size, as the window's whole picture). Partly off
        // its display, or across two, it stays held.
        let frames: Vec<CGRect> = f.crop.screens.iter().map(|screen| screen.frame).collect();
        let area = window::reach(rect, f.crop.top, &frames);
        let Some(screen) = screen_for(&f.crop.screens, area) else { continue };
        if move_crop(&f.stream, &mut f.crop, screen, rect, area, &f.settings) {
            tracing::debug!(id = f.id, ?rect, "the crop moved to the window");
            f.gate.open_from(clock::now() + SETTLING);
        } else {
            // Tried again in a moment, rather than every reading.
            std::thread::sleep(Duration::from_millis(250));
        }
        (last, still) = (None, 0);
    }
}

/// Moves the crop to `area` on `screens[screen]`, for the window now at `window`, first changing the stream's display
/// when it's another one, and waits for the stream to take each change. Whether it did.
fn move_crop(
    stream: &SCStream,
    crop: &mut Crop,
    screen: usize,
    window: CGRect,
    area: CGRect,
    settings: &StreamSettings,
) -> bool {
    if screen != crop.screen {
        let filter = filter_for(&crop.screens[screen].display, &crop.app, &crop.excepted);
        // SAFETY: a running stream and a valid filter; the block lives until the call returns, and the framework
        // copies it.
        if !applied(|done| unsafe { stream.updateContentFilter_completionHandler(&filter, Some(done)) }) {
            tracing::debug!("the window take couldn't move to the window's new display");
            return false;
        }
        crop.screen = screen;
    }
    let config = settings.configuration(Some(source_rect(crop.screens[screen].frame, area)));
    // SAFETY: as above, with a new configuration.
    if !applied(|done| unsafe { stream.updateConfiguration_completionHandler(&config, Some(done)) }) {
        tracing::debug!("the window take's crop couldn't follow the window");
        return false;
    }
    (crop.window, crop.rect) = (window, area);
    true
}

/// Makes a change through `call`, which hands the block it's given to ScreenCaptureKit, and waits up to a second for
/// the answer. Whether it worked.
fn applied(call: impl FnOnce(&DynBlock<dyn Fn(*mut NSError)>)) -> bool {
    let (tx, rx) = sync_channel::<bool>(1);
    let done = RcBlock::new(move |error: *mut NSError| {
        let _ = tx.try_send(error.is_null());
    });
    call(&done);
    rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::CGSize;

    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn the_crop_is_in_its_displays_own_points() {
        // A display to the left of the main one, and one above it.
        assert_eq!(
            source_rect(rect(-1920.0, 0.0, 1920.0, 1080.0), rect(-1500.0, 200.0, 800.0, 600.0)),
            rect(420.0, 200.0, 800.0, 600.0)
        );
        assert_eq!(
            source_rect(rect(0.0, -1117.0, 1728.0, 1117.0), rect(100.0, -900.0, 640.0, 400.0)),
            rect(100.0, 217.0, 640.0, 400.0)
        );
    }

    #[test]
    fn a_window_counts_as_on_a_display_only_when_all_of_it_is() {
        let main = rect(0.0, 0.0, 1470.0, 956.0);
        assert!(contains(main, rect(319.0, 304.0, 600.0, 412.0)));
        assert!(contains(main, rect(-0.5, 0.0, 1470.0, 956.4)), "rounding at the edges");
        assert!(!contains(main, rect(1200.0, 304.0, 600.0, 412.0)), "hanging off the right");
        assert!(!contains(main, rect(300.0, 700.0, 600.0, 412.0)), "below the bottom");
    }

    #[test]
    fn the_gate_holds_frames_and_lets_through_only_those_after_it_opens() {
        let gate = Gate::default();
        assert!(gate.admits(10.0), "open from the start");
        gate.hold();
        assert!(!gate.admits(10.5));
        gate.open_from(11.0);
        assert!(!gate.admits(10.9), "captured before it opened, maybe mid-move");
        assert!(gate.admits(11.0));
        // Opening again never lets earlier frames back in.
        gate.hold();
        gate.open_from(10.0);
        assert!(!gate.admits(10.5));
        assert!(gate.admits(11.2));
    }

    #[test]
    fn frames_are_the_same_to_half_a_point() {
        assert!(same(rect(319.0, 304.0, 600.0, 412.0), rect(319.4, 303.6, 600.0, 412.5)));
        assert!(!same(rect(319.0, 304.0, 600.0, 412.0), rect(320.0, 304.0, 600.0, 412.0)));
        assert!(!same(rect(319.0, 304.0, 600.0, 412.0), rect(319.0, 304.0, 601.0, 413.0)));
    }
}
