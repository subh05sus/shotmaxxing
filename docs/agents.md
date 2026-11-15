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

The Shotmaxxing app does everything that takes the system's permissions: it lists the windows, looks at an app before
a take, records, and moves the pointer and types during a take. It uses its own permissions (Screen Recording,
Accessibility, and the microphone and camera when a take records them), the ones you gave it during setup. Your
terminal or agent app needs none.

When an agent needs the app and it isn't running, `shotmaxxing mcp` opens it, behind the agent's window.

You decide what an agent may do, in the app:

- **Once per session**, before Shotmaxxing shows an agent your screen or records it for the first time, the take card
  asks you. The answer lasts until that agent disconnects.
- **Every take** asks you again on the take card, listing every step.

The card says who is asking as the system reports it, not as the agent describes itself: the agent's program, the
app it runs in and, on macOS, who signed that app (for example "From claude in Terminal · signed by Apple"). A
warning shows when the request didn't come from the `shotmaxxing` command, or when the app around it isn't signed.

## Tools

| Tool | What it does |
| --- | --- |
| `list_sources` | The screens and windows that can be recorded, with ids and sizes |
| `list_devices` | Microphones and cameras |
| `start_recording` | Records a display, a window or an app's largest window into a new project. Can add the computer's sound, a microphone and a camera |
| `recording_status` | Whether the recording this session started is still running, and for how long |
| `stop_recording` | Stops it, plans automatic zooms and reports what the project holds, including any warnings |
| `project_info` | A project's length, size, zooms, tracks, cuts, voice settings and canvas |
| `cut_silences` | Shortens pauses over a second where nothing happens on screen |
| `clean_voice` | Turns down hum, fan noise and hiss in the microphone, and evens out its level |
| `set_music` | Puts a music file under the whole video (through cuts, fading out at the end), changes its volume or start, or takes it out |
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
4. The take card asks you first, listing every step. Once you allow it, the card follows the take live.
5. The reply has the project, how far the take got and, for `mode: video`, the exported file. A finished take opens
   in the editor unless the script says `mode: video` or `mode: project`.

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
5. Optionally `cut_silences`, `clean_voice`, `style_project` or `set_music`.
6. `export_project` with `{ "project": "<folder>", "format": "mp4" }`. The reply has the file's path.

Anything an agent does can be changed later in the editor: open the project from the Library.

## Safety

- **Asking first.** Shotmaxxing shows an agent your screen or records it only after you allow that agent's session,
  and films a take only after you allow that take, both on the take card. If nobody answers within two minutes, it
  doesn't happen. An agent that calls a request off takes the question off the card too.
- **Stopping a take.** Press Stop on the take card, press Esc three times, or move the mouse. The take stops at once
  and leaves the rest to you.
- **Pausing or taking over.** Pause on the take card holds the take once the step in progress ends, until you press
  Resume. Take over hands you control the same way, until you press Hand back (or ⌥⌘Return, Ctrl+Alt+Enter on
  Windows). The time on hold is cut from the video, and the agent hears about it.
- **Passwords.** A take never types into a password field. A `user_step` beat hands that part to you:
  - the take waits until you press ⌥⌘Return (Ctrl+Alt+Enter on Windows) or Hand back on the card;
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

The command reaches the running app through a control socket that only you can open (JSON, one message per line):
`~/Library/Application Support/dev.shotmaxxing.desktop/control.sock` on macOS, and on Windows the local named pipe
`\\.\pipe\dev.shotmaxxing.desktop.control.<your SID>`, which the command only talks to when your own account serves
it. Each request opens a connection, says hello with the protocol version, and asks one thing. The app learns which
process connected from the system (on macOS from the socket, on Windows from the pipe), so the take card can name it.
Set `SHOTMAXXING_CONTROL_SOCKET` to point both ends somewhere else while developing (on Windows, another pipe name);
the command then never opens the app, it only talks to one already listening there.

The same work is available from the terminal, where it runs with the terminal's own permissions:

- `shotmaxxing record`
- `shotmaxxing export`
- `shotmaxxing info`
- `shotmaxxing zoom`
- `shotmaxxing take validate`
- `shotmaxxing open`, which opens a project in the app's editor (opening the app first if it isn't running)

Add `--json` for machine-readable output.
