//! macOS: TCC permissions (screen recording, microphone, camera, accessibility, input monitoring) and power
//! (thermal state and Low Power Mode from NSProcessInfo).

use std::ffi::{OsStr, OsString, c_void};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
};
use objc2_foundation::{NSDictionary, NSNumber, NSProcessInfo, NSProcessInfoThermalState, NSString};
use shotmaxxing_core::power::ThermalState;

use super::PowerReading;
use crate::commands::{PermissionKind, PermissionState};

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    // Both are plain status queries/requests with no arguments; they are safe to call from any thread.
    safe fn CGPreflightScreenCaptureAccess() -> bool;
    safe fn CGRequestScreenCaptureAccess() -> bool;
    safe fn CGPreflightListenEventAccess() -> bool;
    safe fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

fn media_type(kind: PermissionKind) -> Option<&'static AVMediaType> {
    // SAFETY: these are immutable framework constants, initialised before any code runs.
    unsafe {
        match kind {
            PermissionKind::Microphone => AVMediaTypeAudio,
            PermissionKind::Camera => AVMediaTypeVideo,
            _ => None,
        }
    }
}

fn capture_device_state(media: &AVMediaType) -> PermissionState {
    // SAFETY: a class method that only reads the current authorisation for a valid media type.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    match status {
        AVAuthorizationStatus::Authorized => PermissionState::Granted,
        AVAuthorizationStatus::NotDetermined => PermissionState::NotDetermined,
        _ => PermissionState::Denied,
    }
}

/// The argument a copy of the app is started with to read its own Screen Recording permission and quit.
const PROBE: &str = "--permission-probe";
/// How long a fresh copy's answer is used again. A page that checks every second starts a probe every other time.
const PROBE_FRESH: Duration = Duration::from_millis(1500);
/// A probe that hasn't answered by then is given up on.
const PROBE_LIMIT: Duration = Duration::from_secs(5);

/// The last probe: when it ended and what it heard (None when it couldn't tell). Held while a probe runs, so
/// checks at the same moment share one.
static LAST_PROBE: Mutex<Option<(Instant, Option<bool>)>> = Mutex::new(None);

/// The executable file this process started from (device and inode), so a probe never starts a different copy
/// that was put in its place since, such as an update installed while the app runs.
static EXECUTABLE: OnceLock<Option<(u64, u64)>> = OnceLock::new();

/// In a copy started with `PROBE`: prints whether this app may record the screen ("1" or "0") and returns true,
/// before anything else starts. False for a normal launch.
pub fn answer_probe() -> bool {
    if !is_probe(std::env::args_os()) {
        EXECUTABLE.get_or_init(executable_identity);
        return false;
    }
    println!("{}", u8::from(CGPreflightScreenCaptureAccess()));
    true
}

fn executable_identity() -> Option<(u64, u64)> {
    let meta = std::fs::metadata(std::env::current_exe().ok()?).ok()?;
    Some((meta.dev(), meta.ino()))
}

/// Whether the app was started as a probe: `PROBE` and nothing else, so no launch by LaunchServices or by a person
/// is taken for one.
fn is_probe(args: impl Iterator<Item = OsString>) -> bool {
    let args: Vec<OsString> = args.skip(1).collect();
    args.len() == 1 && args[0] == OsStr::new(PROBE)
}

/// Whether macOS lets this app record the screen now, as a fresh process of it hears. macOS answers each process
/// once, so after the person allows Screen Recording in System Settings the running app goes on hearing no until it
/// reopens. A new copy started with `PROBE` hears the current answer. Blocks while one runs (a tenth of a second or
/// so), so it's called off the main thread.
fn fresh_screen_access() -> Option<bool> {
    let mut last = LAST_PROBE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, heard)) = *last
        && at.elapsed() < PROBE_FRESH
    {
        return heard;
    }
    let heard = run_probe();
    *last = Some((Instant::now(), heard));
    heard
}

/// Starts this app's own executable with `PROBE` and reads its answer. The copy is the same code, so macOS judges it
/// as this app; it quits before any window, menu-bar icon or shortcut exists.
fn run_probe() -> Option<bool> {
    let exe = std::env::current_exe().ok()?;
    let started_from = *EXECUTABLE.get_or_init(executable_identity);
    if started_from.is_none() || executable_identity() != started_from {
        return None;
    }
    let mut child = Command::new(exe)
        .arg(PROBE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if started.elapsed() > PROBE_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let mut answer = String::new();
    child.stdout.take()?.read_to_string(&mut answer).ok()?;
    match answer.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Screen Recording as the setup shows it: what this process hears, and what a fresh copy hears when this one hears
/// no (asked only then).
fn screen_state(here: bool, fresh: impl FnOnce() -> Option<bool>) -> PermissionState {
    if here {
        return PermissionState::Granted;
    }
    match fresh() {
        Some(true) => PermissionState::NeedsRestart,
        _ => PermissionState::NotDetermined,
    }
}

/// Starts a helper that opens the app again, with `args`, once this process has ended, so the new copy never meets
/// the old one's menu-bar icon, shortcuts or sockets. False when the app isn't running from an .app bundle (a
/// development build), where the caller restarts it another way.
pub fn reopen_after_exit(args: &[&str]) -> bool {
    let Some(bundle) = std::env::current_exe().ok().and_then(|exe| app_bundle(&exe)) else {
        return false;
    };
    // Waits up to 30 seconds for this process to end, then opens the app through LaunchServices.
    let script = r#"pid=$1; app=$2; shift 2
i=0; while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 300 ]; do sleep 0.1; i=$((i+1)); done
if [ "$#" -gt 0 ]; then exec /usr/bin/open "$app" --args "$@"; else exec /usr/bin/open "$app"; fi"#;
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(std::process::id().to_string())
        .arg(&bundle)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .is_ok()
}

/// The .app bundle an executable at `…/Name.app/Contents/MacOS/exe` belongs to.
fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let named = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents";
    (named && bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

pub fn permission(kind: PermissionKind) -> PermissionState {
    let granted = |yes: bool| if yes { PermissionState::Granted } else { PermissionState::NotDetermined };
    match kind {
        PermissionKind::Screen => screen_state(CGPreflightScreenCaptureAccess(), fresh_screen_access),
        PermissionKind::Accessibility => granted(AXIsProcessTrusted()),
        PermissionKind::InputMonitoring => granted(CGPreflightListenEventAccess()),
        PermissionKind::Microphone | PermissionKind::Camera => {
            media_type(kind).map_or(PermissionState::NotNeeded, capture_device_state)
        }
    }
}

pub fn request_permission(kind: PermissionKind) -> bool {
    match kind {
        PermissionKind::Screen => {
            // Shows the system prompt the first time and adds the app to the list in System Settings. It returns
            // false until the app restarts, so settings open too.
            CGRequestScreenCaptureAccess();
            false
        }
        PermissionKind::InputMonitoring => {
            CGRequestListenEventAccess();
            false
        }
        PermissionKind::Accessibility => {
            let key = NSString::from_str("AXTrustedCheckOptionPrompt");
            let value = NSNumber::new_bool(true);
            let options = NSDictionary::from_slices(&[&*key], &[&*value]);
            // SAFETY: NSDictionary is toll-free bridged to CFDictionary, which is what the function expects, and
            // `options` outlives the call.
            unsafe { AXIsProcessTrustedWithOptions(objc2::rc::Retained::as_ptr(&options).cast()) };
            // That adds the app to the list in System Settings, but macOS shows its prompt only the first time, so
            // the settings page opens too: Allow always shows somewhere to turn it on.
            false
        }
        PermissionKind::Microphone | PermissionKind::Camera => {
            let Some(media) = media_type(kind) else { return false };
            if capture_device_state(media) != PermissionState::NotDetermined {
                return false;
            }
            let handler = RcBlock::new(|_granted: Bool| {});
            // SAFETY: valid media type and a block that outlives the call (the framework copies it).
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler) };
            true
        }
    }
}

pub fn settings_url(kind: PermissionKind) -> Option<&'static str> {
    Some(match kind {
        PermissionKind::Screen => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        PermissionKind::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        PermissionKind::Camera => "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera",
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        PermissionKind::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
    })
}

pub fn power() -> PowerReading {
    let info = NSProcessInfo::processInfo();
    let thermal = match info.thermalState() {
        NSProcessInfoThermalState::Fair => ThermalState::Fair,
        NSProcessInfoThermalState::Serious => ThermalState::Serious,
        NSProcessInfoThermalState::Critical => ThermalState::Critical,
        _ => ThermalState::Nominal,
    };
    PowerReading {
        thermal,
        on_battery: false,
        battery_percent: None,
        os_low_power: info.isLowPowerModeEnabled(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_recording_this_process_hears_is_enough() {
        let state = screen_state(true, || panic!("no fresh copy is asked when this process may record"));
        assert_eq!(state, PermissionState::Granted);
    }

    #[test]
    fn screen_recording_only_a_fresh_copy_hears_needs_a_reopen() {
        assert_eq!(screen_state(false, || Some(true)), PermissionState::NeedsRestart);
        assert_eq!(screen_state(false, || Some(false)), PermissionState::NotDetermined);
        // A probe that couldn't run (or hasn't answered yet) changes nothing.
        assert_eq!(screen_state(false, || None), PermissionState::NotDetermined);
    }

    #[test]
    fn only_the_probe_argument_alone_makes_a_probe() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>().into_iter();
        assert!(is_probe(args(&["shotmaxxing-desktop", PROBE])));
        assert!(!is_probe(args(&["shotmaxxing-desktop"])));
        assert!(!is_probe(args(&["shotmaxxing-desktop", "--other"])));
        assert!(!is_probe(args(&["shotmaxxing-desktop", PROBE, "--other"])));
        assert!(!is_probe(args(&["shotmaxxing-desktop", "-psn_0_12345", PROBE])));
    }

    #[test]
    fn the_bundle_is_found_from_its_executable_only() {
        let exe = Path::new("/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing-desktop");
        assert_eq!(app_bundle(exe), Some(PathBuf::from("/Applications/Shotmaxxing.app")));
        assert_eq!(app_bundle(Path::new("/Users/me/shotmaxxing/target/debug/shotmaxxing-desktop")), None);
        assert_eq!(
            app_bundle(Path::new("/Applications/Shotmaxxing/Contents/MacOS/shotmaxxing-desktop")),
            None
        );
        assert_eq!(app_bundle(Path::new("/x.app/Contents/Resources/shotmaxxing-desktop")), None);
    }
}
