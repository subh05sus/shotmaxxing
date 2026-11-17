//! Updates: finding a newer Shotmaxxing, downloading it in the background, and swapping it in when the person says
//! so. The updater plugin asks shotmaxxing.app (which reads GitHub's releases) for the newest version, and only
//! installs a download whose signature matches the public key in tauri.conf.json. Installing restarts the app, so
//! it's refused while something is recording or working, like any other restart.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The update found last and, once downloaded, its bytes.
#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
    downloaded: Mutex<Option<(String, Vec<u8>)>>,
}

/// A newer version, as the page shows it.
#[derive(Debug, Serialize)]
pub struct Found {
    version: String,
    current: String,
    /// When it was published, as RFC 3339, when the release says.
    date: Option<String>,
    /// Whether it's already downloaded and only needs a restart.
    ready: bool,
}

/// How far a download has got, sent to the page as `update-progress`.
#[derive(Clone, Debug, Serialize)]
struct Progress {
    version: String,
    done: u64,
    total: Option<u64>,
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Asks whether a newer version exists. `None` when this one is the newest.
#[tauri::command]
pub async fn check_update<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Updates>,
) -> Result<Option<Found>, String> {
    let update = app.updater().map_err(text)?.check().await.map_err(text)?;
    let ready = |version: &str| {
        state.downloaded.lock().map(|d| d.as_ref().is_some_and(|(v, _)| v == version)).unwrap_or(false)
    };
    let found = update.as_ref().map(|u| Found {
        version: u.version.clone(),
        current: u.current_version.clone(),
        date: u.date.and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok()),
        ready: ready(&u.version),
    });
    *state.found.lock().map_err(text)? = update;
    Ok(found)
}

/// Downloads the update found last, sending `update-progress` as it goes. The signature is checked when installing.
#[tauri::command]
pub async fn download_update<R: Runtime>(app: AppHandle<R>, state: State<'_, Updates>) -> Result<(), String> {
    let update = state.found.lock().map_err(text)?.clone().ok_or("There's no update to download.")?;
    let version = update.version.clone();
    if state.downloaded.lock().map_err(text)?.as_ref().is_some_and(|(v, _)| *v == version) {
        return Ok(());
    }
    let (sender, label) = (app.clone(), version.clone());
    let mut done = 0u64;
    let mut sent_percent = None;
    let bytes = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                // About a hundred messages a download, not one per chunk.
                let percent = total.map(|t| done * 100 / t.max(1));
                if percent != sent_percent || total.is_none() {
                    sent_percent = percent;
                    let _ = sender.emit("update-progress", Progress { version: label.clone(), done, total });
                }
            },
            || {},
        )
        .await
        .map_err(text)?;
    *state.downloaded.lock().map_err(text)? = Some((version, bytes));
    Ok(())
}

/// Installs the downloaded update and opens the new version. Refused while quitting would cut something short.
#[tauri::command]
pub fn install_update<R: Runtime>(app: AppHandle<R>, state: State<'_, Updates>) -> Result<(), String> {
    if let Some(what) = crate::commands::busy(&app) {
        return Err(format!("Shotmaxxing can't restart while {what}. Restart to update once it's done."));
    }
    let update = state.found.lock().map_err(text)?.clone().ok_or("There's no update to install.")?;
    let (version, bytes) =
        state.downloaded.lock().map_err(text)?.take().ok_or("The update hasn't finished downloading yet.")?;
    if version != update.version {
        return Err("A newer update came out while this one downloaded. Check for updates again.".into());
    }
    if let Err(error) = update.install(&bytes) {
        // Keep the download, so trying again doesn't fetch it again.
        *state.downloaded.lock().map_err(text)? = Some((version, bytes));
        return Err(format!("The update couldn't be installed: {error}"));
    }
    crate::commands::relaunch(app, None)
}
