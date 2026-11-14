//! macOS: TCC permissions (screen recording, microphone, camera, accessibility, input monitoring) and power
//! (thermal state and Low Power Mode from NSProcessInfo).

use std::ffi::c_void;

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

pub fn permission(kind: PermissionKind) -> PermissionState {
    let granted = |yes: bool| if yes { PermissionState::Granted } else { PermissionState::NotDetermined };
    match kind {
        PermissionKind::Screen => granted(CGPreflightScreenCaptureAccess()),
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
            true
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
