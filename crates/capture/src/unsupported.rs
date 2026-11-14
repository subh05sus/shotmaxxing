//! Platforms without a capture backend yet.

use crate::{CaptureError, RecordOptions, Recording, Sources};

pub fn sources() -> Result<Sources, CaptureError> {
    Ok(Sources::default())
}

pub fn has_permission() -> bool {
    true
}

pub struct Recorder;

impl Recorder {
    pub fn start(_options: RecordOptions) -> Result<Self, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    pub fn elapsed(&self) -> f64 {
        0.0
    }

    pub fn stop(self) -> Result<Recording, CaptureError> {
        Err(CaptureError::Unsupported)
    }
}
