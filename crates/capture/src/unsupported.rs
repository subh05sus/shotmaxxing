//! Platforms without a capture backend yet.

use crate::{CaptureError, Device, RecordOptions, Recording, Screenshot, Sources, Target};

pub fn sources() -> Result<Sources, CaptureError> {
    Ok(Sources::default())
}

pub fn has_permission() -> bool {
    true
}

pub fn prepare() {}

pub fn thumbnails(targets: &[Target], _max_width: u32) -> Result<Vec<Option<Vec<u8>>>, CaptureError> {
    Ok(vec![None; targets.len()])
}

pub fn screenshot(_target: Target) -> Result<Screenshot, CaptureError> {
    Err(CaptureError::Unsupported)
}

pub fn microphones() -> Vec<Device> {
    Vec::new()
}

pub fn cameras() -> Vec<Device> {
    Vec::new()
}

pub struct Voice;

impl Voice {
    pub fn start(_device: Option<String>, _output: &std::path::Path) -> Result<Self, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    pub fn stop(self) -> Result<f64, CaptureError> {
        Err(CaptureError::Unsupported)
    }
}

pub struct Recorder;

impl Recorder {
    pub fn start(_options: RecordOptions) -> Result<Self, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    pub fn elapsed(&self) -> f64 {
        0.0
    }

    pub fn frames(&self) -> u64 {
        0
    }

    pub fn stop(self) -> Result<Recording, CaptureError> {
        Err(CaptureError::Unsupported)
    }
}
