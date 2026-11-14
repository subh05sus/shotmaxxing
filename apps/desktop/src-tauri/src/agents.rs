//! Which AI agents are on this computer, and connecting Shotmaxxing to them.
//!
//! Detection only reads: an app bundle, a command on the usual paths, or the agent's own settings. Connecting adds
//! one MCP server called `shotmaxxing` to the agent's settings, in that agent's format, and changes nothing else.
//! Claude Code adds it itself, through `claude mcp add`. For the others the settings file is read, the entry is
//! added with every other setting kept (in its order, with its indentation), the old file stays beside it as a
//! backup, and the new one replaces it in a single step. A file with comments isn't rewritten: the person gets the
//! lines to add by hand.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value, json};
use toml_edit::{DocumentMut, Item};

/// The name agents know Shotmaxxing by.
const NAME: &str = "shotmaxxing";

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

/// PATH plus the usual install folders: apps started from the Dock get a short PATH.
fn search_dirs() -> Vec<PathBuf> {
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
    dirs
}

/// Where a command lives, looking on PATH and in the usual install folders.
fn find_command(name: &str) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd")]
    } else {
        vec![name.to_string()]
    };
    search_dirs().into_iter().flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

fn has_command(name: &str) -> bool {
    find_command(name).is_some()
}

fn any_exists(paths: &[PathBuf]) -> bool {
    paths.iter().any(|p| p.exists())
}

/// Where each agent keeps its MCP settings, the usual one first.
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

fn display_name(id: &str) -> &str {
    match id {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "claude-desktop" => "Claude Desktop",
        "cursor" => "Cursor",
        "vscode" => "VS Code",
        "windsurf" => "Windsurf",
        "opencode" => "OpenCode",
        "gemini-cli" => "Gemini CLI",
        other => other,
    }
}

/// How an agent lists its MCP servers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    /// `"mcpServers": { "shotmaxxing": { "command", "args" } }`: Claude Desktop, Cursor, Windsurf, Gemini CLI.
    McpServers,
    /// Claude Code's `~/.claude.json`: the same, with `"type": "stdio"`.
    ClaudeCode,
    /// VS Code: `"servers": { "shotmaxxing": { "type": "stdio", … } }`.
    VsCode,
    /// OpenCode: `"mcp": { "shotmaxxing": { "type": "local", "command": [path, "mcp"] } }`.
    OpenCode,
    /// Codex: `[mcp_servers.shotmaxxing]` in TOML.
    Codex,
}

fn style(id: &str) -> Option<Style> {
    Some(match id {
        "claude-code" => Style::ClaudeCode,
        "codex" => Style::Codex,
        "claude-desktop" | "cursor" | "windsurf" | "gemini-cli" => Style::McpServers,
        "vscode" => Style::VsCode,
        "opencode" => Style::OpenCode,
        _ => return None,
    })
}

impl Style {
    /// The key that holds the servers.
    fn key(self) -> &'static str {
        match self {
            Self::McpServers | Self::ClaudeCode => "mcpServers",
            Self::VsCode => "servers",
            Self::OpenCode => "mcp",
            Self::Codex => "mcp_servers",
        }
    }

    /// Shotmaxxing's entry in a JSON style.
    fn entry(self, command: &str) -> Value {
        match self {
            Self::McpServers | Self::Codex => json!({ "command": command, "args": ["mcp"] }),
            Self::ClaudeCode => json!({ "type": "stdio", "command": command, "args": ["mcp"], "env": {} }),
            Self::VsCode => json!({ "type": "stdio", "command": command, "args": ["mcp"] }),
            Self::OpenCode => json!({ "type": "local", "command": [command, "mcp"], "enabled": true }),
        }
    }
}

/// The settings can't be changed safely here (comments, trailing commas, an unusual layout): it's done by hand.
#[derive(Debug, PartialEq, Eq)]
struct ByHand;

fn runs_us(command: &str) -> bool {
    Path::new(command).file_stem().is_some_and(|stem| stem == NAME)
}

/// Whether a server entry is Shotmaxxing, by its name or by the command it starts.
fn is_ours(name: &str, entry: &Value) -> bool {
    name == NAME
        || match entry.get("command") {
            Some(Value::String(command)) => runs_us(command),
            Some(Value::Array(parts)) => parts.first().and_then(Value::as_str).is_some_and(runs_us),
            _ => false,
        }
}

fn parse_object(text: &str) -> Result<Map<String, Value>, ByHand> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(ByHand),
    }
}

/// JSON laid out like `original`: the same indent, and a newline at the end.
fn pretty(value: &Value, original: &str) -> String {
    let indent = original
        .lines()
        .skip(1)
        .map(|line| &line[..line.len() - line.trim_start_matches([' ', '\t']).len()])
        .find(|space| !space.is_empty())
        .unwrap_or("  ");
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    // Writing a JSON value to memory can't fail.
    let _ = value.serialize(&mut serializer);
    let mut text = String::from_utf8(out).unwrap_or_default();
    text.push('\n');
    text
}

/// The names of the Codex servers that are Shotmaxxing.
fn ours_in_toml(doc: &DocumentMut) -> Vec<String> {
    let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table_like) else { return Vec::new() };
    servers
        .iter()
        .filter(|(name, item)| {
            *name == NAME || item.get("command").and_then(Item::as_str).is_some_and(runs_us)
        })
        .map(|(name, _)| name.to_string())
        .collect()
}

/// `text` (empty for a new file) with Shotmaxxing's entry added, or brought up to date if it's there already.
fn with_entry(style: Style, text: &str, command: &str) -> Result<String, ByHand> {
    if style == Style::Codex {
        let mut doc: DocumentMut = text.parse().map_err(|_| ByHand)?;
        let servers = doc.entry("mcp_servers").or_insert_with(|| {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        let servers = servers.as_table_mut().ok_or(ByHand)?;
        let entry = servers.entry(NAME).or_insert_with(|| Item::Table(toml_edit::Table::new()));
        let entry = entry.as_table_like_mut().ok_or(ByHand)?;
        entry.insert("command", toml_edit::value(command));
        entry.insert("args", toml_edit::value(toml_edit::Array::from_iter(["mcp"])));
        return Ok(doc.to_string());
    }
    let mut root = parse_object(text)?;
    let servers = root.entry(style.key()).or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or(ByHand)?;
    let fresh = style.entry(command);
    match servers.get_mut(NAME) {
        // Settings the person added to the entry, such as environment variables, stay.
        Some(Value::Object(existing)) => {
            for (key, value) in fresh.as_object().into_iter().flatten() {
                existing.insert(key.clone(), value.clone());
            }
        }
        _ => {
            servers.insert(NAME.to_string(), fresh);
        }
    }
    Ok(pretty(&Value::Object(root), text))
}

/// `text` without the entries that start Shotmaxxing, or `None` when it has none.
fn without_entry(style: Style, text: &str) -> Result<Option<String>, ByHand> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    if style == Style::Codex {
        let mut doc: DocumentMut = text.parse().map_err(|_| ByHand)?;
        let ours = ours_in_toml(&doc);
        let Some(servers) = doc.get_mut("mcp_servers").and_then(Item::as_table_like_mut) else {
            return Ok(None);
        };
        if ours.is_empty() {
            return Ok(None);
        }
        for name in &ours {
            servers.remove(name);
        }
        return Ok(Some(doc.to_string()));
    }
    let mut root = parse_object(text)?;
    let Some(Value::Object(servers)) = root.get_mut(style.key()) else { return Ok(None) };
    let ours: Vec<String> = servers.iter().filter(|(k, v)| is_ours(k, v)).map(|(k, _)| k.clone()).collect();
    if ours.is_empty() {
        return Ok(None);
    }
    for name in &ours {
        servers.shift_remove(name);
    }
    Ok(Some(pretty(&Value::Object(root), text)))
}

/// Whether the settings start Shotmaxxing. A path that merely mentions it (a project folder) doesn't count.
fn has_entry(style: Style, text: &str) -> bool {
    if style == Style::Codex {
        return match text.parse::<DocumentMut>() {
            Ok(doc) => !ours_in_toml(&doc).is_empty(),
            Err(_) => text.contains("mcp_servers.shotmaxxing"),
        };
    }
    match parse_object(text) {
        Ok(root) => root
            .get(style.key())
            .and_then(Value::as_object)
            .is_some_and(|servers| servers.iter().any(|(k, v)| is_ours(k, v))),
        // With comments the file can't be read as JSON; a quoted key is a fair sign.
        Err(ByHand) => text.contains("\"shotmaxxing\""),
    }
}

/// Why a settings file wasn't changed.
#[derive(Debug)]
enum Problem {
    ByHand,
    Io(String),
}

/// Changes a settings file: `edit` gets its text (empty for a new file) and returns the new text, or `None` to
/// leave it be. The previous file is kept as `<name>.shotmaxxing-backup`, and the new one replaces it in one step.
/// Returns whether anything changed.
fn update_file(
    path: &Path,
    edit: impl FnOnce(&str) -> Result<Option<String>, ByHand>,
) -> Result<bool, Problem> {
    // Settings kept in a dotfiles folder are often links: the file itself changes, and the link stays.
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Problem::Io(format!("Couldn’t read {}: {e}", path.display()))),
    };
    let Some(new) = edit(text.trim_start_matches('\u{feff}')).map_err(|ByHand| Problem::ByHand)? else {
        return Ok(false);
    };
    let io = |e: std::io::Error| Problem::Io(format!("Couldn’t save {}: {e}", path.display()));
    let (Some(dir), Some(file)) = (path.parent(), path.file_name()) else {
        return Err(Problem::Io(format!("{} isn't a file", path.display())));
    };
    let file = file.to_string_lossy();
    std::fs::create_dir_all(dir).map_err(io)?;
    let existing = std::fs::metadata(&path).ok();
    if existing.is_some() {
        // A copy keeps the original's permissions: some of these files hold sign-in details.
        std::fs::copy(&path, dir.join(format!("{file}.shotmaxxing-backup"))).map_err(io)?;
    }
    let partial = dir.join(format!(".{file}.shotmaxxing-partial"));
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        std::io::Write::write_all(&mut options.open(&partial)?, new.as_bytes())?;
        if let Some(meta) = &existing {
            std::fs::set_permissions(&partial, meta.permissions())?;
        }
        std::fs::rename(&partial, &path)
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_file(&partial);
        return Err(io(error));
    }
    Ok(true)
}

/// The `shotmaxxing` command agents start: the one that comes with the app, else one on the usual paths.
fn cli_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "shotmaxxing.exe" } else { NAME };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .filter(|path| path.is_file())
        .or_else(|| find_command(NAME))
}

/// Runs an agent's own command line tool for up to 30 seconds, and says whether it worked. Some are scripts that
/// need `node`, which isn't on the PATH of apps started from the Dock, so the usual folders are added.
fn run(program: &Path, args: &[&str]) -> Option<bool> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Ok(path) = std::env::join_paths(search_dirs()) {
        command.env("PATH", path);
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ConnectResult {
    ok: bool,
    message: String,
    /// When the settings have to be changed by hand: the file, and the lines to add.
    #[serde(skip_serializing_if = "Option::is_none")]
    manual: Option<Manual>,
}

#[derive(Debug, Serialize)]
pub struct Manual {
    path: PathBuf,
    snippet: String,
}

/// The settings file to change: the one that exists, or the usual one.
fn settings_file(id: &str) -> Option<PathBuf> {
    let paths = config_paths(id);
    paths.iter().find(|p| p.exists()).or(paths.first()).cloned()
}

fn connect(id: &str) -> Result<ConnectResult, String> {
    let style = style(id).ok_or_else(|| format!("Shotmaxxing can’t connect {id} yet."))?;
    let name = display_name(id);
    let command = cli_path()
        .ok_or("Couldn’t find the shotmaxxing command that comes with the app. Reinstall Shotmaxxing to get it back.")?
        .to_string_lossy()
        .into_owned();
    if style == Style::ClaudeCode
        && let Some(claude) = find_command("claude")
    {
        // Removed first, so a new path replaces an old one.
        let _ = run(&claude, &["mcp", "remove", "--scope", "user", NAME]);
        if run(&claude, &["mcp", "add", "--scope", "user", NAME, "--", &command, "mcp"]) == Some(true) {
            return Ok(ConnectResult { ok: true, message: format!("Connected {name}."), manual: None });
        }
        // An older Claude Code: its settings file is changed directly instead.
    }
    let path = settings_file(id).ok_or_else(|| format!("Couldn’t tell where {name} keeps its settings."))?;
    match update_file(&path, |text| with_entry(style, text, &command).map(Some)) {
        Ok(_) => Ok(ConnectResult { ok: true, message: format!("Connected {name}."), manual: None }),
        Err(Problem::ByHand) => Ok(ConnectResult {
            ok: false,
            message: format!(
                "{name}’s settings have comments or an unusual layout, so Shotmaxxing left them alone. Copy the lines and add them to {} yourself.",
                path.display()
            ),
            manual: Some(Manual { snippet: with_entry(style, "", &command).unwrap_or_default(), path }),
        }),
        Err(Problem::Io(message)) => Err(message),
    }
}

fn disconnect(id: &str) -> Result<ConnectResult, String> {
    let style = style(id).ok_or_else(|| format!("Shotmaxxing can’t disconnect {id}."))?;
    let name = display_name(id);
    if style == Style::ClaudeCode
        && let Some(claude) = find_command("claude")
    {
        let _ = run(&claude, &["mcp", "remove", "--scope", "user", NAME]);
    }
    // Anything still starting Shotmaxxing, under any name, goes too.
    for path in config_paths(id).iter().filter(|p| p.exists()) {
        match update_file(path, |text| without_entry(style, text)) {
            Ok(_) => {}
            Err(Problem::ByHand) => {
                return Ok(ConnectResult {
                    ok: false,
                    message: format!(
                        "{name}’s settings have comments or an unusual layout, so Shotmaxxing left them alone. Remove the shotmaxxing entry from {} yourself.",
                        path.display()
                    ),
                    manual: Some(Manual { path: path.clone(), snippet: String::new() }),
                });
            }
            Err(Problem::Io(message)) => return Err(message),
        }
    }
    Ok(ConnectResult { ok: true, message: format!("Disconnected {name}."), manual: None })
}

/// Every supported agent with whether it is installed and connected.
#[tauri::command]
pub async fn detect_agents() -> Result<Vec<AgentStatus>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        AGENT_IDS
            .iter()
            .map(|&id| AgentStatus {
                id,
                installed: installed(id),
                connected: style(id).is_some_and(|style| {
                    config_paths(id)
                        .iter()
                        .any(|p| std::fs::read_to_string(p).is_ok_and(|text| has_entry(style, &text)))
                }),
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Adds Shotmaxxing to an agent's MCP settings, or brings its entry up to date.
#[tauri::command]
pub async fn connect_agent(agent: String) -> Result<ConnectResult, String> {
    tauri::async_runtime::spawn_blocking(move || connect(&agent)).await.map_err(|e| e.to_string())?
}

/// Takes Shotmaxxing out of an agent's MCP settings.
#[tauri::command]
pub async fn disconnect_agent(agent: String) -> Result<ConnectResult, String> {
    tauri::async_runtime::spawn_blocking(move || disconnect(&agent)).await.map_err(|e| e.to_string())?
}

/// The command agents run to reach Shotmaxxing, for setting one up by hand.
#[tauri::command]
pub fn mcp_command() -> String {
    cli_path().map_or_else(|| NAME.to_string(), |path| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CMD: &str = "/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing";

    #[test]
    fn adds_the_entry_and_keeps_everything_else_as_it_was() {
        let before = "{\n    \"theme\": \"dark\",\n    \"mcpServers\": {\n        \"other\": { \"command\": \"npx\" }\n    },\n    \"a\": 1\n}";
        let after = with_entry(Style::McpServers, before, CMD).unwrap();
        let root: Map<String, Value> = serde_json::from_str(&after).unwrap();
        assert_eq!(root.keys().collect::<Vec<_>>(), ["theme", "mcpServers", "a"], "order kept");
        assert_eq!(root["mcpServers"]["other"]["command"], "npx");
        assert_eq!(root["mcpServers"]["shotmaxxing"], json!({ "command": CMD, "args": ["mcp"] }));
        assert!(after.starts_with("{\n    \"theme\""), "four-space indent kept:\n{after}");
        assert!(after.ends_with("}\n"));
    }

    #[test]
    fn writes_each_agents_own_format() {
        let get =
            |style: Style| -> Value { serde_json::from_str(&with_entry(style, "", CMD).unwrap()).unwrap() };
        assert_eq!(get(Style::ClaudeCode)["mcpServers"]["shotmaxxing"]["type"], "stdio");
        assert_eq!(get(Style::VsCode)["servers"]["shotmaxxing"]["args"], json!(["mcp"]));
        assert_eq!(get(Style::OpenCode)["mcp"]["shotmaxxing"]["command"], json!([CMD, "mcp"]));
        let toml = with_entry(Style::Codex, "", CMD).unwrap();
        assert_eq!(toml, format!("[mcp_servers.shotmaxxing]\ncommand = \"{CMD}\"\nargs = [\"mcp\"]\n"));
    }

    #[test]
    fn connecting_again_updates_instead_of_adding_twice() {
        let first = with_entry(Style::McpServers, "", "/old/shotmaxxing").unwrap();
        let mut root: Value = serde_json::from_str(&first).unwrap();
        root["mcpServers"]["shotmaxxing"]["env"] = json!({ "LOG": "1" });
        let second = with_entry(Style::McpServers, &root.to_string(), CMD).unwrap();
        let root: Value = serde_json::from_str(&second).unwrap();
        let servers = root["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers["shotmaxxing"]["command"], CMD);
        assert_eq!(servers["shotmaxxing"]["env"]["LOG"], "1", "the person's own settings stay");
    }

    #[test]
    fn leaves_files_with_comments_alone() {
        let jsonc = "{\n  // mine\n  \"servers\": {}\n}";
        assert_eq!(with_entry(Style::VsCode, jsonc, CMD), Err(ByHand));
        assert_eq!(without_entry(Style::VsCode, jsonc), Err(ByHand));
        assert!(!has_entry(Style::VsCode, jsonc));
        assert!(has_entry(Style::VsCode, "{\n  // mine\n  \"servers\": { \"shotmaxxing\": {} }\n}"));
    }

    #[test]
    fn a_folder_named_shotmaxxing_isnt_a_connection() {
        let claude = r#"{"projects": {"/Users/me/code/shotmaxxing": {"mcpServers": {}}}, "mcpServers": {}}"#;
        assert!(!has_entry(Style::ClaudeCode, claude));
        let renamed =
            r#"{"mcpServers": {"smx": {"command": "/usr/local/bin/shotmaxxing", "args": ["mcp"]}}}"#;
        assert!(has_entry(Style::McpServers, renamed), "found by the command it starts");
        let after = without_entry(Style::McpServers, renamed).unwrap().unwrap();
        assert!(!has_entry(Style::McpServers, &after));
        assert_eq!(without_entry(Style::McpServers, &after), Ok(None), "nothing left to take out");
    }

    #[test]
    fn codex_settings_keep_their_comments() {
        let before = "# my settings\nmodel = \"o3\"\n\n[mcp_servers.other]\ncommand = \"npx\" # keep\n";
        let after = with_entry(Style::Codex, before, CMD).unwrap();
        assert!(after.starts_with(before), "{after}");
        assert!(after.contains("[mcp_servers.shotmaxxing]"));
        assert!(has_entry(Style::Codex, &after));
        let removed = without_entry(Style::Codex, &after).unwrap().unwrap();
        assert_eq!(removed.trim_end(), before.trim_end());
        assert_eq!(with_entry(Style::Codex, "mcp_servers = 3", CMD), Err(ByHand));
    }

    #[test]
    fn saves_through_links_and_keeps_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles/mcp.json");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "{\"mcpServers\": {}}").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&real, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        let link = dir.path().join("mcp.json");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(not(unix))]
        std::fs::copy(&real, &link).unwrap();

        let changed = update_file(&link, |text| with_entry(Style::McpServers, text, CMD).map(Some)).unwrap();
        assert!(changed);
        #[cfg(unix)]
        {
            assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "the link stays");
            let mode =
                std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&real).unwrap().permissions());
            assert_eq!(mode & 0o777, 0o600, "permissions kept");
            let backup = real.with_file_name("mcp.json.shotmaxxing-backup");
            assert_eq!(std::fs::read_to_string(backup).unwrap(), "{\"mcpServers\": {}}");
        }
        assert!(has_entry(Style::McpServers, &std::fs::read_to_string(&link).unwrap()));

        // A new file in a new folder.
        let fresh = dir.path().join("new/settings.json");
        assert!(update_file(&fresh, |text| with_entry(Style::McpServers, text, CMD).map(Some)).unwrap());
        assert!(has_entry(Style::McpServers, &std::fs::read_to_string(&fresh).unwrap()));
        // Nothing to change: the file isn't touched.
        assert!(!update_file(&fresh, |_| Ok(None)).unwrap());
        let left: Vec<_> = std::fs::read_dir(fresh.parent().unwrap()).unwrap().collect();
        assert_eq!(left.len(), 1, "no partial files left behind");
    }
}
