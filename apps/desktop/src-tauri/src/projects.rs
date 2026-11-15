//! Opening, saving and listing `.smx` projects for the editor and the Library.

use std::path::{Path, PathBuf};

use serde::Serialize;
use shotmaxxing_capture::{CURSOR_FILE, EVENTS_FILE};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::cursor::{PathSettings, render_path};
use shotmaxxing_core::project::{Project, TrackKind, ZoomDepth, ZoomRegion};
use shotmaxxing_core::telemetry::{CursorSample, InputEvent, decode_cursor, read_events};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};
use tauri::{AppHandle, Manager, Runtime};

use crate::recording::{projects_folder, tildify};

#[derive(Serialize)]
pub struct LoadedProject {
    root: String,
    project: Project,
    /// The screen recording, as a file path for the asset protocol.
    media: Option<String>,
    /// The sound and camera recordings, placed on the screen's timeline.
    tracks: Vec<TrackMedia>,
    cursor: Vec<CursorSample>,
    events: Vec<InputEvent>,
}

#[derive(Serialize)]
pub struct TrackMedia {
    kind: TrackKind,
    path: String,
    offset_us: i64,
    /// The cleaned-up copy of the microphone, once voice cleanup has made one.
    cleaned: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectSummary {
    path: String,
    name: String,
    duration_us: u64,
    created_at_ms: u64,
    updated_at_ms: u64,
    app: Option<String>,
    agent: Option<String>,
    zooms: usize,
    width: f64,
    height: f64,
    media: Option<String>,
    size_bytes: u64,
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn screen_media(root: &Path, project: &Project) -> Option<String> {
    project
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Screen)
        .map(|t| root.join(&t.path))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

fn track_media(root: &Path, project: &Project) -> Vec<TrackMedia> {
    project
        .tracks
        .iter()
        .filter(|t| t.kind != TrackKind::Screen)
        .map(|t| (t, root.join(&t.path)))
        .filter(|(_, path)| path.exists())
        .map(|(t, path)| TrackMedia {
            kind: t.kind,
            path: path.to_string_lossy().into_owned(),
            offset_us: t.offset_us,
            cleaned: (t.kind == TrackKind::Microphone)
                .then(|| root.join(shotmaxxing_render::CLEAN_MIC))
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().into_owned()),
        })
        .collect()
}

/// A stretch of the recording, in seconds.
#[derive(Serialize)]
pub struct Span {
    start: f64,
    end: f64,
}

/// The long pauses in the take's voice where nothing happens on screen: what "Cut silences" takes out.
#[tauri::command]
pub async fn find_pauses(path: String) -> Result<Vec<Span>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        shotmaxxing_render::find_pauses(Path::new(&path))
            .map(|cuts| cuts.into_iter().map(|(start, end)| Span { start, end }).collect())
            .map_err(text)
    })
    .await
    .map_err(text)?
}

/// Makes the cleaned-up copy of the take's voice and returns where it is.
#[tauri::command]
pub async fn clean_voice(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        shotmaxxing_render::clean_voice(Path::new(&path), &mut |_| true)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(text)
    })
    .await
    .map_err(text)?
}

fn read_telemetry(root: &Path) -> (Vec<CursorSample>, Vec<InputEvent>) {
    let cursor = std::fs::read(root.join(CURSOR_FILE))
        .ok()
        .and_then(|bytes| decode_cursor(&bytes).ok())
        .unwrap_or_default();
    let events = std::fs::File::open(root.join(EVENTS_FILE))
        .ok()
        .and_then(|file| read_events(std::io::BufReader::new(file)).ok())
        .unwrap_or_default();
    (cursor, events)
}

/// The drawn cursor path for a project's own cursor settings.
fn drawn_cursor(project: &Project, cursor: &[CursorSample], events: &[InputEvent]) -> Vec<CursorSample> {
    render_path(
        cursor,
        events,
        PathSettings {
            smoothing: project.cursor.smoothing,
            snap_to_clicks: project.cursor.snap_to_clicks,
            loop_to_start: project.cursor.loop_to_start,
        },
    )
}

/// The drawn cursor path for other settings, while the editor tries them out. The exporter draws the same path.
#[tauri::command]
pub async fn cursor_path(
    path: String,
    smoothing: f64,
    snap_to_clicks: bool,
    loop_to_start: bool,
) -> Result<Vec<CursorSample>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, _) = ProjectBundle::open(&path).map_err(text)?;
        let (cursor, events) = read_telemetry(bundle.root());
        Ok(render_path(&cursor, &events, PathSettings { smoothing, snap_to_clicks, loop_to_start }))
    })
    .await
    .map_err(text)?
}

#[tauri::command]
pub async fn load_project<R: Runtime>(app: AppHandle<R>, path: String) -> Result<LoadedProject, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, project) = ProjectBundle::open(&path).map_err(text)?;
        // Music replaced in an earlier session: once the project is reopened, no undo can bring those copies back.
        match bundle.forget_unused_music() {
            Ok(removed) if !removed.is_empty() => {
                tracing::info!(?removed, "removed music copies nothing plays")
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "could not look for unused music copies"),
        }
        let root = bundle.root().to_path_buf();
        allow_media(&app, &root);
        let (cursor, events) = read_telemetry(&root);
        Ok(LoadedProject {
            root: root.to_string_lossy().into_owned(),
            media: screen_media(&root, &project),
            tracks: track_media(&root, &project),
            cursor: drawn_cursor(&project, &cursor, &events),
            project,
            events,
        })
    })
    .await
    .map_err(text)?
}

/// Lets the editor stream a project's media when it lives outside the usual folder.
fn allow_media<R: Runtime>(app: &AppHandle<R>, root: &Path) {
    use tauri::Manager;
    if let Err(error) = app.asset_protocol_scope().allow_directory(root, true) {
        tracing::warn!(%error, "could not allow the project's media");
    }
}

/// Saves the editor's changes. With a `label`, also keeps a version in the project's history.
#[tauri::command]
pub async fn save_project(path: String, mut project: Project, label: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, _) = ProjectBundle::open(&path).map_err(text)?;
        bundle.save(&mut project).map_err(text)?;
        if let Some(label) = label {
            bundle.snapshot(&project, &label).map_err(text)?;
        }
        Ok(())
    })
    .await
    .map_err(text)?
}

/// Every project in the projects folder, most recently changed first.
#[tauri::command]
pub async fn list_projects<R: Runtime>(
    app: AppHandle<R>,
    folder: Option<String>,
) -> Result<Vec<ProjectSummary>, String> {
    let folder = projects_folder(&app, folder.as_deref())?;
    // The Library shows frames from each project's video.
    if folder.is_dir()
        && let Err(error) = app.asset_protocol_scope().allow_directory(&folder, true)
    {
        tracing::warn!(%error, "could not allow the projects folder for previews");
    }
    tauri::async_runtime::spawn_blocking(move || Ok(scan(&folder))).await.map_err(text)?
}

/// The default projects folder, the short way (`~/Movies/Shotmaxxing`), for Settings to show.
#[tauri::command]
pub fn default_projects_folder<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    Ok(tildify(&projects_folder(&app, None)?, app.path().home_dir().ok().as_deref()))
}

/// Asks where to save recordings, starting from the current folder. Returns the folder the short way, or nothing
/// if the person cancelled.
#[tauri::command]
pub async fn choose_projects_folder<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    current: Option<String>,
) -> Result<Option<String>, String> {
    let start = projects_folder(&app, current.as_deref())?;
    pick_folder(&app, &window, "Save recordings to", &start).await
}

/// The system's folder picker as a sheet on `window`, starting in `start`. Gives the folder the short way (`~/…`).
pub async fn pick_folder<R: Runtime>(
    app: &AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
    title: &str,
    start: &Path,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title(title).set_parent(window);
    if start.is_dir() {
        dialog = dialog.set_directory(start);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    dialog.pick_folder(move |picked| {
        let _ = tx.send(picked);
    });
    let picked =
        tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten()).await.map_err(text)?;
    let home = app.path().home_dir().ok();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| tildify(&p, home.as_deref())))
}

/// Asks for a music file and copies it into the project's media folder. `None` when the person cancels. The
/// editor then adds the track, so undo takes it out again; the copy stays for redo.
#[tauri::command]
pub async fn choose_music<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    project: String,
) -> Result<Option<shotmaxxing_render::ImportedMusic>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title("Choose music")
        .set_parent(&window)
        .add_filter("Music", &shotmaxxing_render::MUSIC_TYPES)
        .pick_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked =
        tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten()).await.map_err(text)?;
    let Some(file) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    tauri::async_runtime::spawn_blocking(move || shotmaxxing_render::import_music(Path::new(&project), &file))
        .await
        .map_err(text)?
        .map(Some)
        .map_err(text)
}

fn scan(folder: &Path) -> Vec<ProjectSummary> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut out: Vec<ProjectSummary> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "smx") && path.is_dir())
        .filter_map(|path: PathBuf| {
            let (bundle, project) = ProjectBundle::open(&path).ok()?;
            let root = bundle.root();
            Some(ProjectSummary {
                path: root.to_string_lossy().into_owned(),
                name: project.name.clone(),
                duration_us: project.duration_us(),
                created_at_ms: project.created_at_ms,
                updated_at_ms: project.updated_at_ms,
                app: project.source.app_name.clone(),
                agent: project.agent.as_ref().map(|a| a.client.clone()),
                zooms: project.zooms.len(),
                width: project.source.size.width,
                height: project.source.size.height,
                media: screen_media(root, &project),
                size_bytes: folder_size(&root.join("media")),
            })
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.updated_at_ms));
    out
}

fn folder_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
        })
        .unwrap_or(0)
}

/// Gives a project a new name. The folder keeps its name, so links to it keep working.
#[tauri::command]
pub async fn rename_project(path: String, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("A project needs a name.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, mut project) = ProjectBundle::open(&path).map_err(text)?;
        project.name = name;
        bundle.save(&mut project).map_err(text)
    })
    .await
    .map_err(text)?
}

/// Copies a project next to the original, named "… copy", and returns the copy's path.
#[tauri::command]
pub async fn duplicate_project(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, original) = ProjectBundle::open(&path).map_err(text)?;
        let parent = bundle.root().parent().ok_or("The project has no folder.")?.to_path_buf();
        let mut copy = original.clone();
        copy.name = format!("{} copy", original.name);
        copy.id = String::new();
        let target = ProjectBundle::create(&parent, &mut copy).map_err(text)?;
        for dir in ["media", "telemetry"] {
            copy_dir(&bundle.root().join(dir), &target.root().join(dir)).map_err(text)?;
        }
        target.save(&mut copy).map_err(text)?;
        Ok(target.root().to_string_lossy().into_owned())
    })
    .await
    .map_err(text)?
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Moves a project to the Trash (the Recycle Bin on Windows), where it can still be restored.
#[tauri::command]
pub async fn trash_project(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Only ever move a real project folder.
        ProjectBundle::open(&path).map_err(text)?;
        trash::delete(&path).map_err(text)
    })
    .await
    .map_err(text)?
}

/// Plans auto zoom again from the recorded clicks and typing. Nothing is saved; the editor applies the result as
/// an undoable edit.
#[tauri::command]
pub async fn plan_auto_zooms(path: String, depth: ZoomDepth) -> Result<Vec<ZoomRegion>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, project) = ProjectBundle::open(&path).map_err(text)?;
        let (cursor, events) = read_telemetry(bundle.root());
        let input = ZoomInput {
            duration_us: project.duration_us(),
            capture: project.source.size,
            events: &events,
            cursor: &cursor,
        };
        Ok(plan_zooms(&input, &project.zoom.tuning, depth, project.zoom.motion.transition_ms()))
    })
    .await
    .map_err(text)?
}
