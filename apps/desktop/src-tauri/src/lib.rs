//! The Shotmaxxing desktop app.
//!
//! The app lives in the menu bar (the system tray on Windows). Windows open on demand: Home, one editor per
//! project, the record dialog, and the two floating windows shown while filming (the recording pill and the
//! agent take card). Video pixels never pass through the web view; the UI only drives the native core.

mod agents;
mod captions;
mod commands;
mod edge;
mod export;
mod packs;
mod platform;
mod projects;
mod recording;
mod screenshots;
mod shortcuts;
mod system;
mod takes;
mod tray;
mod voice_over;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

/// In a copy of the app started only to read its own permissions: answers and returns true, before anything else
/// starts, and the copy should then end. False for a normal launch.
pub fn answer_permission_probe() -> bool {
    platform::answer_permission_probe()
}

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .manage(recording::Recordings::default())
        .manage(recording::Notices::default())
        .manage(system::MenuBarIcon::default())
        .manage(tray::IconArea::default())
        .manage(export::Exports::default())
        .manage(takes::Takes::default())
        .manage(takes::Trust::default())
        .manage(voice_over::VoiceOvers::default())
        .manage(captions::Captioning::default())
        .setup(|app| {
            let handle = app.handle();
            tray::create(handle)?;
            shortcuts::register(handle)?;
            takes::listen(handle);
            windows::restore_theme(handle);
            // "launch" tells the UI the app has just started, for the logo's short build. Reopened by itself (to
            // finish turning on Screen Recording), it goes back to the page it was on.
            let launch = match commands::reopened_to() {
                Some(to) => format!("launch?to={to}"),
                None => "launch".to_owned(),
            };
            windows::open_home(handle, Some(&launch))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::platform_info,
            commands::permission_status,
            commands::open_permission_settings,
            commands::relaunch,
            commands::power_status,
            commands::list_sources,
            commands::source_thumbnails,
            commands::list_devices,
            agents::detect_agents,
            commands::open_window,
            commands::reveal_in_folder,
            commands::show_error,
            windows::set_app_theme,
            packs::list_packs,
            packs::add_pack,
            packs::remove_pack,
            packs::restore_pack,
            commands::open_link,
            agents::connect_agent,
            agents::disconnect_agent,
            agents::mcp_command,
            recording::start_recording,
            screenshots::take_screenshot,
            screenshots::copy_screenshot,
            voice_over::start_voice_over,
            voice_over::stop_voice_over,
            voice_over::discard_voice_over,
            recording::stop_recording,
            recording::discard_recording,
            recording::restart_recording,
            recording::take_recording_notices,
            projects::load_project,
            projects::cursor_path,
            projects::save_project,
            projects::list_projects,
            projects::plan_auto_zooms,
            projects::find_pauses,
            projects::clean_voice,
            captions::caption_model,
            captions::generate_captions,
            captions::cancel_captions,
            captions::save_captions,
            projects::choose_music,
            projects::choose_background,
            projects::import_background,
            projects::rename_project,
            projects::duplicate_project,
            projects::trash_project,
            projects::default_projects_folder,
            projects::choose_projects_folder,
            export::export_project,
            export::cancel_export,
            export::choose_export_folder,
            system::set_menu_bar_icon,
            system::launch_at_login,
            system::set_launch_at_login,
            takes::take_card,
            takes::agent_trust,
            takes::set_agent_trust,
            takes::answer_take,
            takes::stop_take,
            takes::pause_take,
            takes::resume_take,
            takes::take_over_take,
            takes::hand_back_take,
            takes::dismiss_take,
        ])
        .build(tauri::generate_context!())
        .expect("Shotmaxxing could not start");

    app.run(|app, event| {
        // A voice-over its editor can't finish any more is thrown away, whatever else the event means.
        if let RunEvent::WindowEvent { label, event: WindowEvent::Destroyed, .. } = &event {
            app.state::<voice_over::VoiceOvers>().abandon(Some(label));
        }
        on_event(app, event);
    });
}

fn on_event(app: &tauri::AppHandle, event: RunEvent) {
    match event {
        // Closing the last window keeps the app running in the menu bar. Quit from the menu ends it.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        // Quitting mid-recording keeps the take.
        RunEvent::Exit => {
            app.state::<recording::Recordings>().finish_on_quit();
            app.state::<export::Exports>().cancel_all();
            app.state::<captions::Captioning>().cancel_all();
            app.state::<voice_over::VoiceOvers>().abandon(None);
        }
        // The take card can't be closed while it asks or a take plays: it comes straight back.
        RunEvent::WindowEvent { label, event: WindowEvent::Destroyed, .. }
            if label == "take" && app.state::<takes::Takes>().is_live() =>
        {
            app.state::<takes::Takes>().keep_shown(app);
        }
        RunEvent::WindowEvent { event: WindowEvent::Destroyed, .. } if app.webview_windows().is_empty() => {
            if !app.state::<system::MenuBarIcon>().hidden() {
                windows::set_dock_visible(app, false);
            } else {
                // Without a menu bar icon the app must stay findable: macOS keeps the Dock icon, and on Windows
                // closing the last window ends the app (a recording keeps going until it is stopped).
                #[cfg(not(target_os = "macos"))]
                if !app.state::<recording::Recordings>().is_recording() {
                    app.exit(0);
                }
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { has_visible_windows: false, .. } => {
            if let Err(error) = windows::open_home(app, None) {
                tracing::warn!(%error, "could not reopen Home");
            }
        }
        _ => {}
    }
}
