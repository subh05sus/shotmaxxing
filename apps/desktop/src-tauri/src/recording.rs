//! Recording from the app. One recording runs at a time, on a thread of its own: capture objects never cross threads,
//! and the UI never waits on the encoder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use shotmaxxing_capture::{DeviceChoice, FinishedProject, ProjectOptions, ProjectRecording, Target};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{tray, windows};

enum Finish {
    Keep,
    Discard,
}

/// Choices from Settings that shape a recording.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartOptions {
    /// The most frames per second: 30, 60 or 120.
    fps: Option<u32>,
    hide_desktop_icons: bool,
    /// The folder chosen in Settings. Empty or missing means the default folder.
    folder: Option<String>,
    /// Record what the computer plays.
    system_audio: bool,
    /// A microphone id, `default` for the system's choice, or nothing for no microphone.
    microphone: Option<String>,
    /// A camera id, `default`, or nothing for no camera.
    camera: Option<String>,
}

fn device_choice(choice: Option<&str>) -> DeviceChoice {
    match choice.map(str::trim) {
        None | Some("") => DeviceChoice::Off,
        Some("default") => DeviceChoice::Default,
        Some(id) => DeviceChoice::Id(id.to_string()),
    }
}

/// What a finished recording wants to tell the person (a microphone that wasn't allowed, dropped frames), kept
/// until the editor for that project opens and asks.
#[derive(Default)]
pub struct Notices(Mutex<HashMap<String, Vec<String>>>);

impl Notices {
    fn remember(&self, finished: &FinishedProject) {
        let mut notes = finished.warnings.clone();
        if finished.dropped > 0 {
            notes.push(format!("{} frames were dropped while the computer was busy.", finished.dropped));
        }
        if !notes.is_empty()
            && let Ok(mut map) = self.0.lock()
        {
            map.insert(finished.root.to_string_lossy().into_owned(), notes);
        }
    }
}

/// Hands over, once, what the recording of `project` wanted to say.
#[tauri::command]
pub fn take_recording_notices(state: State<'_, Notices>, project: String) -> Vec<String> {
    state.0.lock().ok().and_then(|mut map| map.remove(&project)).unwrap_or_default()
}

struct Session {
    source: String,
    name: String,
    options: StartOptions,
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

/// Where projects go: the folder chosen in Settings, or Movies/Shotmaxxing (Videos/Shotmaxxing on Windows).
pub fn projects_folder<R: Runtime>(app: &AppHandle<R>, chosen: Option<&str>) -> Result<PathBuf, String> {
    match chosen.map(str::trim).filter(|c| !c.is_empty()) {
        Some(chosen) => Ok(expand_home(chosen, app.path().home_dir().ok().as_deref())),
        None => app.path().video_dir().map(|dir| dir.join("Shotmaxxing")).map_err(|e| e.to_string()),
    }
}

/// `~/Movies` becomes a full path. Settings stores folders the way it shows them.
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    let rest = path.strip_prefix('~').filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\']));
    match (rest, home) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(path),
    }
}

/// A full path shown the short way, starting with `~` inside the home folder.
pub fn tildify(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
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

fn start_session(
    folder: PathBuf,
    source: String,
    name: String,
    options: StartOptions,
) -> Result<(Session, PathBuf), String> {
    let target = parse_source(&source)?;
    let mut capture = ProjectOptions::new(target);
    capture.max_fps = options.fps.unwrap_or(60).clamp(15, 120);
    capture.hide_desktop_icons = options.hide_desktop_icons;
    capture.system_audio = options.system_audio;
    capture.microphone = device_choice(options.microphone.as_deref());
    capture.camera = device_choice(options.camera.as_deref());
    let (ready_tx, ready_rx) = sync_channel::<Result<PathBuf, String>>(1);
    let (finish_tx, finish_rx) = channel::<Finish>();
    let (done_tx, done_rx) = sync_channel::<Result<Option<FinishedProject>, String>>(1);
    let thread_name = name.clone();
    std::thread::Builder::new()
        .name("smx-recording".into())
        .spawn(move || {
            let recording = match ProjectRecording::start(&folder, &thread_name, capture) {
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
    // The first recording with a microphone or camera waits while the system asks the person, so allow for that.
    let root = match ready_rx.recv_timeout(Duration::from_secs(150)) {
        Ok(root) => root?,
        Err(_) => {
            // If it starts after all, it stops straight away and leaves nothing behind.
            let _ = finish_tx.send(Finish::Discard);
            return Err("The recording didn't start in time. Try again.".to_string());
        }
    };
    Ok((Session { source, name, options, finish: finish_tx, done: done_rx }, root))
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
    options: Option<StartOptions>,
) -> Result<Started, String> {
    if state.0.lock().map_err(|_| "The recorder is busy.".to_string())?.is_some() {
        return Err("A recording is already running. Stop it first.".into());
    }
    let options = options.unwrap_or_default();
    let folder = projects_folder(&app, options.folder.as_deref())?;
    let name =
        name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Recording".into());
    let (session, root) =
        tauri::async_runtime::spawn_blocking(move || start_session(folder, source, name, options))
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
    app.state::<Notices>().remember(&finished);
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
    let folder = projects_folder(&app, session.options.folder.as_deref())?;
    let (session, root) = tauri::async_runtime::spawn_blocking(move || {
        let (source, name, options) = (session.source.clone(), session.name.clone(), session.options.clone());
        session.end(Finish::Discard)?;
        start_session(folder, source, name, options)
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
                app.state::<Notices>().remember(&finished);
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
    use std::path::{Path, PathBuf};

    use super::{Target, encode_component, expand_home, parse_source, tildify};

    #[test]
    fn folders_round_trip_through_the_short_form() {
        let home = Path::new("/Users/me");
        assert_eq!(
            expand_home("~/Movies/Shotmaxxing", Some(home)),
            PathBuf::from("/Users/me/Movies/Shotmaxxing")
        );
        assert_eq!(expand_home("~", Some(home)), PathBuf::from("/Users/me/"));
        assert_eq!(expand_home("/Volumes/Work", Some(home)), PathBuf::from("/Volumes/Work"));
        assert_eq!(expand_home("~other/x", Some(home)), PathBuf::from("~other/x"));
        assert_eq!(tildify(Path::new("/Users/me/Movies/Shotmaxxing"), Some(home)), "~/Movies/Shotmaxxing");
        assert_eq!(tildify(Path::new("/Volumes/Work"), Some(home)), "/Volumes/Work");
    }

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
