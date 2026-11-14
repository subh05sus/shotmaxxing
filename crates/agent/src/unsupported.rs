//! Platforms without a performer yet.

use shotmaxxing_core::take::TakeScript;

use crate::{AgentError, Prepared, Progress, Take, TakeOptions};

pub fn prepare(_app: Option<&str>, _window: Option<&str>) -> Result<Prepared, AgentError> {
    Err(AgentError::Unsupported)
}

pub fn run_take(
    _script: &TakeScript,
    _options: TakeOptions,
    _progress: &mut dyn FnMut(Progress),
) -> Result<Take, AgentError> {
    Err(AgentError::Unsupported)
}
