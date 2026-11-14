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

/// Triggers the system's own prompt where there is one. Returns true when a prompt was shown, so there is no need
/// to open settings as well.
pub fn request_permission(kind: PermissionKind) -> bool {
    imp::request_permission(kind)
}

pub fn settings_url(kind: PermissionKind) -> Option<&'static str> {
    imp::settings_url(kind)
}

pub fn power() -> PowerReading {
    imp::power()
}
