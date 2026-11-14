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
    Denied,
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
    width: u32,
    height: u32,
}

/// Screens that can be recorded. Windows are listed by the capture engine once recording lands.
#[tauri::command]
pub fn list_sources<R: Runtime>(app: AppHandle<R>) -> Result<Vec<CaptureSource>, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(i, m)| CaptureSource {
            id: format!("display-{}", i + 1),
            kind: "display",
            title: m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
            width: m.size().width,
            height: m.size().height,
        })
        .collect())
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
        "record" => windows::open_record(&app),
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

#[derive(Debug, Serialize)]
pub struct ConnectResult {
    ok: bool,
    message: String,
}

/// Adds Shotmaxxing to an agent's MCP settings.
#[tauri::command]
pub fn connect_agent(agent: String) -> Result<ConnectResult, String> {
    Err(format!(
        "Connecting {agent} from the app arrives with the agent server. Until then, add the MCP settings shown on this page by hand."
    ))
}
