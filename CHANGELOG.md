# Changelog

What changed in each release. Versions follow [semantic versioning](https://semver.org); until 1.0, a minor version
can change how things work.

## Unreleased

- **Content packs:** add wallpapers and looks from a JSON file in Settings → Content packs. They show in the editor's
  Background tool, a look applies its settings in one click, and projects keep a copy of the wallpaper they use.
  `shotmaxxing pack check` and `pack list` check and list packs, and agents can choose pack wallpapers.
  [How to make one](docs/content-packs.md).

## 0.1.0 (2026-11-16)

The first preview. It's built for macOS. The Windows version is written but hasn't been tested on a Windows computer
yet.

### Record

- Record a display, a window or an area, with the computer's sound, a microphone and a camera. Recordings are HEVC in
  a fragmented MP4, encoded by the Mac's media engine, so a crash loses at most two seconds.
- Window recordings keep their menus. The window's app is recorded on its display and cropped to the window, so a
  dropdown or sheet over the window shows at full size, and the crop follows the window as it moves. Menus that
  another process draws over the window, such as System Settings' panes, are added as they appear, never a password
  or sign-in sheet.
- Full-screen apps are listed from their own Spaces and recorded with their toolbars. With Stage Manager on, a parked
  window comes back to the stage first, at its own size.
- Screenshots: ⌥⇧4 picks a screen or a window for a full-resolution picture (without the pointer), opens it in the
  editor to frame and mark up, then copies it or saves a PNG. The next screenshot starts with the last one's look.
- ⌥⇧5 starts and stops a recording. While you record, a small pill shows the time, with Stop and a menu to restart
  or discard the recording; the menu-bar icon can stop it too.
- Shotmaxxing's own windows never show in a recording, and clicks on them, the pill or the menu-bar icon aren't
  recorded.

### Edit

- Auto zoom follows clicks, typing, the control in use and what changes on screen, and says why each zoom is there.
  Choose how far it zooms and whether it glides or cuts, or add, move and remove zooms by hand.
- A rendered cursor: size, smoothing, styles, click effects, snapping to clicks, a spotlight, motion blur and click
  sounds.
- The look: wallpapers, padding, corners, a shadow, and 16:9, 4:3, 1:1 or 9:16.
- A timeline to trim, split and change speed, with text, arrows and blur.
- Sound: voice cleanup and silence cutting on the computer, a voice-over recorded in the editor, and music under the
  whole video that fades out at the end.
- Captions transcribed on the computer with Whisper (a one-time 60 MB model download, checked against a pinned hash).
  Every word can be fixed, they come in two styles, they're drawn into exports exactly as previewed, and they save
  as SRT or WebVTT.
- A camera bubble, autosave and version history.
- Export to MP4 (H.264 or HEVC) or GIF with the sound mixed, or any frame as a PNG. Exports wait in a queue and render
  one at a time.

### Agents

- Connect Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode or Gemini CLI from the Agents page,
  in one click. Disconnect takes the entry out again.
- `shotmaxxing mcp`, an MCP server with 15 tools: list sources and devices, record, tidy up, style and export, and
  plan and perform takes.
- Planned takes: the agent writes a take script, and Shotmaxxing performs it at a human pace while it records, with
  curved pointer moves, settled clicks and readable typing. Controls are found through Accessibility and checked
  again just before each click.
- The person allows each agent session and each take on the take card, which names the program that's asking and
  who signed it. The card follows the take live, with Pause, Take over and Stop.
- Takes never type into password fields. Pressing Esc three times or moving the mouse stops one at once. A sign-in
  step hands control to the person until they hand it back (⌥⌘Return), and that wait is cut from the video.
- The `shotmaxxing` command: sources, devices, record, export, info, zoom, take validate, take schema, take prepare,
  take run and open, with `--json`.
- An agent skill that teaches agents the workflow ([skills/shotmaxxing](skills/shotmaxxing/SKILL.md)).

### Stays cool

- Nothing animates or redraws while the app sits idle, and there are no live previews while recording.
- Low power mode turns the work down on battery or when the computer runs warm. The recording itself is never slowed
  down.

### Privacy

- Recordings, projects and captions stay on the computer. There's no telemetry, analytics or account.
- The app goes online for two things: once a day it asks GitHub whether a newer release is out (Settings → Check for
  updates turns that off), and the captions' model downloads once, the first time you make captions.

### Windows

Recording (the screen, windows, the pointer, clicks and shortcuts, the computer's sound, microphones and the camera),
screenshots, export, captions, voice-overs, agent takes (through UI Automation and SendInput) and the control pipe are
all written, build on CI and pass its tests, but haven't been tried on a Windows computer. Expect rough edges.

### Known limitations

- The macOS app isn't notarized and the Windows installer isn't signed yet, so each system warns the first time. The
  [README](README.md#install) says how to open them.
- In a window recording, a menu that reaches past the window's edge is cut at that edge. Record the screen or an area
  to keep it whole.
- Takes use the real pointer and keyboard, so leave the computer alone while one runs. Takes on a virtual display of
  their own are planned.
- There are no automatic updates yet: the app tells you when a new version is out and links to it.
