//! Global shortcuts, available from any app.

use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::windows;

/// ⌥⇧5 (Alt+Shift+5): new recording. Close to the system's own ⇧⌘5 screenshot shortcut without clashing with it.
fn record() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Digit5)
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let record = record();
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if *shortcut == record
                    && event.state() == ShortcutState::Pressed
                    && let Err(error) = windows::open_record(app)
                {
                    tracing::warn!(%error, "could not open the record dialog");
                }
            })
            .build(),
    )?;
    // Another app may own the shortcut already. Recording still works from the menu, so this isn't fatal.
    if let Err(error) = app.global_shortcut().register(record) {
        tracing::warn!(%error, "could not register the record shortcut");
    }
    Ok(())
}
