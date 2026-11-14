//! Windows: screen capture needs no permission (Windows Graphics Capture shows its own border instead), and the
//! microphone and camera have per-app switches in Settings. Power comes from GetSystemPowerStatus.

use shotmaxxing_core::power::ThermalState;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use super::PowerReading;
use crate::commands::{PermissionKind, PermissionState};

pub fn permission(kind: PermissionKind) -> PermissionState {
    match kind {
        PermissionKind::Screen | PermissionKind::Accessibility | PermissionKind::InputMonitoring => {
            PermissionState::NotNeeded
        }
        // The app learns about a refusal when it opens the device; until then it counts as allowed.
        PermissionKind::Microphone | PermissionKind::Camera => PermissionState::Granted,
    }
}

pub fn request_permission(_kind: PermissionKind) -> bool {
    false
}

pub fn settings_url(kind: PermissionKind) -> Option<&'static str> {
    match kind {
        PermissionKind::Microphone => Some("ms-settings:privacy-microphone"),
        PermissionKind::Camera => Some("ms-settings:privacy-webcam"),
        _ => None,
    }
}

pub fn power() -> PowerReading {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: the function only writes into the struct we pass, which lives for the whole call.
    let ok = unsafe { GetSystemPowerStatus(&mut status) }.is_ok();
    if !ok {
        return PowerReading {
            thermal: ThermalState::Nominal,
            on_battery: false,
            battery_percent: None,
            os_low_power: false,
        };
    }
    PowerReading {
        // Windows doesn't report thermal pressure to apps.
        thermal: ThermalState::Nominal,
        on_battery: status.ACLineStatus == 0,
        battery_percent: (status.BatteryLifePercent <= 100).then_some(status.BatteryLifePercent),
        // Bit 0 of SystemStatusFlag is battery saver.
        os_low_power: status.SystemStatusFlag & 1 == 1,
    }
}
