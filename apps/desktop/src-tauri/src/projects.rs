//! Opening, saving and listing `.smx` projects for the editor and the Library.

use std::path::{Path, PathBuf};

use serde::Serialize;
use shotmaxxing_capture::{CURSOR_FILE, EVENTS_FILE};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::{Project, TrackKind, ZoomDepth, ZoomRegion};
use shotmaxxing_core::telemetry::{CursorSample, InputEvent, decode_cursor, read_events};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};
use tauri::{AppHandle, Runtime};

use crate::recording::projects_folder;

#[derive(Serialize)]
pub struct LoadedProject {
    root: String,
    project: Project,
    /// The screen recording, as a file path for the asset protocol.
    media: Option<String>,
    cursor: Vec<CursorSample>,
    events: Vec<InputEvent>,
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

/// The editor draws the cursor at display rate; 60 samples a second is plenty and keeps the payload small.
fn thin_cursor(samples: Vec<CursorSample>) -> Vec<CursorSample> {
    let mut out: Vec<CursorSample> = Vec::with_capacity(samples.len());
    let last = samples.last().cloned();
    for sample in samples {
        if out.last().is_none_or(|prev| sample.t_us >= prev.t_us + 16_000) {
            out.push(sample);
        }
    }
    if let Some(last) = last
        && out.last().is_some_and(|prev| prev.t_us != last.t_us)
    {
        out.push(last);
    }
    out
}

#[tauri::command]
pub async fn load_project<R: Runtime>(app: AppHandle<R>, path: String) -> Result<LoadedProject, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (bundle, project) = ProjectBundle::open(&path).map_err(text)?;
        let root = bundle.root().to_path_buf();
        allow_media(&app, &root);
        let (cursor, events) = read_telemetry(&root);
        Ok(LoadedProject {
            root: root.to_string_lossy().into_owned(),
            media: screen_media(&root, &project),
            project,
            cursor: thin_cursor(cursor),
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
pub async fn list_projects<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ProjectSummary>, String> {
    let folder = projects_folder(&app)?;
    tauri::async_runtime::spawn_blocking(move || Ok(scan(&folder))).await.map_err(text)?
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

#[cfg(test)]
mod tests {
    use super::{CursorSample, thin_cursor};

    #[test]
    fn thins_the_cursor_to_about_sixty_samples_a_second() {
        let samples: Vec<CursorSample> = (0..240)
            .map(|i| CursorSample {
                t_us: i * 8_333,
                x: f64::from(i as u32),
                y: 0.0,
                shape: Default::default(),
            })
            .collect();
        let thinned = thin_cursor(samples);
        assert!(thinned.len() > 100 && thinned.len() < 130, "{}", thinned.len());
        assert_eq!(thinned.last().map(|s| s.t_us), Some(239 * 8_333));
    }
}
