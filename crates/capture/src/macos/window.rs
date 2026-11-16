//! Where a window is, Stage Manager, and full-screen apps.
//!
//! With Stage Manager on, only the active app's windows are on the stage, and the rest wait in a strip at the side
//! of the screen as small tilted pictures. A window parked there reports the thumbnail's frame (a Finder window
//! reads as about 124 × 111 points, off the left edge), ScreenCaptureKit pictures it small, tilted and soft, and
//! while it isn't changing it isn't drawn at all, so a picture of it may never come. The window server also reports
//! a window smaller than it is while it's on its way out of the strip, and while its app is behind (a 560 × 392 test
//! window read as 505 × 354 the whole time). Neither it nor ScreenCaptureKit says how big a window really is (window
//! pictures carry no frame info on macOS 27, and a window-only filter's size is that same small frame); its app does,
//! through Accessibility ([`ax::own_frame`]), when that's allowed. So a window is brought forward before it's
//! pictured or recorded ([`bring_forward`]): its app is asked to come forward, and the window slides back to its
//! place in about half a second, which is waited for until its frame is its own again. A take that started on a
//! frame from partway (one did, at 399 × 201 points for a window of about 895 × 450) squeezed the whole window into
//! that small video. The picker, which mustn't move windows about, tells a parked window by where it is
//! ([`in_strip`]).
//!
//! A full-screen app has a Space of its own, and ScreenCaptureKit only pictures the Space that's showing (it can't
//! start on a window elsewhere), so asking that app forward also takes the Mac to its Space, as clicking it in the
//! Dock does. Its window is as wide as the screen and reaches the bottom ([`full_screen_shaped`]); its toolbars can be
//! windows of their own above it ([`toolbar_of`]), which a take records with it ([`reach`]).

use std::time::{Duration, Instant};

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFPreferencesCopyAppValue, CFRetained, CFString, CFType, CGPoint,
    CGRect, CGSize,
};
use objc2_core_graphics::{
    CGDisplayBounds, CGGetActiveDisplayList, CGRectMakeWithDictionaryRepresentation,
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowBounds, kCGWindowIsOnscreen, kCGWindowLayer,
    kCGWindowNumber, kCGWindowOwnerPID,
};
use objc2_foundation::NSString;

use super::ax;

/// A window as the window server has it now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its frame on the desktop, in points from the top left.
    pub bounds: CGRect,
    /// Showing on this desktop: not minimised, hidden, or on another desktop.
    pub on_screen: bool,
    /// The process that owns it.
    pub pid: i32,
}

/// A value in one of the window server's window dictionaries.
///
/// # Safety
/// `info` must be a dictionary from `CGWindowListCopyWindowInfo`.
pub(super) unsafe fn entry<'a>(info: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    // SAFETY: the dictionary's values are CF objects, valid while it is.
    unsafe { (info.value((key as *const CFString).cast()) as *const CFType).as_ref() }
}

/// One of the window server's window dictionaries, read: the window's number, its layer, and where it is.
///
/// # Safety
/// `info` must be a dictionary from `CGWindowListCopyWindowInfo`.
unsafe fn read(info: &CFDictionary) -> Option<(u32, i64, Placed)> {
    // SAFETY: the dictionary is keyed by the window server's CFString constants: the bounds a CGRect's dictionary
    // form, the number, layer and owner numbers, and on-screen a boolean that's left out when it isn't.
    unsafe {
        let found =
            entry(info, kCGWindowBounds).map(|value| &*(value as *const CFType).cast::<CFDictionary>());
        let mut bounds = CGRect::default();
        if !CGRectMakeWithDictionaryRepresentation(found, &mut bounds) {
            return None;
        }
        let number = entry(info, kCGWindowNumber)?.downcast_ref::<CFNumber>()?.as_i64()?;
        let layer = entry(info, kCGWindowLayer)?.downcast_ref::<CFNumber>()?.as_i64()?;
        let pid = entry(info, kCGWindowOwnerPID)?.downcast_ref::<CFNumber>()?.as_i32()?;
        let on_screen = entry(info, kCGWindowIsOnscreen)
            .and_then(|value| value.downcast_ref::<CFBoolean>())
            .is_some_and(CFBoolean::as_bool);
        Some((u32::try_from(number).ok()?, layer, Placed { bounds, on_screen, pid }))
    }
}

/// Where the window is, whether it shows, and whose it is, as the window server has it now. `None` once it has
/// closed.
pub fn placed(id: u32) -> Option<Placed> {
    let list = CGWindowListCopyWindowInfo(CGWindowListOption::OptionIncludingWindow, id)?;
    if list.count() < 1 {
        return None;
    }
    // SAFETY: the window server answers with an array of its window dictionaries.
    unsafe {
        let info = (list.value_at_index(0) as *const CFDictionary).as_ref()?;
        read(info).map(|(_, _, placed)| placed)
    }
}

/// The windows showing on this Space, front to back: each one's number, layer, and place.
fn showing() -> Vec<(u32, i64, Placed)> {
    let options = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, 0) else { return Vec::new() };
    (0..list.count())
        // SAFETY: the window server answers with an array of its window dictionaries.
        .filter_map(|index| unsafe {
            (list.value_at_index(index) as *const CFDictionary).as_ref().and_then(|i| read(i))
        })
        .collect()
}

/// The process whose window is in front at `point`, on the desktop in points, when one shows there.
pub fn owner_at(point: CGPoint) -> Option<i32> {
    front_owner(&showing(), &displays(), point)
}

/// Whose window, of `windows` front to back, is in front at `point`. A window above the usual layer that covers a
/// whole display is passed over: that's an overlay clicks go through. The Dock keeps one over every screen, on a
/// layer above the recording pill's, so without this every click would seem to be on the Dock.
fn front_owner(windows: &[(u32, i64, Placed)], displays: &[CGRect], point: CGPoint) -> Option<i32> {
    windows
        .iter()
        .filter(|(_, layer, w)| *layer <= 0 || !displays.iter().any(|&display| near(w.bounds, display)))
        .find(|(_, _, w)| {
            let b = w.bounds;
            point.x >= b.origin.x
                && point.x < b.origin.x + b.size.width
                && point.y >= b.origin.y
                && point.y < b.origin.y + b.size.height
        })
        .map(|(_, _, w)| w.pid)
}

/// The processes whose windows are left out of every take: this one, and the Shotmaxxing app when another process
/// (the command) records.
pub fn left_out_processes() -> Vec<i32> {
    let mut pids = vec![std::process::id() as i32];
    let app = NSString::from_str(shotmaxxing_core::APP_ID);
    pids.extend(
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&app)
            .iter()
            .map(|a| a.processIdentifier()),
    );
    pids.sort_unstable();
    pids.dedup();
    pids
}

/// The processes, other than those in `skip`, with a window in front of window `id` and over `area`: who has
/// something open over a window take (a menu, a sheet), front to back. None when the window isn't showing.
pub fn in_front_of(id: u32, area: CGRect, skip: &[i32]) -> Vec<i32> {
    front_of(&showing(), id, area, skip)
}

fn front_of(windows: &[(u32, i64, Placed)], id: u32, area: CGRect, skip: &[i32]) -> Vec<i32> {
    let Some(at) = windows.iter().position(|&(number, ..)| number == id) else { return Vec::new() };
    let mut pids = Vec::new();
    for (_, _, w) in &windows[..at] {
        if !skip.contains(&w.pid) && !pids.contains(&w.pid) && overlap(w.bounds, area) {
            pids.push(w.pid);
        }
    }
    pids
}

/// Whether two frames share any area.
fn overlap(a: CGRect, b: CGRect) -> bool {
    a.origin.x < b.origin.x + b.size.width
        && b.origin.x < a.origin.x + a.size.width
        && a.origin.y < b.origin.y + b.size.height
        && b.origin.y < a.origin.y + a.size.height
}

unsafe extern "C" {
    /// libproc's: the path of process `pid`'s executable, written into `buffer`. Its length, or 0 when unknown.
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
}

/// Whether process `pid` is a helper an app shows some of its interface through: an app extension (each System
/// Settings pane is one, and draws its own menus) or an XPC service (a sandboxed app's open and save panel).
pub fn helper(pid: i32) -> bool {
    let mut buffer = [0u8; 4096];
    // SAFETY: the buffer is as long as the size given.
    let length = unsafe { proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    let Ok(length) = usize::try_from(length) else { return false };
    std::str::from_utf8(&buffer[..length.min(buffer.len())]).is_ok_and(helper_path)
}

/// Whether the executable at `path` is a helper ([`helper`]). Never one that shows or asks for passwords (AutoFill,
/// the password prompt, Touch ID): a window take leaves those out, as it always has.
fn helper_path(path: &str) -> bool {
    const PRIVATE: [&str; 6] = [
        "AutoFill",
        "AuthenticationServices",
        "Password",
        "Keychain",
        "SecurityAgent",
        "LocalAuthentication",
    ];
    (path.contains(".appex/") || path.contains(".xpc/")) && !PRIVATE.iter().any(|word| path.contains(word))
}

/// The displays' frames on the desktop, in points from the top left of the main display.
pub fn displays() -> Vec<CGRect> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: room for 16 displays, and the count of those filled in.
    if unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) }.0 != 0 {
        return Vec::new();
    }
    ids[..(count as usize).min(ids.len())].iter().map(|&id| CGDisplayBounds(id)).collect()
}

/// One of another app's settings, as the person has it.
fn preference(domain: &'static str, key: &'static str) -> Option<CFRetained<CFType>> {
    CFPreferencesCopyAppValue(&CFString::from_static_str(key), &CFString::from_static_str(domain))
}

/// Whether Stage Manager is on.
pub fn stage_manager_on() -> bool {
    let Some(value) = preference("com.apple.WindowManager", "GloballyEnabled") else { return false };
    // A boolean, or a number when it was set by hand.
    match value.downcast_ref::<CFBoolean>() {
        Some(flag) => flag.as_bool(),
        None => value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64).is_some_and(|n| n != 0),
    }
}

/// Whether Stage Manager keeps its strip on the right of the screen: it does when the Dock is on the left.
pub fn strip_on_right() -> bool {
    preference("com.apple.dock", "orientation")
        .and_then(|value| value.downcast_ref::<CFString>().map(ToString::to_string))
        .is_some_and(|side| side == "left")
}

/// The most a strip thumbnail measures either way, in points. Those seen were 92 × 107 to 147 × 97.
const THUMBNAIL_MOST: f64 = 300.0;
/// How far from the screen's edge a strip thumbnail's middle can be, in points, either way: they sit a few dozen
/// points in, and a tilted one can hang off the edge.
const STRIP_BAND: f64 = 260.0;

/// Whether a window with this frame is in Stage Manager's strip, going by where it is: small, with its middle in
/// the band along the strip's side (the right when `right`) of one of `displays` (their frames). Only meaningful with
/// Stage Manager on, where windows on the stage keep out of that band.
pub fn in_strip(frame: CGRect, displays: &[CGRect], right: bool) -> bool {
    let (w, h) = (frame.size.width, frame.size.height);
    if w > THUMBNAIL_MOST || h > THUMBNAIL_MOST {
        return false;
    }
    let (x, y) = (frame.origin.x + w / 2.0, frame.origin.y + h / 2.0);
    displays.iter().any(|display| {
        let edge = if right { display.origin.x + display.size.width } else { display.origin.x };
        (x - edge).abs() <= STRIP_BAND && y >= display.origin.y && y <= display.origin.y + display.size.height
    })
}

/// Whether window `id`, owned by `pid` and showing at `frame`, is parked in Stage Manager's strip: by where it is
/// (on one of `displays`), unless its app says that small frame is its real size, as for a small window near the
/// screen's edge.
pub fn parked(id: u32, frame: CGRect, pid: i32, displays: &[CGRect]) -> bool {
    stage_manager_on()
        && in_strip(frame, displays, strip_on_right())
        && ax::own_frame(id, pid).is_none_or(|own| !at_real_size(frame, (own.size.width, own.size.height)))
}

/// How often a window coming forward is looked at.
const TICK: Duration = Duration::from_millis(30);
/// The longest a window is waited for. Stage Manager takes about half a second to bring one back, and going to a
/// full-screen app's Space about as long.
const MOST: Duration = Duration::from_secs(3);
/// Readings in a row a window has to hold still at the size its app gives (90 ms)...
const STILL_KNOWN: u32 = 3;
/// ...or, when its app can't say (no Accessibility), for a quarter of a second, so a pause partway isn't taken
/// for the end...
const STILL_GUESSED: u32 = 8;
/// ...or, at a size other than the one its app gives (an app whose own frame isn't its window's), for 0.6 s:
/// Stage Manager never stops that long partway.
const STILL_LONG: u32 = 20;
/// Without its app's word, how long a window that needn't move is given to start moving before it's taken as in
/// place.
const START_MOVING: Duration = Duration::from_millis(400);
/// How often its app is asked for its size again while the window isn't at it (an answer from partway is replaced).
const ASK_AGAIN: Duration = Duration::from_millis(200);
/// How long a window that has come from another Space is given for the switch to finish sliding in.
const SPACE_SWITCH: Duration = Duration::from_millis(350);

/// Gets a window ready to be recorded or pictured, and waits (at most three seconds) until it is: one Stage Manager
/// parked, or shrank while its app is behind, comes back to the stage at its own size, and one on another Space (a
/// full-screen app's) comes to this one. Its app is asked to come forward, which macOS allows while Shotmaxxing is
/// the active app, as it is when someone has just clicked Record or picked a window, and which also takes the Mac to
/// that app's Space. Does nothing for a window that's showing with Stage Manager off, or whose app is already in
/// front, so nothing moves without need.
pub fn bring_forward(id: u32) {
    let Some(before) = placed(id) else { return };
    let stage_manager = stage_manager_on();
    let away = !before.on_screen;
    if !stage_manager && !away {
        return;
    }
    // One on another Space comes by itself: its app's front window may be another (Chrome with two full-screen
    // windows), and the app coming forward would go to that one's Space.
    if away && super::spaces::go_to_window(before.pid, id) {
        settle(id, before, true);
        return;
    }
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(before.pid) else { return };
    if app.isActive() && !away {
        return;
    }
    let parked = before.on_screen && parked(id, before.bounds, before.pid, &displays());
    if !app.activateWithOptions(NSApplicationActivationOptions::empty()) {
        tracing::debug!(id, "the window's app didn't come forward");
        return;
    }
    settle(id, before, parked || away);
}

/// Waits for a window to come to rest where it'll be recorded: showing on this Space, and still for a moment at
/// the size its app says it is, or when its app can't say, out of Stage Manager's strip. One that was parked or on
/// another Space (`must_change`) has to have changed first, and one from another Space gets time for the switch to
/// finish. A full-screen app's tab that isn't the one in front never shows, so then the tab in front, in its place,
/// is waited for: it's the one recorded ([`stand_in`]). At most [`MOST`].
fn settle(mut id: u32, before: Placed, must_change: bool) {
    let start = Instant::now();
    let (stage_manager, right, screens) = (stage_manager_on(), strip_on_right(), displays());
    let (mut last, mut same, mut changed) = (before, 0u32, false);
    let mut own: Option<(f64, f64)> = None;
    let mut asked: Option<Instant> = None;
    // When it first showed here: from the start, for one that was showing.
    let mut arrived: Option<Instant> = before.on_screen.then_some(start);
    while start.elapsed() < MOST {
        std::thread::sleep(TICK);
        let Some(mut now) = placed(id) else { return };
        if !now.on_screen && !before.on_screen {
            let shown = stand_in(id);
            if shown != id
                && let Some(front) = placed(shown)
            {
                (id, now) = (shown, front);
            }
        }
        if now == last {
            same += 1;
        } else {
            (last, same, changed) = (now, 0, true);
        }
        if !now.on_screen {
            continue;
        }
        // Come from another Space, the switch may still be sliding it in.
        let here = *arrived.get_or_insert_with(Instant::now);
        if !before.on_screen && here.elapsed() < SPACE_SWITCH {
            continue;
        }
        if own.is_none_or(|size| !at_real_size(now.bounds, size))
            && asked.is_none_or(|at| at.elapsed() >= ASK_AGAIN)
        {
            if let Some(frame) = ax::own_frame(id, now.pid) {
                own = Some((frame.size.width, frame.size.height));
            }
            asked = Some(Instant::now());
        }
        let out_of_strip = !(stage_manager && in_strip(now.bounds, &screens, right));
        let ready = match own {
            Some(size) if at_real_size(now.bounds, size) => same >= STILL_KNOWN,
            Some(_) => out_of_strip && same >= STILL_LONG,
            None => {
                out_of_strip
                    && (same >= STILL_LONG
                        || (same >= STILL_GUESSED && (changed || start.elapsed() >= START_MOVING)))
            }
        };
        if ready && (changed || !must_change) {
            return;
        }
    }
    tracing::debug!(id, ?last, ?own, "the window didn't come to rest in time");
}

/// The window to record or picture for `id`: itself when it's showing, or else a window of the same app showing in
/// the same place. That's how a tabbed window's other tabs are, each a window of its own shown one at a time, a
/// full-screen app's too, so the tab listed for one may not be the tab in front when its Space shows.
pub fn stand_in(id: u32) -> u32 {
    let Some(chosen) = placed(id) else { return id };
    if chosen.on_screen {
        return id;
    }
    let spaces = super::spaces::spaces_of(id);
    showing()
        .into_iter()
        .find(|&(number, layer, w)| {
            layer == 0
                && w.pid == chosen.pid
                && near(w.bounds, chosen.bounds)
                && same_window(spaces.as_deref(), super::spaces::spaces_of(number).as_deref())
        })
        .map_or(id, |(number, ..)| number)
}

/// Whether a window on `spaces` and one showing on `showing` can be tabs of one window: a tab behind is on no
/// Space, and the tab in front is on the window's. Two windows on different Spaces are two windows, as two
/// full-screen windows of one app are. When the window server doesn't say, they're taken as tabs.
fn same_window(spaces: Option<&[i64]>, showing: Option<&[i64]>) -> bool {
    match (spaces, showing) {
        (Some(spaces), Some(showing)) => spaces.is_empty() || spaces.iter().any(|s| showing.contains(s)),
        _ => true,
    }
}

/// Within a point either way: frames read from different calls round differently.
const SLACK: f64 = 1.0;

/// The same frame, to a point.
fn near(a: CGRect, b: CGRect) -> bool {
    (a.origin.x - b.origin.x).abs() <= SLACK
        && (a.origin.y - b.origin.y).abs() <= SLACK
        && (a.size.width - b.size.width).abs() <= SLACK
        && (a.size.height - b.size.height).abs() <= SLACK
}

/// Whether a window with this frame fills `display` the way a full-screen app's window does: the display's whole
/// width, down to its bottom edge, and at least half its height. (On a Mac with a camera notch, the strip beside the
/// notch stays empty above it, and an app's toolbars can be windows of their own: [`toolbar_of`].) A window someone
/// sized to the whole screen on a desktop matches too, which changes nothing for it: it has no such toolbars.
pub fn full_screen_shaped(frame: CGRect, display: CGRect) -> bool {
    (frame.origin.x - display.origin.x).abs() <= SLACK
        && (frame.size.width - display.size.width).abs() <= SLACK
        && (frame.origin.y + frame.size.height - (display.origin.y + display.size.height)).abs() <= SLACK
        && frame.origin.y >= display.origin.y - SLACK
        && frame.size.height >= display.size.height / 2.0
}

/// Whether `frame` is a full-screen app's toolbar, a window of its own as wide as `display` that starts above the
/// app's full-screen window `window` there: Chrome's tabs and address bar, Terminal's tab bar.
pub fn toolbar_of(frame: CGRect, window: CGRect, display: CGRect) -> bool {
    (frame.origin.x - display.origin.x).abs() <= SLACK
        && (frame.size.width - display.size.width).abs() <= SLACK
        && frame.origin.y >= display.origin.y - SLACK
        && frame.origin.y < window.origin.y - SLACK
        && frame.origin.y + frame.size.height <= window.origin.y + window.size.height
}

/// What a take of a window with this frame records: the window, and while it's in full screen on one of `displays`
/// (their frames), the `top` points of toolbars above it as well (measured when the take started).
pub fn reach(frame: CGRect, top: f64, displays: &[CGRect]) -> CGRect {
    if top <= 0.0 || !displays.iter().any(|display| full_screen_shaped(frame, *display)) {
        return frame;
    }
    CGRect::new(
        CGPoint::new(frame.origin.x, frame.origin.y - top),
        CGSize::new(frame.size.width, frame.size.height + top),
    )
}

/// Where the window server says a window is ([`super::spaces`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// It can't say (its calls are missing): windows are told apart by their shape alone.
    Unknown,
    /// On no Space: minimised, hidden, a tab behind another, or a helper that never shows.
    Nowhere,
    /// On a Space that's showing.
    Here,
    /// On a desktop that isn't showing.
    OtherDesktop,
    /// On a full-screen app's Space that isn't showing: which one. Each full-screen window has a Space of its own.
    OtherFullScreen(i64),
}

/// A window as the picker weighs it.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub pid: i32,
    pub frame: CGRect,
    pub on_screen: bool,
    pub space: Space,
}

/// Which of `windows` (apps' ordinary windows, front to back) the picker lists, each with whether it's in full
/// screen. Every one showing on this Space is, but for a full-screen app's toolbars, which are recorded with its
/// window. From other Spaces, only windows macOS will go to: when one window can be brought forward by itself
/// (`one_window`, [`super::spaces::go_to_window`]) that's any of them; otherwise only those of apps with no window
/// showing here (asked to come forward, an app goes to its front window's Space then, and stays here otherwise).
/// Windows on other desktops are listed when the window server says where windows are ([`Space`]), and full-screen
/// apps' windows once per Space: two full-screen windows of one app are two Spaces, and listed apart.
/// Windows on no Space (minimised, hidden, tabs behind others, helpers that never show) aren't listed. Without the
/// window server's word, they can't be told from windows on other desktops, so then only full-screen apps are, by
/// their shape, once per app and display: their tabs are windows of their own, all the same size, and only the one
/// in front shows when the Space does ([`stand_in`] records that one).
pub fn listed(windows: &[Seen], displays: &[CGRect], one_window: bool) -> Vec<(usize, bool)> {
    let fills = |frame: CGRect| displays.iter().position(|display| full_screen_shaped(frame, *display));
    // Full-screen windows showing here, with their app and display.
    let showing: Vec<(i32, usize, CGRect)> = windows
        .iter()
        .filter(|w| w.on_screen)
        .filter_map(|w| fills(w.frame).map(|d| (w.pid, d, w.frame)))
        .collect();
    // Apps with a window showing here.
    let here: Vec<i32> = windows.iter().filter(|w| w.on_screen).map(|w| w.pid).collect();
    let mut shaped: Vec<(i32, usize)> = Vec::new();
    let mut spaces: Vec<(i32, i64)> = Vec::new();
    let mut out = Vec::new();
    for (index, w) in windows.iter().enumerate() {
        let display = fills(w.frame);
        if w.on_screen {
            let toolbar = display.is_none()
                && showing
                    .iter()
                    .any(|&(pid, d, main)| pid == w.pid && toolbar_of(w.frame, main, displays[d]));
            if !toolbar {
                out.push((index, display.is_some()));
            }
            continue;
        }
        let reachable = one_window && w.space != Space::Unknown;
        if here.contains(&w.pid) && !reachable {
            continue;
        }
        match w.space {
            Space::OtherDesktop => out.push((index, false)),
            Space::OtherFullScreen(space) => {
                // Its toolbars and any small window on its Space go with it.
                if display.is_some() && !spaces.contains(&(w.pid, space)) {
                    spaces.push((w.pid, space));
                    out.push((index, true));
                }
            }
            Space::Unknown => {
                if let Some(d) = display
                    && !shaped.contains(&(w.pid, d))
                {
                    shaped.push((w.pid, d));
                    out.push((index, true));
                }
            }
            Space::Here | Space::Nowhere => {}
        }
    }
    out
}

/// Whether `frame` is a Stage Manager strip thumbnail of a window whose real size is `size`. A thumbnail is a sixth
/// of the window or so and never big, where a window someone resized rarely shrinks below half both ways.
pub fn thumbnail_of(frame: CGRect, size: (f64, f64)) -> bool {
    let (w, h) = (frame.size.width, frame.size.height);
    w < size.0 * 0.5 && h < size.1 * 0.5 && w <= THUMBNAIL_MOST && h <= THUMBNAIL_MOST
}

/// Whether `frame` is the window at its real size, not a Stage Manager thumbnail or a frame mid-animation.
pub fn at_real_size(frame: CGRect, size: (f64, f64)) -> bool {
    (frame.size.width - size.0).abs() <= 2.0 && (frame.size.height - size.1).abs() <= 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(w, h))
    }

    #[test]
    fn clicks_go_through_overlays_that_cover_a_whole_display() {
        let display = rect(0.0, 0.0, 1470.0, 956.0);
        let shown = |bounds, pid| Placed { bounds, on_screen: true, pid };
        // Front to back, as the window server lists them: the Dock's overlay over the whole screen and the Dock
        // itself, both on layer 20, the recording pill on layer 5, and an app's window.
        let windows = [
            (39, 20, shown(display, 1270)),
            (40, 20, shown(rect(400.0, 880.0, 670.0, 76.0), 1270)),
            (10524, 5, shown(rect(555.0, 33.0, 360.0, 84.0), 55620)),
            (10529, 0, shown(rect(55.0, 33.0, 1249.0, 860.0), 42)),
        ];
        let at = |x, y| front_owner(&windows, &[display], CGPoint::new(x, y));
        assert_eq!(at(818.8, 71.0), Some(55620), "the pill, under the Dock's overlay");
        assert_eq!(at(700.0, 900.0), Some(1270), "the Dock");
        assert_eq!(at(300.0, 500.0), Some(42), "the app");
        assert_eq!(at(10.0, 940.0), None, "the desktop");
        // A full-screen app's window covers the display too, but on the usual layer: clicks go to it.
        let full = [(39, 20, shown(display, 1270)), (7, 0, shown(display, 42))];
        assert_eq!(front_owner(&full, &[display], CGPoint::new(10.0, 10.0)), Some(42));
    }

    #[test]
    fn whats_open_over_a_window_is_told_by_who_owns_it() {
        let shown = |bounds, pid| Placed { bounds, on_screen: true, pid };
        let window = rect(384.0, 152.0, 740.0, 625.0);
        // Front to back: the Dock's overlay, a pane's menu over the window's top edge, the recording pill (ours),
        // a menu elsewhere on the screen, the window itself, and another app's window behind it.
        let windows = [
            (39, 20, shown(rect(0.0, 0.0, 1470.0, 956.0), 1270)),
            (900, 101, shown(rect(950.0, 60.0, 200.0, 260.0), 25035)),
            (10524, 5, shown(rect(555.0, 33.0, 360.0, 84.0), 55620)),
            (901, 101, shown(rect(20.0, 800.0, 150.0, 90.0), 31337)),
            (10308, 0, shown(window, 600)),
            (77, 0, shown(rect(300.0, 300.0, 600.0, 400.0), 700)),
        ];
        assert_eq!(front_of(&windows, 10308, window, &[600, 55620]), vec![1270, 25035]);
        assert_eq!(front_of(&windows, 10308, window, &[600, 55620, 1270, 25035]), Vec::<i32>::new());
        assert!(front_of(&windows, 4242, window, &[]).is_empty(), "a window that isn't showing");
    }

    #[test]
    fn helpers_are_app_extensions_and_xpc_services_but_never_password_ones() {
        assert!(helper_path(
            "/System/Library/ExtensionKit/Extensions/Bluetooth.appex/Contents/MacOS/Bluetooth"
        ));
        assert!(helper_path(
            "/System/Library/Frameworks/AppKit.framework/Versions/C/XPCServices/com.apple.appkit.xpc.openAndSavePanelService.xpc/Contents/MacOS/com.apple.appkit.xpc.openAndSavePanelService"
        ));
        assert!(!helper_path("/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock"), "an app");
        assert!(!helper_path("/Applications/Notes.app/Contents/MacOS/Notes"), "an app");
        assert!(!helper_path(
            "/System/Library/Frameworks/AuthenticationServices.framework/XPCServices/AutoFillPanelService.xpc/Contents/MacOS/AutoFillPanelService"
        ));
        assert!(!helper_path(
            "/System/Library/Frameworks/Security.framework/Versions/A/MachServices/SecurityAgent.bundle/Contents/MacOS/SecurityAgent"
        ));
    }

    #[test]
    fn a_thumbnail_frame_is_not_the_real_size() {
        assert!(at_real_size(rect(-201.0, 419.0, 860.0, 501.5), (860.0, 500.0)));
        assert!(!at_real_size(rect(-201.0, 419.0, 147.0, 97.0), (860.0, 500.0)), "a Stage Manager thumbnail");
    }

    #[test]
    fn strip_thumbnails_are_told_from_resized_windows() {
        let frame = |w: f64, h: f64| rect(32.0, 661.0, w, h);
        // Thumbnails seen in the strip, for windows of 600 × 412, 860 × 500 and 920 × 464 points.
        assert!(thumbnail_of(frame(92.0, 114.0), (600.0, 412.0)));
        assert!(thumbnail_of(frame(147.0, 97.0), (860.0, 500.0)));
        assert!(thumbnail_of(frame(124.0, 111.0), (920.0, 464.0)));
        // A window made smaller, narrower or shorter is followed instead.
        assert!(!thumbnail_of(frame(500.0, 300.0), (920.0, 464.0)));
        assert!(!thumbnail_of(frame(300.0, 460.0), (920.0, 464.0)));
        assert!(
            !thumbnail_of(rect(32.0, 661.0, 400.0, 180.0), (920.0, 464.0)),
            "under half both ways, but too big for one"
        );
        // A small window is never mistaken for its own thumbnail.
        assert!(!thumbnail_of(frame(200.0, 150.0), (240.0, 180.0)));
    }

    #[test]
    fn strip_thumbnails_are_found_by_where_they_are() {
        let laptop = rect(0.0, 0.0, 1470.0, 956.0);
        let beside = rect(1470.0, 0.0, 1920.0, 1080.0);
        let displays = [laptop, beside];
        // Thumbnails seen in the strip on a laptop's display, one of them tilted off its edge.
        for parked in [
            rect(32.0, 661.0, 92.0, 114.0),
            rect(15.0, 661.0, 95.0, 114.0),
            rect(32.0, 589.0, 92.0, 107.0),
            rect(-201.0, 419.0, 147.0, 97.0),
            rect(16.0, 224.0, 123.0, 114.0),
        ] {
            assert!(in_strip(parked, &displays, false), "{parked:?}");
        }
        // Windows on the stage: a normal one, a small one in the middle, a small one on the side with no strip.
        assert!(!in_strip(rect(319.0, 304.0, 600.0, 412.0), &displays, false));
        assert!(!in_strip(rect(600.0, 400.0, 200.0, 150.0), &displays, false));
        assert!(!in_strip(rect(1300.0, 400.0, 150.0, 120.0), &[laptop], false));
        // With the Dock on the left, the strip is on the right.
        assert!(in_strip(rect(1300.0, 400.0, 150.0, 120.0), &[laptop], true));
        assert!(!in_strip(rect(32.0, 661.0, 92.0, 114.0), &[laptop], true));
        // Each display has its own strip.
        assert!(in_strip(rect(1500.0, 500.0, 100.0, 100.0), &displays, false));
        // Below every display isn't in a strip.
        assert!(!in_strip(rect(20.0, 1200.0, 100.0, 100.0), &[laptop], false));
    }

    /// The laptop display these frames were read on, 1470 × 956 points with a camera notch.
    const LAPTOP: CGRect =
        CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: 1470.0, height: 956.0 } };

    #[test]
    fn full_screen_windows_are_told_by_their_shape() {
        // Read on a Mac with a notch: Chrome's and Terminal's full-screen windows, below their toolbars.
        assert!(full_screen_shaped(rect(0.0, 121.0, 1470.0, 835.0), LAPTOP));
        assert!(full_screen_shaped(rect(0.0, 69.0, 1470.0, 887.0), LAPTOP));
        // A display to the right, and one above the main display.
        assert!(full_screen_shaped(rect(1470.0, 0.0, 1920.0, 1080.0), rect(1470.0, 0.0, 1920.0, 1080.0)));
        assert!(full_screen_shaped(rect(0.0, -1050.0, 1680.0, 1050.0), rect(0.0, -1050.0, 1680.0, 1050.0)));
        // Ordinary windows: the Shotmaxxing window, one above the Dock, one hanging off the right, a toolbar.
        assert!(!full_screen_shaped(rect(224.0, 175.0, 1120.0, 740.0), LAPTOP));
        assert!(!full_screen_shaped(rect(0.0, 33.0, 1470.0, 860.0), LAPTOP));
        assert!(!full_screen_shaped(rect(200.0, 121.0, 1470.0, 835.0), LAPTOP));
        assert!(!full_screen_shaped(rect(0.0, 33.0, 1470.0, 124.0), LAPTOP));
    }

    #[test]
    fn a_full_screen_apps_toolbars_are_the_screen_wide_windows_above_it() {
        let chrome = rect(0.0, 121.0, 1470.0, 835.0);
        // Its tab strip and address bar, read as one window and as two.
        assert!(toolbar_of(rect(0.0, 33.0, 1470.0, 124.0), chrome, LAPTOP));
        assert!(toolbar_of(rect(0.0, 33.0, 1470.0, 41.0), chrome, LAPTOP));
        assert!(toolbar_of(rect(0.0, 74.0, 1470.0, 47.0), chrome, LAPTOP));
        assert!(
            toolbar_of(rect(0.0, 33.0, 1470.0, 68.0), rect(0.0, 69.0, 1470.0, 887.0), LAPTOP),
            "Terminal's"
        );
        // Popups and narrower windows aren't.
        assert!(!toolbar_of(rect(135.0, 59.0, 858.0, 89.0), chrome, LAPTOP));
        assert!(!toolbar_of(rect(584.0, 106.0, 403.0, 84.0), chrome, LAPTOP));
        // Nor is a window below its top.
        assert!(!toolbar_of(rect(0.0, 500.0, 1470.0, 100.0), chrome, LAPTOP));
    }

    #[test]
    fn a_take_of_a_full_screen_window_reaches_up_over_its_toolbars() {
        let chrome = rect(0.0, 121.0, 1470.0, 835.0);
        assert_eq!(reach(chrome, 88.0, &[LAPTOP]), rect(0.0, 33.0, 1470.0, 923.0));
        // Out of full screen, or with nothing above it, it's the window alone.
        let left = rect(200.0, 150.0, 900.0, 600.0);
        assert_eq!(reach(left, 88.0, &[LAPTOP]), left);
        assert_eq!(reach(chrome, 0.0, &[LAPTOP]), chrome);
    }

    #[test]
    fn the_picker_lists_full_screen_apps_on_other_spaces_once() {
        // Without the window server's word on where windows are.
        let seen =
            |pid: i32, frame: CGRect, on_screen: bool| Seen { pid, frame, on_screen, space: Space::Unknown };
        let (chrome, terminal, finder, wakatime) = (1, 2, 3, 4);
        // As read while on the Desktop: Chrome and Terminal are full screen on Spaces of their own.
        let windows = [
            seen(finder, rect(224.0, 175.0, 900.0, 450.0), true),
            seen(chrome, rect(0.0, 33.0, 1470.0, 124.0), false),
            seen(chrome, rect(0.0, 121.0, 1470.0, 835.0), false),
            // Terminal's three tabs, one window each, and their tab bars.
            seen(terminal, rect(0.0, 69.0, 1470.0, 887.0), false),
            seen(terminal, rect(0.0, 33.0, 1470.0, 68.0), false),
            seen(terminal, rect(0.0, 69.0, 1470.0, 887.0), false),
            seen(terminal, rect(0.0, 69.0, 1470.0, 887.0), false),
            // A hidden app's windows, and a helper that never shows.
            seen(wakatime, rect(535.0, 129.0, 400.0, 478.0), false),
            seen(wakatime, rect(0.0, 456.0, 500.0, 500.0), false),
        ];
        assert_eq!(listed(&windows, &[LAPTOP], true), [(0, false), (2, true), (3, true)]);

        // On Terminal's Space: its front tab shows, with its tab bar, and its other tabs aren't listed again.
        let windows = [
            seen(terminal, rect(0.0, 33.0, 1470.0, 68.0), true),
            seen(terminal, rect(0.0, 69.0, 1470.0, 887.0), true),
            seen(terminal, rect(0.0, 69.0, 1470.0, 887.0), false),
            seen(chrome, rect(0.0, 121.0, 1470.0, 835.0), false),
            seen(finder, rect(224.0, 175.0, 900.0, 450.0), false),
        ];
        assert_eq!(listed(&windows, &[LAPTOP], true), [(1, true), (3, true)]);
    }

    #[test]
    fn from_a_full_screen_app_the_picker_lists_windows_on_the_desktop() {
        let at =
            |pid: i32, frame: CGRect, on_screen: bool, space: Space| Seen { pid, frame, on_screen, space };
        let (terminal, chrome, whatsapp, motion, settings, wakatime) = (1, 2, 3, 4, 5, 6);
        // As read on the owner's Mac from Terminal's full-screen Space, with Stage Manager on the desktop.
        let windows = [
            at(terminal, rect(0.0, 33.0, 1470.0, 68.0), true, Space::Here),
            at(terminal, rect(0.0, 69.0, 1470.0, 887.0), true, Space::Here),
            // Its other tabs and their tab bars, ordered out.
            at(terminal, rect(0.0, 69.0, 1470.0, 887.0), false, Space::Nowhere),
            at(terminal, rect(0.0, 33.0, 1470.0, 68.0), false, Space::Nowhere),
            // A window of Terminal's on the desktop: Terminal is here, so macOS wouldn't go to it.
            at(terminal, rect(16.0, 665.0, 108.0, 117.0), false, Space::OtherDesktop),
            // Chrome full screen on its own Space, with its toolbar and a small window there.
            at(chrome, rect(0.0, 33.0, 1470.0, 124.0), false, Space::OtherFullScreen(646)),
            at(chrome, rect(0.0, 121.0, 1470.0, 835.0), false, Space::OtherFullScreen(646)),
            at(chrome, rect(135.0, 59.0, 858.0, 458.0), false, Space::OtherFullScreen(646)),
            // Windows parked in Stage Manager's strip on the desktop.
            at(whatsapp, rect(16.0, 519.0, 137.0, 137.0), false, Space::OtherDesktop),
            at(motion, rect(16.0, 346.0, 137.0, 150.0), false, Space::OtherDesktop),
            at(settings, rect(16.0, 207.0, 86.0, 122.0), false, Space::OtherDesktop),
            // A hidden app's window.
            at(wakatime, rect(515.0, 124.0, 446.0, 673.0), false, Space::Nowhere),
        ];
        // Asking Terminal to come forward would stay here, so its window on the desktop is listed only when one
        // window can be brought forward by itself.
        assert_eq!(
            listed(&windows, &[LAPTOP], false),
            [(1, true), (6, true), (8, false), (9, false), (10, false)]
        );
        assert_eq!(
            listed(&windows, &[LAPTOP], true),
            [(1, true), (4, false), (6, true), (8, false), (9, false), (10, false)]
        );
    }

    #[test]
    fn two_full_screen_windows_of_one_app_are_listed_apart() {
        let at =
            |pid: i32, frame: CGRect, on_screen: bool, space: Space| Seen { pid, frame, on_screen, space };
        let (chrome, terminal, finder) = (1, 2, 3);
        let full = rect(0.0, 121.0, 1470.0, 835.0);
        let toolbar = rect(0.0, 33.0, 1470.0, 124.0);
        // As on the owner's Mac from the desktop: Chrome full screen on two Spaces (“Google” and “google - Google
        // Search”), each with its toolbar, and Terminal on a third.
        let windows = [
            at(finder, rect(224.0, 175.0, 900.0, 450.0), true, Space::Here),
            at(chrome, toolbar, false, Space::OtherFullScreen(646)),
            at(chrome, full, false, Space::OtherFullScreen(646)),
            at(chrome, toolbar, false, Space::OtherFullScreen(702)),
            at(chrome, full, false, Space::OtherFullScreen(702)),
            at(terminal, rect(0.0, 69.0, 1470.0, 887.0), false, Space::OtherFullScreen(523)),
        ];
        assert_eq!(listed(&windows, &[LAPTOP], true), [(0, false), (2, true), (4, true), (5, true)]);
        assert_eq!(listed(&windows, &[LAPTOP], false), [(0, false), (2, true), (4, true), (5, true)]);

        // From one of Chrome's Spaces, the other is listed when that one window can be brought forward by itself.
        let windows = [
            at(chrome, toolbar, true, Space::Here),
            at(chrome, full, true, Space::Here),
            at(chrome, full, false, Space::OtherFullScreen(702)),
            at(finder, rect(224.0, 175.0, 900.0, 450.0), false, Space::OtherDesktop),
        ];
        assert_eq!(listed(&windows, &[LAPTOP], true), [(1, true), (2, true), (3, false)]);
        assert_eq!(listed(&windows, &[LAPTOP], false), [(1, true), (3, false)]);
    }

    #[test]
    fn a_stand_in_is_a_tab_of_the_same_window() {
        // A tab behind another is on no Space; the one in front, on the window's.
        assert!(same_window(Some(&[]), Some(&[646])));
        assert!(same_window(Some(&[646]), Some(&[646])));
        // Two full-screen windows of one app are on Spaces of their own.
        assert!(!same_window(Some(&[702]), Some(&[646])));
        // Without the window server's word, the frame decides.
        assert!(same_window(None, Some(&[646])));
        assert!(same_window(Some(&[702]), None));
    }
}
