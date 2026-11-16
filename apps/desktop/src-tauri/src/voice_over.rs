//! Voice-overs: narration recorded in the editor while the video plays, saved into the project's media folder. The
//! editor lays each one on the finished video's timeline, from where the playhead was when it started.

use std::sync::Mutex;

use serde::Serialize;
use shotmaxxing_capture::VoiceRecorder;
use shotmaxxing_core::bundle::ProjectBundle;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use crate::recording::Recordings;

/// The voice-over being recorded, if any: one at a time, as there's one microphone to talk into.
#[derive(Default)]
pub struct VoiceOvers {
    active: Mutex<Option<Active>>,
}

struct Active {
    recorder: VoiceRecorder,
    /// The file, relative to the project, as tracks name their files.
    path: String,
    /// The editor window it's for. Closing that window throws the voice-over away.
    window: String,
}

impl VoiceOvers {
    /// Whether a voice-over is being recorded.
    pub fn busy(&self) -> bool {
        self.active.lock().is_ok_and(|active| active.is_some())
    }

    fn take(&self) -> Option<Active> {
        self.active.lock().ok().and_then(|mut active| active.take())
    }

    /// Throws away a voice-over nobody can finish any more: its editor closed, or the app is quitting. `window`
    /// limits it to that editor's.
    pub fn abandon(&self, window: Option<&str>) {
        let Ok(mut active) = self.active.lock() else { return };
        if active.as_ref().is_some_and(|a| window.is_none_or(|w| a.window == w))
            && let Some(active) = active.take()
        {
            tracing::info!(path = %active.path, "discarded a voice-over its editor left behind");
            active.recorder.discard();
        }
    }
}

/// A finished voice-over, for the editor to add to the project.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOverFile {
    /// Relative to the project, as tracks name their files.
    path: String,
    seconds: f64,
}

/// Starts recording a voice-over for `project` from `microphone` (the system's default when empty). Returns once
/// the microphone is sending sound, so the editor can start playing in step with it.
#[tauri::command]
pub async fn start_voice_over<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    project: String,
    microphone: Option<String>,
) -> Result<(), String> {
    if app.state::<Recordings>().is_recording() {
        return Err("Stop the recording first: it's using the microphone.".into());
    }
    let busy = || "A voice-over is already recording.".to_string();
    if app.state::<VoiceOvers>().active.lock().map_err(|_| unavailable())?.is_some() {
        return Err(busy());
    }
    let (bundle, _) = ProjectBundle::open(&project).map_err(|e| e.to_string())?;
    let path = bundle.next_voice_over();
    let output = bundle.root().join(&path);
    let microphone = microphone.filter(|id| !id.trim().is_empty());
    // Starting waits for the microphone's first sound, which can take a moment (and the first time, an answer to
    // the system's question).
    let recorder = tauri::async_runtime::spawn_blocking(move || VoiceRecorder::start(microphone, output))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let voice = app.state::<VoiceOvers>();
    let mut active = voice.active.lock().map_err(|_| unavailable())?;
    if active.is_some() {
        recorder.discard();
        return Err(busy());
    }
    *active = Some(Active { recorder, path, window: window.label().to_string() });
    Ok(())
}

/// Stops the voice-over and finishes its file.
#[tauri::command]
pub async fn stop_voice_over<R: Runtime>(app: AppHandle<R>) -> Result<VoiceOverFile, String> {
    let active = app.state::<VoiceOvers>().take().ok_or("No voice-over is recording.")?;
    let path = active.path;
    let take = tauri::async_runtime::spawn_blocking(move || active.recorder.stop())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(VoiceOverFile { path, seconds: take.duration_us as f64 / 1e6 })
}

/// Stops the voice-over and throws it away.
#[tauri::command]
pub async fn discard_voice_over<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(active) = app.state::<VoiceOvers>().take() {
        tauri::async_runtime::spawn_blocking(move || active.recorder.discard())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn unavailable() -> String {
    "the voice-over recorder is unavailable".into()
}
