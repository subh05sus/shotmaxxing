//! Every window the app opens, with its size and native look. Each window loads the same UI bundle at a different
//! route, so it only builds the screen it shows.

use std::path::Path;

use tauri::webview::WebviewWindowBuilder;
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Runtime, WebviewUrl, WebviewWindow};

#[cfg(target_os = "macos")]
use tauri::{LogicalPosition, TitleBarStyle};

/// Shows or hides the Dock icon on macOS: visible while any window is open, hidden when the app is only in the
/// menu bar. Other platforms always show the app in the taskbar while a window is open.
pub fn set_dock_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy =
            if visible { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory };
        if let Err(error) = app.set_activation_policy(policy) {
            tracing::warn!(%error, "could not change the activation policy");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}

/// How a window joins macOS's Spaces, beyond what Tauri sets: `NSWindowCollectionBehavior` flags.
#[cfg(target_os = "macos")]
mod spaces {
    /// Comes to the Space that's showing when it's brought forward, instead of taking the Mac back to its own.
    pub const MOVE_TO_ACTIVE_SPACE: usize = 1 << 1;
    /// Can show over a full-screen app, in that app's Space.
    pub const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
}

/// Adds `flags` ([`spaces`]) to how `window` joins Spaces. AppKit wants windows changed on the main thread.
#[cfg(target_os = "macos")]
fn join_spaces<R: Runtime>(window: &WebviewWindow<R>, flags: usize) {
    let target = window.clone();
    let changed = window.run_on_main_thread(move || {
        let Ok(ns_window) = target.ns_window() else { return };
        let ns_window = ns_window.cast::<objc2::runtime::AnyObject>();
        // SAFETY: Tauri's NSWindow for this window, alive while the window is, changed on the main thread;
        // collectionBehavior is an NSUInteger.
        unsafe {
            let behavior: usize = objc2::msg_send![&*ns_window, collectionBehavior];
            let () = objc2::msg_send![&*ns_window, setCollectionBehavior: behavior | flags];
        }
    });
    if let Err(error) = changed {
        tracing::warn!(%error, "a window couldn't be set to show over full-screen apps");
    }
}

/// Brings an existing window forward, optionally moving it to another route.
fn focus<R: Runtime>(window: &WebviewWindow<R>, route: Option<&str>) -> tauri::Result<()> {
    if let Some(route) = route {
        let script = format!("location.hash = {};", serde_json::to_string(&format!("#/{route}"))?);
        window.eval(&script)?;
    }
    window.unminimize()?;
    window.show()?;
    window.set_focus()
}

/// The standard window look: an overlay title bar with inset traffic lights on macOS, the native frame on Windows.
fn standard<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(20.0, 27.5));
    builder
}

/// Where the theme chosen in Settings is remembered, in the app's config folder, so the next launch starts in it.
const THEME_FILE: &str = "theme";

/// The native appearance for a theme setting: `None` follows the system.
fn native_theme(setting: &str) -> Option<tauri::Theme> {
    match setting {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// Starts the app in the theme it was left in, before any window opens, so the sidebar's material doesn't flash in
/// the system's appearance first.
pub fn restore_theme<R: Runtime>(app: &AppHandle<R>) {
    let Ok(dir) = app.path().app_config_dir() else { return };
    if let Ok(setting) = std::fs::read_to_string(dir.join(THEME_FILE)) {
        app.set_theme(native_theme(setting.trim()));
    }
}

/// Makes the app's native parts follow the theme chosen in Settings: `system`, `light` or `dark`. The window
/// material, menus and dialogs otherwise follow the system, so a Light UI on a Mac in Dark Mode put dark text on a
/// dark sidebar.
#[tauri::command]
pub fn set_app_theme<R: Runtime>(app: AppHandle<R>, theme: String) -> Result<(), String> {
    if !["system", "light", "dark"].contains(&theme.as_str()) {
        return Err(format!("there's no theme called “{theme}”"));
    }
    app.set_theme(native_theme(&theme));
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(THEME_FILE), theme).map_err(|e| e.to_string())
}

/// Native translucent material behind the window: the sidebar material on macOS, Mica on Windows 11. The UI
/// paints its own content areas and leaves the sidebar clear, so the material shows only there.
fn with_material<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    #[cfg(target_os = "macos")]
    let effect = Effect::Sidebar;
    #[cfg(not(target_os = "macos"))]
    let effect = Effect::Mica;
    builder
        .transparent(true)
        .effects(EffectsBuilder::new().effect(effect).state(EffectState::FollowsWindowActiveState).build())
}

pub fn open_home<R: Runtime>(app: &AppHandle<R>, route: Option<&str>) -> tauri::Result<()> {
    set_dock_visible(app, true);
    if let Some(window) = app.get_webview_window("home") {
        return focus(&window, route);
    }
    let url = format!("index.html#/{}", route.unwrap_or(""));
    let builder = WebviewWindowBuilder::new(app, "home", WebviewUrl::App(url.into()))
        .title("Shotmaxxing")
        .inner_size(1120.0, 740.0)
        .min_inner_size(860.0, 560.0)
        .center();
    with_material(standard(builder)).build()?;
    Ok(())
}

pub fn open_editor<R: Runtime>(app: &AppHandle<R>, project: &str) -> tauri::Result<()> {
    set_dock_visible(app, true);
    let label = editor_label(project);
    if let Some(window) = app.get_webview_window(&label) {
        // It shows this project again, even if it has moved on to another page since.
        return focus(&window, Some(&format!("editor/{project}")));
    }
    let url = format!("index.html#/editor/{project}");
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Shotmaxxing")
        .inner_size(1360.0, 860.0)
        .min_inner_size(1000.0, 640.0)
        .center();
    standard(builder).build()?;
    Ok(())
}

/// Opens a saved project (its folder) in the editor.
pub fn open_project<R: Runtime>(app: &AppHandle<R>, root: &Path) -> tauri::Result<()> {
    open_editor(app, &encode_component(&root.to_string_lossy()))
}

/// The same encoding as JavaScript's `encodeURIComponent`, so editor routes match whichever side opened them.
fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Opens the record dialog, ready for a recording or, when `mode` is `screenshot`, for a screenshot. An open dialog
/// switches to that mode.
pub fn open_record<R: Runtime>(app: &AppHandle<R>, mode: Option<&str>) -> tauri::Result<()> {
    set_dock_visible(app, true);
    let mode = if mode == Some("screenshot") { "screenshot" } else { "video" };
    if let Some(window) = app.get_webview_window("record") {
        let _ = app.emit_to("record", "record-mode", mode);
        return focus(&window, None);
    }
    let url = format!("index.html#/record?mode={mode}");
    let builder = WebviewWindowBuilder::new(app, "record", WebviewUrl::App(url.into()))
        .title(if mode == "screenshot" { "New screenshot" } else { "New recording" })
        .inner_size(840.0, 600.0)
        .min_inner_size(640.0, 460.0)
        .center();
    let window = standard(builder).build()?;
    // Opened from a full-screen app (with the shortcut, say), it shows there, over that app, rather than taking the
    // Mac back to the desktop where the other Shotmaxxing windows are.
    #[cfg(target_os = "macos")]
    join_spaces(&window, spaces::MOVE_TO_ACTIVE_SPACE | spaces::FULL_SCREEN_AUXILIARY);
    #[cfg(not(target_os = "macos"))]
    let _ = window;
    Ok(())
}

pub fn open_onboarding<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    open_home(app, Some("onboarding"))
}

/// The recording pill: small, always on top, never captured in the recording, and it doesn't take focus.
pub fn open_pill<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("pill") {
        return window.show();
    }
    // Room for the pill and its shadow: 16 pt above it and 24 pt below.
    let (width, height) = (360.0, 84.0);
    let builder = floating(app, "pill", "index.html#/pill", width, height)?;
    let x = monitor_width(app).map_or(400.0, |w| (w - width) / 2.0);
    let window = builder.position(x, 28.0).build()?;
    over_full_screen(&window);
    Ok(())
}

/// The live take card for agent takes, bottom right.
pub fn open_take_card<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("take") {
        return window.show();
    }
    let (width, height) = (420.0, 540.0);
    let builder = floating(app, "take", "index.html#/take", width, height)?;
    let (x, y) = monitor_size(app).map_or((900.0, 300.0), |(w, h)| (w - width - 12.0, h - height - 12.0));
    let window = builder.position(x, y).build()?;
    over_full_screen(&window);
    Ok(())
}

/// A floating window (the pill, the take card) is on every Space already; this lets it show over a full-screen app
/// too, which is where a take of one happens.
fn over_full_screen<R: Runtime>(window: &WebviewWindow<R>) {
    #[cfg(target_os = "macos")]
    join_spaces(window, spaces::FULL_SCREEN_AUXILIARY);
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

fn floating<'a, R: Runtime>(
    app: &'a AppHandle<R>,
    label: &str,
    url: &str,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindowBuilder<'a, R, AppHandle<R>>> {
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("Shotmaxxing")
        .inner_size(width, height)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        // A click on Stop or Allow works straight away, without first bringing the app forward.
        .accept_first_mouse(true);
    // Keeping these windows out of recordings: on macOS the capture filter leaves out the whole app, so screenshots
    // and screen sharing still see them. Windows has no such filter, so there the window opts out of capture.
    #[cfg(windows)]
    let builder = builder.content_protected(true);
    Ok(builder)
}

fn monitor_size<R: Runtime>(app: &AppHandle<R>) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let size: LogicalSize<f64> = monitor.size().to_logical(monitor.scale_factor());
    Some((size.width, size.height))
}

fn monitor_width<R: Runtime>(app: &AppHandle<R>) -> Option<f64> {
    monitor_size(app).map(|(w, _)| w)
}

/// Window labels allow only letters, digits and `-/:_`.
/// One editor window per project. Paths are long and can share a prefix, so the label uses a hash of the whole
/// target; the sample keeps a readable label.
fn editor_label(target: &str) -> String {
    use std::hash::{Hash, Hasher};
    // `?export=1` and other options don't make it a different project.
    let project = target.split('?').next().unwrap_or(target);
    if project.len() <= 32 && project.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return format!("editor-{}", sanitize_label(project));
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    project.hash(&mut hasher);
    format!("editor-{:016x}", hasher.finish())
}

fn sanitize_label(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '-' })
        .take(64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{encode_component, sanitize_label};

    #[test]
    fn encodes_like_encode_uri_component() {
        assert_eq!(encode_component("/Users/a b/Rec (1).smx"), "%2FUsers%2Fa%20b%2FRec%20(1).smx");
        assert_eq!(encode_component("Café"), "Caf%C3%A9");
    }

    #[test]
    fn labels_are_safe() {
        assert_eq!(sanitize_label("launch demo/2.smx"), "launch-demo-2-smx");
        assert_eq!(sanitize_label("sample"), "sample");
    }
}
