//! The Shotmaxxing desktop app.
//!
//! The app lives in the menu bar (the system tray on Windows). Windows open on demand: Home, one editor per
//! project, the record dialog, and the two floating windows shown while filming (the recording pill and the
//! agent take card). Video pixels never pass through the web view; the UI only drives the native core.

mod commands;
mod platform;
mod shortcuts;
mod tray;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

/// Starts the app and blocks until it quits.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app = tauri::Builder::default()
        // A second launch focuses the running app instead of starting another copy.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(error) = windows::open_home(app, None) {
                tracing::warn!(%error, "could not open Home for a second launch");
            }
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle();
            tray::create(handle)?;
            shortcuts::register(handle)?;
            windows::open_home(handle, None)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform_info,
            commands::permission_status,
            commands::open_permission_settings,
            commands::power_status,
            commands::list_sources,
            commands::open_window,
            commands::reveal_in_folder,
            commands::connect_agent,
        ])
        .build(tauri::generate_context!())
        .expect("Shotmaxxing could not start");

    app.run(|app, event| match event {
        // Closing the last window keeps the app running in the menu bar. Quit from the menu ends it.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::WindowEvent { event: WindowEvent::Destroyed, .. } => {
            if app.webview_windows().is_empty() {
                windows::set_dock_visible(app, false);
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { has_visible_windows: false, .. } => {
            if let Err(error) = windows::open_home(app, None) {
                tracing::warn!(%error, "could not reopen Home");
            }
        }
        _ => {}
    });
}
