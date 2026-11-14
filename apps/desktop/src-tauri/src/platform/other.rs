//! Platforms without native support yet (Linux builds compile, so contributors can work on the shared code).

use shotmaxxing_core::power::ThermalState;

use super::PowerReading;
use crate::commands::{PermissionKind, PermissionState};

pub fn permission(_kind: PermissionKind) -> PermissionState {
    PermissionState::NotNeeded
}

pub fn request_permission(_kind: PermissionKind) -> bool {
    false
}

pub fn settings_url(_kind: PermissionKind) -> Option<&'static str> {
    None
}

pub fn power() -> PowerReading {
    PowerReading {
        thermal: ThermalState::Nominal,
        on_battery: false,
        battery_percent: None,
        os_low_power: false,
    }
}
