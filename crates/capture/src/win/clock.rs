//! The system's performance counter, in seconds: the clock capture stamps frames with, so the telemetry can use it
//! too.

use std::sync::OnceLock;

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub fn now() -> f64 {
    static FREQUENCY: OnceLock<f64> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut frequency = 0i64;
        // SAFETY: writes one integer.
        let _ = unsafe { QueryPerformanceFrequency(&mut frequency) };
        frequency.max(1) as f64
    });
    let mut count = 0i64;
    // SAFETY: writes one integer.
    let _ = unsafe { QueryPerformanceCounter(&mut count) };
    count as f64 / frequency
}
