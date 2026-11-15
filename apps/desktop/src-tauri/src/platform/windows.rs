//! Windows: screen capture needs no permission (Windows Graphics Capture shows its own border instead), and the
//! microphone and camera have privacy switches in Settings, read here from where Windows keeps them. Power comes
//! from GetSystemPowerStatus.

use shotmaxxing_core::power::ThermalState;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::{PCWSTR, w};

use super::PowerReading;
use crate::commands::{PermissionKind, PermissionState};

pub fn permission(kind: PermissionKind) -> PermissionState {
    match kind {
        PermissionKind::Screen | PermissionKind::Accessibility | PermissionKind::InputMonitoring => {
            PermissionState::NotNeeded
        }
        PermissionKind::Microphone => device_state("microphone"),
        PermissionKind::Camera => device_state("webcam"),
    }
}

fn device_state(capability: &str) -> PermissionState {
    if device_allowed(capability) { PermissionState::Granted } else { PermissionState::Denied }
}

/// Whether Windows' privacy switches let desktop apps use a device (`microphone` or `webcam`): the switch for the
/// whole computer, the one for this person and the one for desktop apps. Any of them off is a refusal; a switch
/// nobody has touched is on.
fn device_allowed(capability: &str) -> bool {
    const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
    let base = format!(r"{STORE}\{capability}");
    let desktop = format!(r"{base}\NonPackaged");
    [(HKEY_LOCAL_MACHINE, &base), (HKEY_CURRENT_USER, &base), (HKEY_CURRENT_USER, &desktop)]
        .into_iter()
        .all(|(root, key)| consent(root, key).as_deref() != Some("Deny"))
}

/// One switch: `Allow`, `Deny`, or `None` when it was never set.
fn consent(root: HKEY, key: &str) -> Option<String> {
    let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
    let mut buffer = [0u16; 32];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: reads a short string value into `buffer`, whose size in bytes is given.
    let read = unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            w!("Value"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
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
