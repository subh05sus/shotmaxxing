//! How the app sits in the system: its menu bar icon and whether it opens at login.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_autostart::ManagerExt;

/// Whether the menu bar icon is hidden. Without it the app keeps a way back: the Dock icon stays on macOS, and on
/// Windows the app ends with its last window.
#[derive(Default)]
pub struct MenuBarIcon {
    hidden: AtomicBool,
}

impl MenuBarIcon {
    pub fn hidden(&self) -> bool {
        self.hidden.load(Ordering::Relaxed)
    }
}

#[tauri::command]
pub fn set_menu_bar_icon<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, MenuBarIcon>,
    visible: bool,
) -> Result<(), String> {
    state.hidden.store(!visible, Ordering::Relaxed);
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_visible(visible).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Whether the app opens at login, as the system has it.
#[tauri::command]
pub fn launch_at_login<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_launch_at_login<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    let login = app.autolaunch();
    if enabled { login.enable() } else { login.disable() }.map_err(|e| e.to_string())
}
