//! Global shortcuts, available from any app.

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::recording::{self, Recordings};
use crate::windows;

/// ⌥⇧5 (Alt+Shift+5): start a new recording, or stop the one running and open it. Close to the system's own ⇧⌘5
/// screenshot shortcut without clashing with it.
fn record() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Digit5)
}

/// ⌥⇧4 (Alt+Shift+4): take a screenshot, from the record dialog opened ready for one. It sits beside ⌥⇧5 the way
/// the system's ⇧⌘4 sits beside ⇧⌘5.
fn screenshot() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Digit4)
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (record, screenshot) = (record(), screenshot());
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let opened = if *shortcut == screenshot {
                    windows::open_record(app, Some("screenshot"))
                } else if *shortcut != record {
                    return;
                } else if app.state::<Recordings>().is_recording() {
                    recording::stop_and_edit(app);
                    return;
                } else {
                    windows::open_record(app, None)
                };
                if let Err(error) = opened {
                    tracing::warn!(%error, "could not open the record dialog");
                }
            })
            .build(),
    )?;
    // Another app may own a shortcut already. Both work from the menu too, so this isn't fatal.
    for (shortcut, what) in [(record, "record"), (screenshot, "screenshot")] {
        if let Err(error) = app.global_shortcut().register(shortcut) {
            tracing::warn!(%error, "could not register the {what} shortcut");
        }
    }
    Ok(())
}
