//! Which AI agents are on this computer, and which already have Shotmaxxing connected.
//!
//! Detection only reads: an app bundle, a command on the usual paths, or the agent's own settings folder. An agent
//! counts as connected when its MCP settings mention Shotmaxxing.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AgentStatus {
    id: &'static str,
    installed: bool,
    connected: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Apps started from the Dock get a short PATH, so the usual install folders are searched too.
fn has_command(name: &str) -> bool {
    let home = home();
    let mut dirs: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            ".local/bin",
            ".npm-global/bin",
            ".bun/bin",
            ".volta/bin",
            ".claude/local",
            ".opencode/bin",
        ]
        .iter()
        .map(|d| if d.starts_with('/') { PathBuf::from(d) } else { home.join(d) }),
    );
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd")]
    } else {
        vec![name.to_string()]
    };
    dirs.iter().any(|dir| names.iter().any(|n| dir.join(n).is_file()))
}

fn any_exists(paths: &[PathBuf]) -> bool {
    paths.iter().any(|p| p.exists())
}

fn mentions_us(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| text.contains("shotmaxxing"))
}

/// Where each agent keeps its MCP settings. Shared with the connect step so both look at the same files.
pub fn config_paths(id: &str) -> Vec<PathBuf> {
    let home = home();
    let app_support = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    };
    match id {
        "claude-code" => vec![home.join(".claude.json")],
        "codex" => vec![home.join(".codex/config.toml")],
        "claude-desktop" => vec![app_support.join("Claude/claude_desktop_config.json")],
        "cursor" => vec![home.join(".cursor/mcp.json")],
        "vscode" => vec![app_support.join("Code/User/mcp.json")],
        "windsurf" => vec![home.join(".codeium/windsurf/mcp_config.json")],
        "opencode" => {
            vec![home.join(".config/opencode/opencode.json"), home.join(".config/opencode/opencode.jsonc")]
        }
        "gemini-cli" => vec![home.join(".gemini/settings.json")],
        _ => Vec::new(),
    }
}

fn installed(id: &str) -> bool {
    let home = home();
    let apps = |name: &str| {
        vec![
            PathBuf::from(format!("/Applications/{name}.app")),
            home.join(format!("Applications/{name}.app")),
        ]
    };
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
    match id {
        "claude-code" => has_command("claude") || home.join(".claude").is_dir(),
        "codex" => has_command("codex") || home.join(".codex").is_dir(),
        "claude-desktop" => any_exists(&apps("Claude")) || local.join("AnthropicClaude").is_dir(),
        "cursor" => any_exists(&apps("Cursor")) || local.join("Programs/cursor").is_dir(),
        "vscode" => any_exists(&apps("Visual Studio Code")) || has_command("code"),
        "windsurf" => any_exists(&apps("Windsurf")) || home.join(".codeium/windsurf").is_dir(),
        "opencode" => has_command("opencode") || home.join(".config/opencode").is_dir(),
        "gemini-cli" => has_command("gemini") || home.join(".gemini").is_dir(),
        _ => false,
    }
}

pub const AGENT_IDS: [&str; 8] =
    ["claude-code", "codex", "claude-desktop", "cursor", "vscode", "windsurf", "opencode", "gemini-cli"];

/// Every supported agent with whether it is installed and connected.
#[tauri::command]
pub async fn detect_agents() -> Result<Vec<AgentStatus>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        AGENT_IDS
            .iter()
            .map(|&id| AgentStatus {
                id,
                installed: installed(id),
                connected: config_paths(id).iter().any(|p| mentions_us(p)),
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}
