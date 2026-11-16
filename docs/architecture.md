# Architecture

Shotmaxxing is a screen recorder and studio editor that a person or an AI agent can drive. This document explains
how it fits together and why. Design rules for the interface live in [design/principles.md](design/principles.md).

## Goals that shape everything

1. **Never run hot.** Recording a demo must not make a laptop's fans spin. Idle costs nothing.
2. **Mac and Windows from day one,** behind interfaces that let Linux be added later.
3. **Agent-native.** Everything the interface can do, the control API can do, and the interface uses that same API.
4. **Editable to the end.** The cursor, zooms, frame and captions are rebuilt from data at render time, never baked
   into the recording.

## The big picture

```
┌──────────────────────────────── Shotmaxxing app (Tauri 2) ────────────────────────────────┐
│                                                                                            │
│  Svelte 5 UI in the system webview                                                         │
│  Home · editor · record dialog · recording pill · take card                                │
│        │  typed commands + events                                                          │
│        ▼                                                                                   │
│  ┌──────────────────────────── Rust core ─────────────────────────────┐                    │
│  │ control API ── the one API for the UI, CLI, MCP and scripts        │                    │
│  │ project model (.smx) · timeline · autosave · version history       │                    │
│  │ recorder ── capture → encode → fragmented MP4 + telemetry          │                    │
│  │ renderer ── the export draws the editor's scene, frame for frame   │                    │
│  │ takes ── beats compiler, performer, refs, consent, hand-off        │                    │
│  │ analysis ── auto zoom, silence, captions (on demand)               │                    │
│  └───────┬──────────────┬──────────────┬──────────────┬───────────────┘                    │
│          │              │              │              │                                    │
│   capture backend  encode backend  input backend  a11y backend  display backend (planned)  │
│   mac: SCK         VideoToolbox    CGEventTap     AXUIElement   CGVirtualDisplay           │
│   win: WGC         Media Found.    LL hooks       UI Automation IddCx                      │
└──────────────────────────────────────────────┬─────────────────────────────────────────────┘
                                               │ local socket / named pipe, this user's only
                     ┌─────────────────────────┼─────────────────────────┐
                shotmaxxing mcp          shotmaxxing CLI            your scripts / CI
                (stdio, for agents)      (take run demo.yaml)
```

## Repository layout

```
apps/desktop/          Tauri 2 app: Svelte 5 UI + the Tauri shell (src-tauri)
crates/
  core/                project model, timeline, take scripts, telemetry, auto zoom, caption rows and subtitle
                       files, shared types
  capture/             recording and input telemetry: macos/ (ScreenCaptureKit, AVAssetWriter),
                       win/ (Windows.Graphics.Capture, Media Foundation, low-level input hooks)
  render/              export: the scene and sprites, Core Image on macOS, the portable CPU compositor and
                       Media Foundation on Windows, the sound mix and voice tools
  captions/            on-device speech to text with whisper.cpp: the speech model's one-time download, and words
                       timed and grouped into captions
  control/             the control API between the command and the app (private socket or named pipe)
  agent/               the take performer: refs, human-paced pointer and typing, hand-off
  cli/                 the `shotmaxxing` binary: CLI subcommands and `shotmaxxing mcp`
packages/ui/           design tokens, motion and shared Svelte components
skills/shotmaxxing/    the agent skill
scripts/               the design guard, the icon and CLI sidecar builds, the docs' screenshots
docs/                  architecture, agents, roadmap, design principles, and the screenshots (images/)
```

Platform code lives in `macos` and `win` modules in each crate, chosen by `cfg(target_os)` behind the same
functions. Adding Linux means adding a third module, not touching the editor.

## Performance architecture

These are the rules the code is reviewed against.

### Recording

- **Zero-copy from capture to encoder.** ScreenCaptureKit hands us `IOSurface`-backed `CVPixelBuffer`s that go
  straight into VideoToolbox. Windows.Graphics.Capture hands us D3D11 textures that go straight into a Media
  Foundation hardware encoder. Frames never touch the CPU or the webview.
- **Hardware encoding, fragmented MP4.** Recordings are HEVC on macOS, from the Apple media engine, and H.264 on
  Windows, from Media Foundation's hardware encoder (NVENC, Quick Sync or AMF). Fragments are flushed every two
  seconds, so a crash loses at most two seconds.
- **Adaptive frame rate.** Capture delivers frames only when pixels change (dirty rects), up to 60 fps. A still
  screen costs almost nothing, and export resamples to a constant rate.
- **The real cursor is excluded from capture.** Cursor position, shape and clicks are recorded as compact telemetry
  (a few KB per minute) and drawn at render time.
- **Nothing else runs while recording.** No live preview, no analysis, no thumbnails. The only UI is a small native
  pill. Analysis (auto zoom, silence detection) runs after stopping and takes about a second.
- **Capture at the right size.** We capture at the display's native pixel size and never upscale. For window
  recordings, only the window's pixels are encoded: its app's windows on the display, cropped to the window (the
  crop follows it), so a menu that opens over it shows at full size rather than shrinking the picture. A take
  starts only once the window is at its own size: with Stage Manager on, macOS reports a window smaller than it is
  while it's parked or coming back to the stage, so the take waits until the size the app gives (through
  Accessibility, when allowed) is the one on screen. A full-screen app is recorded with its toolbars, which can be
  windows of their own above its window. A menu that another process draws over the window is added as it appears
  (System Settings' panes, for one, are app extensions), never a password or sign-in sheet. A menu that reaches past
  the window's edge is cut at that edge: record the screen or an area to keep it whole.
- **Other Spaces.** macOS only pictures the Space that's showing, so windows elsewhere are listed by their app's
  icon, and recording one takes the Mac to its Space first: full-screen apps (each has a Space of its own) and, from
  inside a full-screen app, windows on the desktop. The window server is asked for that window rather than its app's
  front window, and its app, through Accessibility when allowed, raises it: an app with windows on two Spaces would
  otherwise take the Mac to its front window's. Which Space a window is on comes from the window server's own
  calls (SkyLight's, looked up when first needed; without them, full-screen apps are told by their shape), which
  also keep out windows on no Space: minimised, hidden, tabs behind others and helpers. Only windows macOS will go
  to are listed: an app with a window showing here stays here when it comes forward. The record dialog opens over a
  full-screen app, and the recording pill and take card show there.

### Editing and preview

- **One scene for preview and export.** The editor composes its preview in the webview, from the recording's video
  and CSS (transforms, corners, shadows). The export draws the same scene (`render/src/scene.rs` mirrors the
  editor's maths): with Core Image on macOS, and on the CPU with tiny-skia on Windows. Both blend in sRGB as the
  browser does, so what you see is what you export, and a test holds the two export renderers to each other.
- **Render on demand.** The preview changes only when the playhead moves or a property changes. When paused and
  idle, nothing redraws.
- **Decode with hardware.** The preview plays the recording with the webview's own video player, which decodes with
  VideoToolbox on macOS and Media Foundation on Windows.
- **No proxies yet.** Large sources play at full size. The power policy already has a switch for proxies, so that
  when they come they're made only while plugged in and cool.
- **The UI layer stays light.** Svelte compiles to small, direct DOM updates, and no animation runs while idle.

### Thermal and power awareness

- The core watches thermal state (`NSProcessInfo.thermalState`, Windows power and thermal APIs) and battery.
- When the machine is warm or on battery, Low power mode turns on: preview fps drops, background jobs pause and UI
  motion becomes fades. **The recording itself is never throttled.**

### Process model

- With no windows open, only the tray process runs (target: under 30 MB RAM, ~0% CPU).
- Windows are created on demand and destroyed when closed, not hidden.
- The windows are the Home window (Library, Learn, Agents, Settings and onboarding), one editor window per project,
  the record dialog, the recording pill and the take card. Each loads the same UI at its own hash route. `go()`
  changes pages inside a window, and sends a Home page or a project to its own window instead of turning this one
  into it. Opening a project whose window is open brings that window back to it.
- The MCP server is a mode of the same binary (`shotmaxxing mcp`), not a separate Node runtime.

## The project format: `.smx`

A project is a folder bundle (macOS shows it as one file; Windows shows it as a folder with an icon):

```
Launch demo.smx/
├ project.json      timeline, styles, zooms, edits and captions (schema_version)
├ media/            screen.mp4, camera.mp4, microphone.m4a, system-audio.m4a (fragmented MP4 / M4A),
│                   voice-overs and music added in the editor; a screenshot project has screenshot.png
├ telemetry/        cursor.smxc (compact samples), events.jsonl (clicks, keys, focus, screen changes)
├ cache/            kept for derived files (safe to delete)
└ history/          snapshots for Version history
```

- Edits change only `project.json`; a voice-over or music adds its file to `media/`. Saves are atomic (write a
  temporary file, flush it, then rename it over the old one).
- `project.json` is plain JSON with a `schema_version`, so agents and scripts can read projects directly. A newer
  version than the app knows is refused rather than guessed at.

## Agent control

### One API, many clients

The Rust core exposes a **control API** over a Unix domain socket (macOS) or a named pipe (Windows), and the system
keeps it private: the socket file is this user's alone (mode 0600), and the pipe's access list admits only this user,
from this computer, while the command checks that the pipe is served by this user before it says anything. Every
client uses it:

- the app's own UI (through Tauri commands that call the same functions),
- `shotmaxxing mcp`: a stdio MCP server for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode,
  Gemini CLI and any other MCP client,
- the `shotmaxxing` CLI, for scripts and CI,
- an agent skill file that teaches the recommended workflow.

Messages stream progress events, so an agent sees "step 3 of 8" live instead of polling.

### Takes

A **take** is a planned performance. An agent (or a person) writes a take script: YAML for people, JSON over MCP,
both validated by one JSON Schema.

```yaml
app: Safari
framing: window
beats:
  - click: { ref: toolbar.address }
  - type: "shotmaxxing.dev\n"
  - wait: { for: idle }
  - say: "Open the docs"
  - click: { find: nav.docs }
  - key: cmd+f
  - type: "auto zoom"
output: { mode: editor }
```

The flow is two calls, `prepare` then `perform`:

1. **Prepare** launches and stages the app, checks permissions, and returns accessibility refs plus a screenshot,
   so the agent can plan every beat at once.
2. **Perform** compiles the beats into a human-paced performance and plays it on one clock while recording, then
   restores everything and opens the editor (or exports).

What makes a take look human and stay safe:

- **The beats compiler.** Travel time follows Fitts's law (longer moves take longer, logarithmically). The cursor
  follows curved paths, settles before each press, and pauses after it so viewers see the result. Typing runs at a
  readable pace.
- **Ref fingerprints.** Each accessibility ref carries a fingerprint (role, label, enabled state, bounds). Right
  before performing, every ref is re-checked; a moved, covered or disabled control stops the take by name instead of
  clicking the wrong spot.
- **Real input, as a person gives it.** A take moves the real pointer and presses real keys through the system's
  input (CGEvent on macOS, SendInput on Windows), so the recording shows exactly what happened, and the person
  moving the mouse stops it. Takes on a virtual display of their own, so the person can keep working, are planned.
- **The app does the work, after asking.** The command sends anything that takes the system's permissions to the
  running app (opening it when needed), which does it with its own: listing windows, preparing, recording and
  performing. It asks the person on the take card once per agent session before it sees or records the screen, and
  before every take. The card names the caller from the connection itself: the peer process (the socket's
  `LOCAL_PEERPID` on macOS, the pipe's client process on Windows), the agent that started it, the app that runs in
  and that app's code signature. On macOS a signer is named only when Apple vouches for it (Apple's own code, the
  Mac App Store or a Developer ID, never a self-signed or development certificate, which anyone can get in any name);
  on Windows when the program's Authenticode signature, its own or a system catalog's, checks out to a trusted root,
  or, for a Store or MSIX app whose program isn't signed itself, when Windows installed its package as part of
  Windows, from the Microsoft Store, or as an organisation's line-of-business app (named by its publisher).
  Typing into password fields and deleting files outside projects are always blocked. A Stop control is always on
  screen, and pressing Esc three times aborts any take.
- **Hand-off.** When a step needs a person (signing in, say), the take pauses and the take card asks them. Only the
  person resumes it: Hand back on the card, or ⌥⌘Return (Ctrl+Alt+Enter on Windows). The wait is cut from the video.

Besides planned takes, an agent can **record while something else does the work**: the person, or the agent's own
computer-use tools. It starts and stops the recording, then tidies it up with auto zoom and silence cutting.

## Editor intelligence

- **Auto zoom** combines clicks, typing, accessibility focus (the bounds of the control in use), screen-change
  regions and cursor dwell. It frames the whole control or changed region, not just the pointer, merges nearby
  activity and records a reason for every zoom ("Typing in Search · 2.1x · 1.8 s"). The camera is fully in a
  moment before the activity and holds through it, easing in before and out after, so the picture is still while
  things happen; nearby zooms glide from one to the next, and the camera never jumps. Only what happens on the
  recorded area counts: a click beside a recorded window, typing into another app, or a moment too near the end to
  zoom into (the click that stops the take) makes no zoom, and clicks on Shotmaxxing's own windows and menu-bar
  icon, which are never in the video, aren't recorded at all.
- **Cursor** is redrawn from telemetry: size, smoothing, native styles, snap-to-click, click effects, motion blur,
  arcs, loop and spotlight.
- **Local AI helpers** (captions with Whisper, voice cleanup, silence cutting) run on device, only when asked.
  Captions download their speech model on first use; the other two need none.

## Content packs and plugins

- **Content packs** (built): one JSON file of wallpapers and looks, read and checked by `render::packs` and kept in the
  app's data folder under `packs/`. A wallpaper's CSS must use the gradient subset that both the preview (the
  webview) and the exporter (`render::wallpaper`) draw, must be opaque, and may stack 8 layers; a look sets
  Background settings within the editor's own ranges. A project that uses a pack wallpaper stores its CSS
  (`Background::Wallpaper { id, css }`), so it opens and exports the same without the pack. See
  [content-packs.md](content-packs.md).
- **Planned:** packs with cursor sets, device frames and export targets, and effect plugins as WebAssembly modules
  plus WGSL shaders, run sandboxed inside the compositor.

No pack or plugin gets unrestricted access to the system: a pack is data, checked before it's used.

## Privacy

Recordings, projects and captions never leave the machine. There is no telemetry, analytics or account. The app
makes two requests of its own: once a day it asks GitHub's API whether a newer release is out (Settings → Check for
updates turns that off), and the first time someone asks for captions it downloads the speech model from Hugging
Face, once, checked against a pinned hash. The sound is transcribed on the computer and never sent.

## Platform support

| | macOS | Windows |
| --- | --- | --- |
| Minimum | 14 Sonoma | 10 20H1 (build 19041) |
| Capture | ScreenCaptureKit | Windows.Graphics.Capture |
| Audio | ScreenCaptureKit (the computer's sound), AVFoundation (microphones) | WASAPI (loopback for the computer's sound) |
| Encode | VideoToolbox (HEVC) | Media Foundation (H.264, on the hardware encoder) |
| Export compositor | Core Image | Portable CPU compositor (tiny-skia) |
| Accessibility | AXUIElement | UI Automation |
| Isolated agent display | Planned (CGVirtualDisplay) | Planned (IddCx driver) |
| Distribution | GitHub Releases | GitHub Releases |
| Status | Preview | Written, awaiting testing on Windows |
