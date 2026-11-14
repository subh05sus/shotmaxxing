//! Recording from the app. One recording runs at a time, on a thread of its own: capture objects never cross threads,
//! and the UI never waits on the encoder.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::time::Duration;

use serde::Serialize;
use shotmaxxing_capture::{FinishedProject, ProjectRecording, Target};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{tray, windows};

enum Finish {
    Keep,
    Discard,
}

struct Session {
    source: String,
    name: String,
    finish: Sender<Finish>,
    done: Receiver<Result<Option<FinishedProject>, String>>,
}

impl Session {
    /// Ends the recording and waits for the file to be finished.
    fn end(self, how: Finish) -> Result<Option<FinishedProject>, String> {
        let _ = self.finish.send(how);
        self.done.recv_timeout(Duration::from_secs(90)).map_err(|_| {
            "Saving the recording took too long. It may still appear in your Library.".to_string()
        })?
    }
}

/// The recording in progress, if any.
#[derive(Default)]
pub struct Recordings(Mutex<Option<Session>>);

impl Recordings {
    pub fn is_recording(&self) -> bool {
        self.0.lock().is_ok_and(|slot| slot.is_some())
    }

    fn take(&self) -> Result<Session, String> {
        self.0
            .lock()
            .map_err(|_| "The recorder is busy.".to_string())?
            .take()
            .ok_or_else(|| "No recording is running.".to_string())
    }

    /// Keeps whatever is being recorded when the app quits.
    pub fn finish_on_quit(&self) {
        if let Ok(mut slot) = self.0.lock()
            && let Some(session) = slot.take()
            && let Err(error) = session.end(Finish::Keep)
        {
            tracing::warn!(%error, "could not finish the recording while quitting");
        }
    }
}

/// Where new projects go: Movies/Shotmaxxing on macOS, Videos/Shotmaxxing on Windows.
pub fn projects_folder<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path().video_dir().map(|dir| dir.join("Shotmaxxing")).map_err(|e| e.to_string())
}

fn parse_source(id: &str) -> Result<Target, String> {
    let number = |rest: &str| rest.parse::<u32>().map_err(|_| format!("“{id}” isn't a screen or a window."));
    if id == "display" {
        Ok(Target::Display(None))
    } else if let Some(rest) = id.strip_prefix("display-") {
        Ok(Target::Display(Some(number(rest)?)))
    } else if let Some(rest) = id.strip_prefix("window-") {
        Ok(Target::Window(number(rest)?))
    } else {
        Err(format!("“{id}” isn't a screen or a window."))
    }
}

fn start_session(folder: PathBuf, source: String, name: String) -> Result<(Session, PathBuf), String> {
    let target = parse_source(&source)?;
    let (ready_tx, ready_rx) = sync_channel::<Result<PathBuf, String>>(1);
    let (finish_tx, finish_rx) = channel::<Finish>();
    let (done_tx, done_rx) = sync_channel::<Result<Option<FinishedProject>, String>>(1);
    let thread_name = name.clone();
    std::thread::Builder::new()
        .name("smx-recording".into())
        .spawn(move || {
            let recording = match ProjectRecording::start(&folder, &thread_name, target, 60, None) {
                Ok(recording) => recording,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };
            let root = recording.root().to_path_buf();
            let _ = ready_tx.send(Ok(root.clone()));
            // If the app goes away without saying, keep what was recorded.
            let how = finish_rx.recv().unwrap_or(Finish::Keep);
            let result = recording.stop().map_err(|e| e.to_string());
            let result = match how {
                Finish::Keep => result.map(Some),
                Finish::Discard => {
                    let _ = std::fs::remove_dir_all(&root);
                    Ok(None)
                }
            };
            let _ = done_tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    let root = ready_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|_| "The recording didn't start in time. Try again.".to_string())??;
    Ok((Session { source, name, finish: finish_tx, done: done_rx }, root))
}

#[derive(Serialize)]
pub struct Started {
    project: String,
}

#[derive(Serialize)]
pub struct Stopped {
    project: String,
    duration_us: u64,
    zooms: usize,
    dropped_frames: u64,
}

/// Starts recording `source` (an id from `list_sources`) into a new project.
#[tauri::command]
pub async fn start_recording<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Recordings>,
    source: String,
    name: Option<String>,
) -> Result<Started, String> {
    if state.0.lock().map_err(|_| "The recorder is busy.".to_string())?.is_some() {
        return Err("A recording is already running. Stop it first.".into());
    }
    let folder = projects_folder(&app)?;
    let name =
        name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Recording".into());
    let (session, root) = tauri::async_runtime::spawn_blocking(move || start_session(folder, source, name))
        .await
        .map_err(|e| e.to_string())??;
    let mut slot = state.0.lock().map_err(|_| "The recorder is busy.".to_string())?;
    *slot = Some(session);
    tray::set_recording(&app, true);
    Ok(Started { project: root.to_string_lossy().into_owned() })
}

/// Stops, saves the project with auto zoom applied, and returns where it is.
#[tauri::command]
pub async fn stop_recording<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Recordings>,
) -> Result<Stopped, String> {
    let session = state.take()?;
    tray::set_recording(&app, false);
    let finished = tauri::async_runtime::spawn_blocking(move || session.end(Finish::Keep))
        .await
        .map_err(|e| e.to_string())??
        .ok_or_else(|| "The recording was discarded.".to_string())?;
    Ok(Stopped {
        project: finished.root.to_string_lossy().into_owned(),
        duration_us: finished.project.duration_us(),
        zooms: finished.project.zooms.len(),
        dropped_frames: finished.dropped,
    })
}

/// Stops and deletes the recording in progress.
#[tauri::command]
pub async fn discard_recording<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Recordings>,
) -> Result<(), String> {
    let session = state.take()?;
    tray::set_recording(&app, false);
    tauri::async_runtime::spawn_blocking(move || session.end(Finish::Discard))
        .await
        .map_err(|e| e.to_string())??;
    Ok(())
}

/// Throws away what was recorded so far and starts again on the same source.
#[tauri::command]
pub async fn restart_recording<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Recordings>,
) -> Result<Started, String> {
    let session = state.take()?;
    let folder = projects_folder(&app)?;
    let (session, root) = tauri::async_runtime::spawn_blocking(move || {
        let (source, name) = (session.source.clone(), session.name.clone());
        session.end(Finish::Discard)?;
        start_session(folder, source, name)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut slot = state.0.lock().map_err(|_| "The recorder is busy.".to_string())?;
    *slot = Some(session);
    Ok(Started { project: root.to_string_lossy().into_owned() })
}

/// Stops from the menu bar or the shortcut: the pill says it is saving, then the editor opens.
pub fn stop_and_edit<R: Runtime>(app: &AppHandle<R>) {
    let Ok(session) = app.state::<Recordings>().take() else { return };
    tray::set_recording(app, false);
    let _ = app.emit_to("pill", "recording-saving", ());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(move || session.end(Finish::Keep)).await;
        close_pill(&app);
        let opened = match result {
            Ok(Ok(Some(finished))) => {
                windows::open_editor(&app, &encode_component(&finished.root.to_string_lossy()))
            }
            Ok(Ok(None)) => Ok(()),
            Ok(Err(error)) => {
                tracing::warn!(%error, "the recording couldn't be saved");
                windows::open_home(&app, Some("home/library"))
            }
            Err(error) => {
                tracing::warn!(%error, "the recording couldn't be saved");
                Ok(())
            }
        };
        if let Err(error) = opened {
            tracing::warn!(%error, "could not open the recording");
        }
    });
}

/// Discards from the menu bar.
pub fn discard_and_close<R: Runtime>(app: &AppHandle<R>) {
    let Ok(session) = app.state::<Recordings>().take() else { return };
    tray::set_recording(app, false);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = tauri::async_runtime::spawn_blocking(move || session.end(Finish::Discard)).await;
        close_pill(&app);
    });
}

fn close_pill<R: Runtime>(app: &AppHandle<R>) {
    if let Some(pill) = app.get_webview_window("pill")
        && let Err(error) = pill.close()
    {
        tracing::warn!(%error, "could not close the recording pill");
    }
}

/// The same encoding as JavaScript's `encodeURIComponent`, so editor routes match whichever side opened them.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Target, encode_component, parse_source};

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode_component("/Users/a b/Rec (1).smx"), "%2FUsers%2Fa%20b%2FRec%20(1).smx");
        assert_eq!(encode_component("Café"), "Caf%C3%A9");
    }

    #[test]
    fn parses_source_ids() {
        assert_eq!(parse_source("display").unwrap(), Target::Display(None));
        assert_eq!(parse_source("display-2").unwrap(), Target::Display(Some(2)));
        assert_eq!(parse_source("window-5793").unwrap(), Target::Window(5793));
        assert!(parse_source("window-x").is_err());
        assert!(parse_source("area").is_err());
    }
}
