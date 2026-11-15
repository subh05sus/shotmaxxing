//! Commands the UI calls. Each one is small and returns plain data; the work lives in the core crates.

use serde::{Deserialize, Serialize};
use shotmaxxing_core::power::{LowPowerSetting, PowerInputs, PowerPolicy, ThermalState};
use tauri::{AppHandle, Runtime};
use tauri_plugin_opener::OpenerExt;

use crate::{platform, windows};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Screen,
    Microphone,
    Camera,
    Accessibility,
    InputMonitoring,
}

impl PermissionKind {
    pub const ALL: [Self; 5] =
        [Self::Screen, Self::Microphone, Self::Camera, Self::Accessibility, Self::InputMonitoring];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Denied,
    /// Only macOS asks before the first use.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    NotDetermined,
    /// The platform doesn't ask for this (screen capture on Windows, for example).
    NotNeeded,
}

#[derive(Debug, Serialize)]
pub struct PermissionStatus {
    kind: PermissionKind,
    state: PermissionState,
}

#[derive(Debug, Serialize)]
pub struct PlatformInfo {
    os: &'static str,
    arch: &'static str,
    family: &'static str,
}

#[tauri::command]
pub fn platform_info() -> PlatformInfo {
    PlatformInfo { os: std::env::consts::OS, arch: std::env::consts::ARCH, family: std::env::consts::FAMILY }
}

#[tauri::command]
pub fn permission_status() -> Vec<PermissionStatus> {
    PermissionKind::ALL
        .into_iter()
        .map(|kind| PermissionStatus { kind, state: platform::permission(kind) })
        .collect()
}

/// Asks the system for a permission. Where the system shows its own prompt, that comes first; otherwise (or after
/// a refusal) the matching settings page opens so the person can switch it on.
#[tauri::command]
pub fn open_permission_settings<R: Runtime>(app: AppHandle<R>, kind: PermissionKind) -> Result<(), String> {
    if platform::request_permission(kind) {
        return Ok(());
    }
    match platform::settings_url(kind) {
        Some(url) => app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[derive(Debug, Serialize)]
pub struct PowerStatus {
    low_power: bool,
    reason: Option<&'static str>,
    thermal: &'static str,
    on_battery: bool,
    preview_fps_cap: u32,
}

/// The power policy from the core, fed with what the system reports right now. The UI applies the person's own
/// Low power setting on top.
#[tauri::command]
pub fn power_status() -> PowerStatus {
    let reading = platform::power();
    let policy = PowerPolicy::decide(PowerInputs {
        thermal: reading.thermal,
        on_battery: reading.on_battery,
        battery_percent: reading.battery_percent,
        os_low_power: reading.os_low_power,
        setting: LowPowerSetting::Automatic,
    });
    PowerStatus {
        low_power: policy.low_power,
        reason: policy.reason.map(|r| r.describe()),
        thermal: match reading.thermal {
            ThermalState::Nominal => "nominal",
            ThermalState::Fair => "fair",
            ThermalState::Serious => "serious",
            ThermalState::Critical => "critical",
        },
        on_battery: reading.on_battery,
        preview_fps_cap: policy.preview_fps_cap,
    }
}

#[derive(Debug, Serialize)]
pub struct CaptureSource {
    id: String,
    kind: &'static str,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<String>,
    width: u32,
    height: u32,
}

/// Screens and windows that can be recorded. Windows smaller than a thumbnail, and the app's own windows, are left
/// out.
#[tauri::command]
pub async fn list_sources<R: Runtime>(app: AppHandle<R>) -> Result<Vec<CaptureSource>, String> {
    let found = tauri::async_runtime::spawn_blocking(shotmaxxing_capture::sources)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if found.displays.is_empty() && found.windows.is_empty() {
        // No capture backend on this platform yet: list the screens the window system knows about.
        let monitors = app.available_monitors().map_err(|e| e.to_string())?;
        return Ok(monitors
            .into_iter()
            .enumerate()
            .map(|(i, m)| CaptureSource {
                id: format!("display-{}", i + 1),
                kind: "display",
                title: m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
                app: None,
                width: m.size().width,
                height: m.size().height,
            })
            .collect());
    }
    let displays = found.displays.into_iter().map(|d| CaptureSource {
        id: format!("display-{}", d.id),
        kind: "display",
        title: d.name,
        app: None,
        width: d.width.round() as u32,
        height: d.height.round() as u32,
    });
    let windows = found.windows.into_iter().map(|w| CaptureSource {
        id: format!("window-{}", w.id),
        kind: "window",
        title: if w.title.is_empty() { w.app.clone() } else { w.title },
        app: Some(w.app),
        width: w.width.round() as u32,
        height: w.height.round() as u32,
    });
    Ok(displays.chain(windows).collect())
}

/// A source's picture for the picker.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePicture {
    /// A data URL: a JPEG of the source, or a parked window's app icon as a PNG.
    url: Option<String>,
    /// A window Stage Manager parked in its strip: it comes forward when it's recorded or pictured.
    parked: bool,
}

/// Small pictures of the given sources (ids from `list_sources`), keyed by id. Sources that can't be pictured are
/// left out, and the picker keeps its placeholder for them.
#[tauri::command]
pub async fn source_thumbnails(
    ids: Vec<String>,
) -> Result<std::collections::HashMap<String, SourcePicture>, String> {
    use base64::Engine;
    use shotmaxxing_capture::{Target, Thumbnail};
    tauri::async_runtime::spawn_blocking(move || {
        let parsed: Vec<(String, Target)> = ids
            .into_iter()
            .filter_map(|id| {
                let target = if let Some(rest) = id.strip_prefix("display-") {
                    Target::Display(Some(rest.parse().ok()?))
                } else {
                    Target::Window(id.strip_prefix("window-")?.parse().ok()?)
                };
                Some((id, target))
            })
            .collect();
        let targets: Vec<Target> = parsed.iter().map(|(_, t)| *t).collect();
        let pictures = shotmaxxing_capture::thumbnails(&targets, 480).map_err(|e| e.to_string())?;
        let url = |kind: &str, bytes: Vec<u8>| {
            format!("data:image/{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
        };
        Ok(parsed
            .into_iter()
            .zip(pictures)
            .filter_map(|((id, _), picture)| {
                let picture = match picture? {
                    Thumbnail::Picture(bytes) => {
                        SourcePicture { url: Some(url("jpeg", bytes)), parked: false }
                    }
                    Thumbnail::Parked { icon } => {
                        SourcePicture { url: icon.map(|bytes| url("png", bytes)), parked: true }
                    }
                };
                Some((id, picture))
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct Devices {
    microphones: Vec<shotmaxxing_capture::Device>,
    cameras: Vec<shotmaxxing_capture::Device>,
}

/// Microphones and cameras, the system default first in each list.
#[tauri::command]
pub async fn list_devices() -> Result<Devices, String> {
    tauri::async_runtime::spawn_blocking(|| Devices {
        microphones: shotmaxxing_capture::microphones(),
        cameras: shotmaxxing_capture::cameras(),
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_window<R: Runtime>(
    app: AppHandle<R>,
    kind: String,
    target: Option<String>,
) -> Result<(), String> {
    let result = match kind.as_str() {
        "home" => windows::open_home(&app, target.as_deref().map(|t| t.trim_start_matches('/'))),
        "editor" => windows::open_editor(&app, target.as_deref().unwrap_or("sample")),
        "record" => windows::open_record(&app, target.as_deref()),
        "onboarding" => windows::open_onboarding(&app),
        "pill" => windows::open_pill(&app),
        "take" => windows::open_take_card(&app),
        other => return Err(format!("There is no “{other}” window.")),
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_in_folder<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// Opens a web page in the default browser. Only `https` links, so the UI can't be made to open anything else.
#[tauri::command]
pub fn open_link<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Only web links can be opened.".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}
