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
| `export_project` | Renders MP4 (H.264), HEVC or GIF, with progress, and can be cancelled. `png` saves one frame (`at`, in seconds into the recording) styled like the video |
| `prepare_take` | Opens an app, finds its window and lists its controls with ids to use as refs, plus a picture of the window |
| `perform_take` | Films a take script: Shotmaxxing moves the pointer, clicks and types at a human pace while it records, then saves the project (and exports when the script asks). `dry_run` shows the timeline first |
| `validate_take` | Checks a take script and lists any problems |

## A planned take

The best demos are planned. The agent writes the whole performance as a take script, and Shotmaxxing performs it
while it records:

- the pointer travels in gentle arcs, timed by distance and target size;
- it settles before each click, and pauses after it so viewers see the result;
- typing runs at a readable pace.

1. `prepare_take` with `{ "app": "Safari" }`. The reply lists the window's controls, such as `text_field.search` or
   `button.export`, with a picture of the window.
2. Write the script:

   ```yaml
   app: Safari
   beats:
     - click: text_field.search
     - type: "shotmaxxing.dev\n"
     - wait: { for: idle }
     - say: Open the docs
     - click: { find: Docs }
   output: { mode: video }
   ```

3. `perform_take` with `{ "script": …, "dry_run": true }` shows the timeline. Then run it without `dry_run`.
4. When the Shotmaxxing app is open, the take card asks you first, listing every step. Allow it, or allow the rest
   of that agent's session. The card then follows the take live.
5. The reply has the project, how far the take got and, for `mode: video`, the exported file.

Targets can be any of these:

- refs from `prepare_take`, checked again just before each click. A control that moved is re-aimed. One that's
  gone, disabled, covered or relabelled stops the take, which says which control it was.
- screen points;
- `{ find: "label" }`, for controls that only appear later.

## Recording while something else does the work

1. `list_sources`, to find the window to film.
2. `start_recording` with `{ "app": "Safari", "microphone": "default" }`.
3. Perform the demo: you do it, or the agent uses its own computer-use tools.
4. `stop_recording`. The reply has:
   - the project folder and its length;
   - the planned zooms;
   - any warnings, such as a microphone that wasn't allowed.
5. Optionally `cut_silences`, `clean_voice` or `style_project`.
6. `export_project` with `{ "project": "<folder>", "format": "mp4" }`. The reply has the file's path.

Anything an agent does can be changed later in the editor: open the project from the Library.

## Safety

- **Asking first.** When the Shotmaxxing app is open, an agent's take starts only after you allow it on the take
  card. You can allow the rest of that agent's session too. If nobody answers within two minutes, the take doesn't
  start. When the app isn't open, the agent's own approval prompt is the only check, and the agent is told there
  was no take card.
- **Stopping a take.** Press Stop on the take card, press Esc three times, or move the mouse. The take stops at once
  and leaves the rest to you.
- **Passwords.** A take never types into a password field. A `user_step` beat hands that part to you:
  - the take waits until you press ⌥⌘Return or Hand back on the card;
  - that stretch is cut from the video.
- **Marked input.** Every click and key a take performs is marked as the agent's in the project.
- **Limits.** One take records at a time. A recording started with `start_recording` stops by itself after
  `max_seconds`: 15 minutes unless the agent sets less.
- **Local only.** Recordings, projects and exports stay on your computer, in the folders you choose.
- **Undo.** Before replanning zooms or cutting silences, a copy of the project's edits is kept in its history.

## Building on it

`shotmaxxing mcp` speaks MCP on stdin and stdout, one JSON-RPC message per line. It supports protocol versions
2025-06-18, 2025-03-26 and 2024-11-05. Tool replies carry the result as text and as `structuredContent`.
Long calls send `notifications/progress` when the request has a `progressToken`, and stop on
`notifications/cancelled`.

The command reaches the running app through a control socket that only you can open, at
`~/Library/Application Support/dev.shotmaxxing.desktop/control.sock` (JSON, one message per line). It carries the
take card today. Set `SHOTMAXXING_CONTROL_SOCKET` to point both ends somewhere else while developing.

The same work is available from the terminal:

- `shotmaxxing record`
- `shotmaxxing export`
- `shotmaxxing info`
- `shotmaxxing zoom`
- `shotmaxxing take validate`

Add `--json` for machine-readable output.
