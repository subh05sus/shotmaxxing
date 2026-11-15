//! Content packs in the app: listing the ones added on this computer, adding one the person chooses, and taking one
//! out. Reading and checking a pack is `shotmaxxing_render::packs`, shared with the `shotmaxxing` command.

use serde::Serialize;
use shotmaxxing_render::packs::{self, Pack};
use tauri::{AppHandle, Runtime};

/// A pack that was just added, and whether it replaced an earlier version of itself.
#[derive(Serialize)]
pub struct AddedPack {
    pack: Pack,
    replaced: bool,
}

fn folder() -> Result<std::path::PathBuf, String> {
    packs::dir().ok_or_else(|| "Couldn't find Shotmaxxing's data folder.".to_string())
}

/// The content packs added on this computer, by name.
#[tauri::command]
pub async fn list_packs() -> Vec<Pack> {
    tauri::async_runtime::spawn_blocking(packs::installed).await.unwrap_or_default()
}

/// Asks for a content pack file and adds it, replacing an earlier version of the same pack. `None` when the person
/// cancels; an error says what's wrong with the file.
#[tauri::command]
pub async fn add_pack<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
) -> Result<Option<AddedPack>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title("Add a content pack")
        .set_parent(&window)
        .add_filter("Content pack", &["json"])
        .pick_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|e| e.to_string())?;
    let Some(file) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let dir = folder()?;
    let (pack, replaced) = tauri::async_runtime::spawn_blocking(move || packs::add(&dir, &file))
        .await
        .map_err(|e| e.to_string())??;
    Ok(Some(AddedPack { pack, replaced }))
}

/// Puts back a pack that was just removed (Undo), checked again.
#[tauri::command]
pub async fn restore_pack(pack: Pack) -> Result<(), String> {
    let dir = folder()?;
    tauri::async_runtime::spawn_blocking(move || packs::put(&dir, &pack).map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
}

/// Takes the pack `id` out. Projects that use its wallpapers keep their own copies.
#[tauri::command]
pub async fn remove_pack(id: String) -> Result<(), String> {
    let dir = folder()?;
    tauri::async_runtime::spawn_blocking(move || packs::remove(&dir, &id)).await.map_err(|e| e.to_string())?
}
