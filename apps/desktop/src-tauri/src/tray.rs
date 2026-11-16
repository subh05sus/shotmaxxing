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

/// The menu-bar icon: a template image, which follows the menu bar's light or dark appearance on macOS.
const ICON: &[u8] = include_bytes!("../icons/tray-template.png");

/// Puts a small red dot on the menu-bar icon while Allow everything is on, or takes it off, and says so in the
/// icon's tooltip. On macOS the dot is a layer over the template image, so the glyph still follows the menu bar.
pub fn set_trusted<R: Runtime>(app: &AppHandle<R>, on: bool) {
    let Some(tray) = app.tray_by_id("main") else { return };
    let tooltip = if on { "Shotmaxxing · Allow everything is on" } else { "Shotmaxxing" };
    if let Err(error) = tray.set_tooltip(Some(tooltip)) {
        tracing::warn!(%error, "couldn't change the menu-bar icon's tooltip");
    }
    #[cfg(target_os = "macos")]
    let marked = tray.with_inner_tray_icon(move |icon| {
        if let Some(item) = icon.ns_status_item() {
            dot::set(&item, on);
        }
    });
    #[cfg(not(target_os = "macos"))]
    let marked = Image::from_bytes(ICON)
        .and_then(|icon| tray.set_icon(Some(if on { dot::drawn(&icon) } else { icon })));
    if let Err(error) = marked {
        tracing::warn!(%error, "couldn't mark the menu-bar icon");
    }
}

#[cfg(target_os = "macos")]
mod dot {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSColor, NSStatusItem};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::NSString;
    use objc2_quartz_core::CALayer;

    /// The dot's layer, by name, so it's found again to take off.
    const NAME: &str = "allow-everything";
    /// Its size, in points, and the icon's: the status item draws its image 18 points high, in the middle.
    const SIZE: f64 = 7.0;
    const ICON: f64 = 18.0;

    pub fn set(item: &NSStatusItem, on: bool) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(button) = item.button(mtm) else { return };
        button.setWantsLayer(true);
        let Some(layer) = button.layer() else { return };
        let name = NSString::from_str(NAME);
        // SAFETY: the button's own layer's sublayers, read on the main thread.
        if let Some(sublayers) = unsafe { layer.sublayers() } {
            for sublayer in sublayers.iter().filter(|l| l.name().is_some_and(|n| *n == *name)) {
                sublayer.removeFromSuperlayer();
            }
        }
        if !on {
            return;
        }
        let bounds = button.bounds();
        let (w, h) = (bounds.size.width, bounds.size.height);
        // At the icon's top right corner, a little in.
        let x = w / 2.0 + ICON / 2.0 - SIZE + 1.0;
        let top = (h - ICON) / 2.0 - 1.0;
        let y = if button.isFlipped() { top } else { h - top - SIZE };
        let dot = CALayer::new();
        dot.setName(Some(&name));
        dot.setBackgroundColor(Some(&NSColor::systemRedColor().CGColor()));
        dot.setCornerRadius(SIZE / 2.0);
        dot.setFrame(CGRect::new(CGPoint::new(x, y), CGSize::new(SIZE, SIZE)));
        layer.addSublayer(&dot);
    }
}

#[cfg(not(target_os = "macos"))]
mod dot {
    use tauri::image::Image;

    /// The icon with a red dot at its top right, a clear ring round it so it reads on any taskbar.
    pub fn drawn(icon: &Image<'_>) -> Image<'static> {
        let (w, h) = (icon.width(), icon.height());
        let mut rgba = icon.rgba().to_vec();
        let radius = w.min(h) as f64 * 0.2;
        let (cx, cy) = (w as f64 - radius - 1.0, radius + 1.0);
        for y in 0..h {
            for x in 0..w {
                let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                let at = ((y * w + x) * 4) as usize;
                if d <= radius {
                    rgba[at..at + 4].copy_from_slice(&[255, 59, 48, 255]);
                } else if d <= radius + 1.5 {
                    rgba[at + 3] = 0;
                }
            }
        }
        Image::new_owned(rgba, w, h)
    }
}

pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(ICON)?)
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
