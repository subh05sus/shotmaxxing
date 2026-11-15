//! Every window the app opens, with its size and native look. Each window loads the same UI bundle at a different
//! route, so it only builds the screen it shows.

use tauri::webview::WebviewWindowBuilder;
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl, WebviewWindow};

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

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
        return focus(&window, None);
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

pub fn open_record<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    set_dock_visible(app, true);
    if let Some(window) = app.get_webview_window("record") {
        return focus(&window, None);
    }
    let builder = WebviewWindowBuilder::new(app, "record", WebviewUrl::App("index.html#/record".into()))
        .title("New recording")
        .inner_size(840.0, 600.0)
        .min_inner_size(640.0, 460.0)
        .center();
    standard(builder).build()?;
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
    let (width, height) = (360.0, 76.0);
    let builder = floating(app, "pill", "index.html#/pill", width, height)?;
    let x = monitor_width(app).map_or(400.0, |w| (w - width) / 2.0);
    builder.position(x, 28.0).build()?;
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
    builder.position(x, y).build()?;
    Ok(())
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
    use super::sanitize_label;

    #[test]
    fn labels_are_safe() {
        assert_eq!(sanitize_label("launch demo/2.smx"), "launch-demo-2-smx");
        assert_eq!(sanitize_label("sample"), "sample");
    }
}
