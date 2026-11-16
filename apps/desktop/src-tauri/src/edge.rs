//! The red edge round every screen while an agent's take moves the pointer and types, so nobody mistakes whose
//! hands are on the computer. It never takes a click, stays out of recordings and screenshots, and draws nothing
//! else.
//!
//! It's four thin windows along each screen's sides, drawn by the system with no web view. One window over the whole
//! screen would answer the check a take makes before each click ("does another window cover this button?"), and the
//! take would stop; a strip a few points wide never covers the middle of a control.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Runtime};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    pub fn show() {}
    pub fn hide() {}
}

/// How thick the edge is, in points.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
const THICKNESS: f64 = 3.0;

/// Whether the edge is up, or on its way up.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// Shows the edge, or takes it away; nothing happens when it's already that way. Its windows are made and closed on
/// the main thread, whose message loop serves them, so the change happens there.
pub fn set<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if SHOWN.swap(on, Ordering::SeqCst) == on {
        return;
    }
    if let Err(error) = app.run_on_main_thread(move || if on { imp::show() } else { imp::hide() }) {
        tracing::warn!(%error, "couldn't change the red edge round the screen");
    }
}
