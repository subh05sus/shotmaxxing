//! The menu bar item (system tray on Windows): the app's home while no window is open.

use shotmaxxing_capture::LeftOut;
use shotmaxxing_core::geometry::Rect;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::{recording, windows};

/// Where the menu-bar icon is on the desktop, so recordings leave out a click on it (opening its menu to stop the
/// take). It isn't among the window server's windows, so it can't be told by its window as the pill can. And it
/// moves: over a full-screen app the menu bar usually slides in only when the pointer goes up, and other items come
/// and go beside it. So the icon's own events keep this current, since the pointer always passes over the icon
/// before clicking it.
#[derive(Default)]
pub struct IconArea(LeftOut);

/// The icon's place, read now, for a recording about to start. The icon's events keep it current after that.
pub fn area<R: Runtime>(app: &AppHandle<R>) -> LeftOut {
    let shared = app.state::<IconArea>().0.clone();
    // No frame when the icon is hidden in Settings: then there's nothing to click.
    let rect = app.tray_by_id("main").and_then(|tray| tray.rect().ok().flatten());
    shared.set(rect.map(|rect| vec![points(app, rect)]).unwrap_or_default());
    shared
}

/// A frame from the tray, in pixels at its display's scale (the main display's, which has the menu bar), in points.
fn points<R: Runtime>(app: &AppHandle<R>, rect: tauri::Rect) -> Rect {
    let scale = app.primary_monitor().ok().flatten().map_or(1.0, |monitor| monitor.scale_factor());
    let position = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    Rect::new(position.x, position.y, size.width, size.height)
}

/// Keeps [`IconArea`] where the icon is, from the icon's own events (the pointer entering, moving over or clicking
/// it), which carry its frame at that moment.
fn follow<R: Runtime>(app: &AppHandle<R>, event: TrayIconEvent) {
    let rect = match event {
        TrayIconEvent::Enter { rect, .. }
        | TrayIconEvent::Move { rect, .. }
        | TrayIconEvent::Leave { rect, .. }
        | TrayIconEvent::Click { rect, .. }
        | TrayIconEvent::DoubleClick { rect, .. } => rect,
        _ => return,
    };
    app.state::<IconArea>().0.set(vec![points(app, rect)]);
}

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
    let screenshot =
        MenuItemBuilder::with_id("screenshot", "New Screenshot…").accelerator("Alt+Shift+4").build(app)?;
    builder = builder.item(&screenshot);
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
        .on_tray_icon_event(|tray, event| follow(tray.app_handle(), event))
        .on_menu_event(|app, event| {
            let result = match event.id().as_ref() {
                "record" => windows::open_record(app, None),
                "screenshot" => windows::open_record(app, Some("screenshot")),
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
