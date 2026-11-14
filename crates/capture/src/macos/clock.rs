//! The host clock. ScreenCaptureKit stamps frames with host time, so telemetry uses the same clock and the two line
//! up without any guessing.

use std::sync::OnceLock;

#[repr(C)]
#[derive(Default)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    safe fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

fn ratio() -> f64 {
    static RATIO: OnceLock<f64> = OnceLock::new();
    *RATIO.get_or_init(|| {
        let mut info = MachTimebaseInfo::default();
        // SAFETY: writes two integers into the struct we own.
        let ok = unsafe { mach_timebase_info(&mut info) } == 0;
        if ok && info.denom != 0 { f64::from(info.numer) / f64::from(info.denom) } else { 1.0 }
    })
}

/// Seconds since boot on the host clock.
pub fn now() -> f64 {
    mach_absolute_time() as f64 * ratio() / 1e9
}
