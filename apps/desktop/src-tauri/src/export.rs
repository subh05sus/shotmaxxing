//! Exporting projects to video files on a background thread, with progress for the editor and instant cancelling.
//! Exports wait in line and render one at a time, so two never share the encoder, the GPU and the heat.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shotmaxxing_core::bundle::sanitize_file_name;
use shotmaxxing_render::{ExportOptions, Format, RenderError, unique_path};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime, State, WebviewWindow};

use crate::projects::pick_folder;
use crate::recording::{expand_home, projects_folder};

/// Exports in progress, by the id the editor gave them.
#[derive(Default)]
pub struct Exports {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Exports in line, by id; the first one renders.
    line: Mutex<VecDeque<String>>,
    moved: Condvar,
}

/// An export's turn to render. The next in line goes when it ends.
struct Turn<'a> {
    exports: &'a Exports,
    id: String,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        if let Ok(mut line) = self.exports.line.lock() {
            line.retain(|id| *id != self.id);
        }
        self.exports.moved.notify_all();
    }
}

impl Exports {
    /// Whether an export is rendering or waiting its turn.
    pub fn busy(&self) -> bool {
        self.jobs.lock().is_ok_and(|jobs| !jobs.is_empty())
    }

    /// Joins the line and waits to be first. `waiting` is called once if there's a wait. `None` when the export is
    /// cancelled while it waits.
    fn wait_turn(&self, id: &str, cancel: &AtomicBool, waiting: impl FnOnce()) -> Option<Turn<'_>> {
        let mut line = self.line.lock().ok()?;
        line.push_back(id.to_owned());
        let turn = Turn { exports: self, id: id.to_owned() };
        let mut waiting = Some(waiting);
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(line);
                return None;
            }
            if line.front().is_some_and(|first| first == id) {
                return Some(turn);
            }
            if let Some(waiting) = waiting.take() {
                waiting();
            }
            line = self.moved.wait_timeout(line, Duration::from_millis(100)).ok()?.0;
        }
    }

    /// Asks every running export to stop, and waits a moment for them to clean up their partial files.
    pub fn cancel_all(&self) {
        let Ok(jobs) = self.jobs.lock() else { return };
        if jobs.is_empty() {
            return;
        }
        for flag in jobs.values() {
            flag.store(true, Ordering::Relaxed);
        }
        drop(jobs);
        let deadline = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < deadline && self.jobs.lock().is_ok_and(|jobs| !jobs.is_empty()) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub id: String,
    /// The `.smx` project folder.
    pub project: String,
    /// The file name, without the extension.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: Format,
    /// Bits per second.
    pub bitrate: f64,
    /// Where to save; next to the project when empty.
    #[serde(default)]
    pub folder: Option<String>,
    /// For a PNG: the moment of the recording to show, in seconds.
    #[serde(default)]
    pub at: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    id: &'a str,
    progress: f64,
    /// Waiting for an earlier export to finish.
    waiting: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ExportOutcome {
    Done { path: String, file: String },
    Cancelled,
}

#[tauri::command]
pub async fn export_project<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    exports: State<'_, Exports>,
    request: ExportRequest,
) -> Result<ExportOutcome, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    exports
        .jobs
        .lock()
        .map_err(|_| "the export list is unavailable".to_string())?
        .insert(request.id.clone(), Arc::clone(&cancel));
    let id = request.id.clone();
    let label = window.label().to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let exports = app.state::<Exports>();
        let target = EventTarget::webview_window(&label);
        let waiting = || {
            let _ = app.emit_to(
                target,
                "export-progress",
                Progress { id: &request.id, progress: 0.0, waiting: true },
            );
        };
        let Some(_turn) = exports.wait_turn(&request.id, &cancel, waiting) else {
            return Ok(ExportOutcome::Cancelled);
        };
        run(&app, &label, &request, &cancel)
    })
    .await;
    if let Ok(mut jobs) = exports.jobs.lock() {
        jobs.remove(&id);
    }
    result.map_err(|e| e.to_string())?
}

fn run<R: Runtime>(
    app: &AppHandle<R>,
    window: &str,
    request: &ExportRequest,
    cancel: &AtomicBool,
) -> Result<ExportOutcome, String> {
    let project = Path::new(&request.project);
    let home = app.path().home_dir().ok();
    let folder = match request.folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(folder) => expand_home(folder, home.as_deref()),
        None => project.parent().map(Path::to_path_buf).ok_or("this project isn't in a folder")?,
    };
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("couldn't make the folder {}: {e}", folder.display()))?;
    let output = unique_path(&folder, &sanitize_file_name(&request.name), request.format.extension());
    let options = ExportOptions {
        width: request.width,
        height: request.height,
        fps: request.fps,
        format: request.format,
        bitrate: request.bitrate,
        output: output.clone(),
        at: request.at,
    };
    let target = EventTarget::webview_window(window);
    let mut last: Option<Instant> = None;
    let result = shotmaxxing_render::export(project, &options, &mut |progress| {
        // About eight updates a second is smooth enough for a progress ring.
        if progress >= 1.0 || last.is_none_or(|at| at.elapsed() >= Duration::from_millis(120)) {
            last = Some(Instant::now());
            let _ = app.emit_to(
                target.clone(),
                "export-progress",
                Progress { id: &request.id, progress, waiting: false },
            );
        }
        !cancel.load(Ordering::Relaxed)
    });
    match result {
        Ok(()) => Ok(ExportOutcome::Done {
            path: output.to_string_lossy().into_owned(),
            file: output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }),
        Err(RenderError::Cancelled) => Ok(ExportOutcome::Cancelled),
        Err(error) => {
            tracing::warn!(%error, "export failed");
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub fn cancel_export(exports: State<'_, Exports>, id: String) {
    if let Ok(jobs) = exports.jobs.lock()
        && let Some(flag) = jobs.get(&id)
    {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Asks where exports go. Starts in the current choice, or the projects folder.
#[tauri::command]
pub async fn choose_export_folder<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    current: Option<String>,
) -> Result<Option<String>, String> {
    let start = match current.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(current) => expand_home(current, app.path().home_dir().ok().as_deref()),
        None => projects_folder(&app, None)?,
    };
    pick_folder(&app, &window, "Save exports to", &start).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_render_one_at_a_time_in_order() {
        let exports = Arc::new(Exports::default());
        let never = AtomicBool::new(false);
        let first = exports.wait_turn("a", &never, || panic!("the first export never waits")).unwrap();
        let (started, second_started) = std::sync::mpsc::channel();
        let line = Arc::clone(&exports);
        let second = std::thread::spawn(move || {
            let never = AtomicBool::new(false);
            let waited = std::cell::Cell::new(false);
            let turn = line.wait_turn("b", &never, || waited.set(true));
            started.send(()).unwrap();
            (turn.is_some(), waited.get())
        });
        assert!(second_started.recv_timeout(Duration::from_millis(300)).is_err(), "b waits while a renders");
        drop(first);
        second_started.recv_timeout(Duration::from_secs(2)).expect("b goes once a is done");
        assert_eq!(second.join().unwrap(), (true, true));
    }

    #[test]
    fn a_waiting_export_can_be_cancelled() {
        let exports = Exports::default();
        let never = AtomicBool::new(false);
        let _first = exports.wait_turn("a", &never, || {}).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(exports.wait_turn("b", &cancelled, || {}).is_none());
        assert_eq!(exports.line.lock().unwrap().len(), 1, "b left the line");
    }
}
