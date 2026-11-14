//! Shotmaxxing core: the platform-independent heart of the recorder and editor.
//!
//! Everything here is pure Rust with no platform code, so it is tested the same way on macOS, Windows and Linux.
//! Platform crates (capture, encode, input, accessibility) produce and consume these types.

#![forbid(unsafe_code)]

pub mod beats;
pub mod bundle;
pub mod camera;
pub mod cursor;
pub mod geometry;
pub mod hash;
pub mod keys;
pub mod power;
pub mod project;
pub mod refs;
pub mod take;
pub mod telemetry;
pub mod timeparse;
pub mod zoom;

pub use geometry::{Point, Rect, Size};
