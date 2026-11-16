//! What the operating system reports: permissions and power. One module per platform, same functions.

use shotmaxxing_core::power::ThermalState;

use crate::commands::{PermissionKind, PermissionState};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod other;
#[cfg(not(any(target_os = "macos", windows)))]
use other as imp;

/// A snapshot of the machine's power situation.
#[derive(Debug, Clone, Copy)]
pub struct PowerReading {
    pub thermal: ThermalState,
    pub on_battery: bool,
    pub battery_percent: Option<u8>,
    pub os_low_power: bool,
}

pub fn permission(kind: PermissionKind) -> PermissionState {
    imp::permission(kind)
}

/// Triggers the system's own prompt where there is one. Returns true when that prompt settles it (Allow or Don't
/// Allow), so there is no need to open settings as well.
pub fn request_permission(kind: PermissionKind) -> bool {
    imp::request_permission(kind)
}

pub fn settings_url(kind: PermissionKind) -> Option<&'static str> {
    imp::settings_url(kind)
}

/// In a copy of the app started only to read its own permissions (macOS answers Screen Recording once per process,
/// so the running app asks a fresh copy): answers and returns true. False for a normal launch.
pub fn answer_permission_probe() -> bool {
    #[cfg(target_os = "macos")]
    return imp::answer_probe();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Arranges for the app to open again, with `args`, once this process ends. False where that isn't possible, so the
/// caller restarts it another way.
pub fn reopen_after_exit(args: &[&str]) -> bool {
    #[cfg(target_os = "macos")]
    return imp::reopen_after_exit(args);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        false
    }
}

pub fn power() -> PowerReading {
    imp::power()
}
