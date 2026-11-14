//! The menu bar item (system tray on Windows): the app's home while no window is open.

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Runtime};

use crate::windows;

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let record =
        MenuItemBuilder::with_id("record", "New Recording…").accelerator("Alt+Shift+5").build(app)?;
    let library = MenuItemBuilder::with_id("library", "Open Library").build(app)?;
    let agents = MenuItemBuilder::with_id("agents", "Agents…").build(app)?;
    let status = MenuItemBuilder::with_id("status", "No agent take running").enabled(false).build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…").accelerator("CmdOrCtrl+,").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Shotmaxxing").accelerator("CmdOrCtrl+Q").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&record)
        .separator()
        .item(&library)
        .item(&agents)
        .separator()
        .item(&status)
        .separator()
        .item(&settings)
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        // A template image follows the menu bar's light or dark appearance on macOS.
        .icon_as_template(true)
        .tooltip("Shotmaxxing")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let result = match event.id().as_ref() {
                "record" => windows::open_record(app),
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
