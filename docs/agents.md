# Agents

AI agents work with Shotmaxxing through MCP (the Model Context Protocol). An agent connected to Shotmaxxing can
record a screen or a window, tidy the recording up and export a finished video, all on your computer.

## Connect an agent

**In the app:** open **Agents** and press **Connect** next to the agent, then start a new chat in that agent.

- Claude Code adds Shotmaxxing itself, through `claude mcp add`.
- For the other agents, Shotmaxxing adds one entry to the agent's settings file:
  - every other setting stays as it was;
  - the previous file is kept beside it as `<file>.shotmaxxing-backup`.
- A JSON settings file with comments isn't rewritten. The app offers to copy the lines so you can add them yourself.
- **Disconnect** takes the entry out again.

**By hand:** add a server that runs `shotmaxxing mcp`. The Agents page shows the command's full path on your
computer. Inside the app it is `/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing`.

| Agent | Settings | Entry |
| --- | --- | --- |
| Claude Code | `claude mcp add --scope user shotmaxxing -- <path> mcp` | |
| Codex | `~/.codex/config.toml` | `[mcp_servers.shotmaxxing]` with `command = "<path>"` and `args = ["mcp"]` |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` | `mcpServers` |
| Cursor | `~/.cursor/mcp.json` | `mcpServers` |
| VS Code | `~/Library/Application Support/Code/User/mcp.json` | `servers`, with `"type": "stdio"` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` |
| OpenCode | `~/.config/opencode/opencode.json` | `mcp`, with `"type": "local"` and `"command": ["<path>", "mcp"]` |
| Gemini CLI | `~/.gemini/settings.json` | `mcpServers` |

An `mcpServers` entry looks like this:

```json
{
  "mcpServers": {
    "shotmaxxing": { "command": "<path>", "args": ["mcp"] }
  }
}
```

## Permissions

macOS asks for Screen Recording the first time a take starts. The permission belongs to the app that started the
server:

- the terminal, for Claude Code, Codex, OpenCode and Gemini CLI;
- the agent's own app, for Claude Desktop, Cursor, VS Code and Windsurf.

The microphone and camera work the same way. Give the permission in System Settings → Privacy & Security, then quit
and reopen that app.

## Tools

| Tool | What it does |
| --- | --- |
| `list_sources` | The screens and windows that can be recorded, with ids and sizes |
| `list_devices` | Microphones and cameras |
| `start_recording` | Records a display, a window or an app's largest window into a new project. Can add the computer's sound, a microphone and a camera |
| `recording_status` | Whether a take is recording, and for how long |
| `stop_recording` | Stops the take, plans automatic zooms and reports what the project holds, including any warnings |
| `project_info` | A project's length, size, zooms, tracks, cuts, voice settings and canvas |
| `cut_silences` | Shortens pauses over a second where nothing happens on screen |
| `clean_voice` | Turns down hum, fan noise and hiss in the microphone, and evens out its level |
| `plan_zooms` | Plans the automatic zooms again, subtle, standard or bold |
| `style_project` | Wallpaper, canvas shape, padding, corners, shadow and cursor size |
| `export_project` | Renders MP4 (H.264), HEVC or GIF, with progress, and can be cancelled |
| `validate_take` | Checks a take script and lists any problems |

## A typical take

1. `list_sources`, to find the window to film.
2. `start_recording` with `{ "app": "Safari", "microphone": "default" }`.
3. Perform the demo: you do it, or the agent uses its own computer-use tools.
4. `stop_recording`. The reply has the project folder, its length, the planned zooms and any warnings, such as a
   microphone that wasn't allowed.
5. Optionally `cut_silences`, `clean_voice` or `style_project`.
6. `export_project` with `{ "project": "<folder>", "format": "mp4" }`. The reply has the file's path.

Anything an agent does can be changed later in the editor: open the project from the Library.

## Safety

- One take records at a time, and a take stops by itself after `max_seconds` (15 minutes unless the agent sets
  less).
- Recordings, projects and exports stay on your computer, in the folders you choose.
- Before replanning zooms or cutting silences, a copy of the project's edits is kept in its history.
- Shotmaxxing performing takes itself (moving the pointer and typing from a take script) is coming next. Until then,
  `validate_take` only checks scripts.

## Building on it

`shotmaxxing mcp` speaks MCP on stdin and stdout, one JSON-RPC message per line. It supports protocol versions
2025-06-18, 2025-03-26 and 2024-11-05. Tool replies carry the result as text and as `structuredContent`.
Long calls send `notifications/progress` when the request has a `progressToken`, and stop on
`notifications/cancelled`. The same work is available from the terminal:

- `shotmaxxing record`
- `shotmaxxing export`
- `shotmaxxing info`
- `shotmaxxing zoom`
- `shotmaxxing take validate`

Add `--json` for machine-readable output.
