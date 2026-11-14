//! The menu bar item (system tray on Windows): the app's home while no window is open.

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};

use crate::{recording, windows};

/// Builds the menu for the current state: while recording, the first items stop or discard the take.
fn menu<R: Runtime>(app: &AppHandle<R>, recording: bool) -> tauri::Result<tauri::menu::Menu<R>> {
    let mut builder = MenuBuilder::new(app);
    if recording {
        let stop =
            MenuItemBuilder::with_id("stop", "Stop Recording").accelerator("Alt+Shift+5").build(app)?;
        let discard = MenuItemBuilder::with_id("discard", "Discard Recording").build(app)?;
        builder = builder.item(&stop).item(&discard);
    } else {
        let record =
            MenuItemBuilder::with_id("record", "New Recording…").accelerator("Alt+Shift+5").build(app)?;
        builder = builder.item(&record);
    }
    let library = MenuItemBuilder::with_id("library", "Open Library").build(app)?;
    let agents = MenuItemBuilder::with_id("agents", "Agents…").build(app)?;
    let status = MenuItemBuilder::with_id("status", "No agent take running").enabled(false).build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…").accelerator("CmdOrCtrl+,").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Shotmaxxing").accelerator("CmdOrCtrl+Q").build(app)?;
    builder
        .separator()
        .item(&library)
        .item(&agents)
        .separator()
        .item(&status)
        .separator()
        .item(&settings)
        .item(&quit)
        .build()
}

/// Switches the menu between its recording and idle forms.
pub fn set_recording<R: Runtime>(app: &AppHandle<R>, recording: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    match menu(app, recording) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                tracing::warn!(%error, "could not update the menu bar menu");
            }
        }
        Err(error) => tracing::warn!(%error, "could not build the menu bar menu"),
    }
}

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        // A template image follows the menu bar's light or dark appearance on macOS.
        .icon_as_template(true)
        .tooltip("Shotmaxxing")
        .menu(&menu(app, false)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let result = match event.id().as_ref() {
                "record" => windows::open_record(app),
                "stop" => {
                    recording::stop_and_edit(app);
                    Ok(())
                }
                "discard" => {
                    recording::discard_and_close(app);
                    Ok(())
                }
                "library" => windows::open_home(app, Some("home/library")),
                "agents" => windows::open_home(app, Some("home/agents")),
                "settings" => windows::open_home(app, Some("home/settings")),
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                tracing::warn!(%error, "menu action failed");
            }
        })
        .build(app)?;
    Ok(())
}
