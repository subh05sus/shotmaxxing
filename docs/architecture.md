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
│  Svelte 5 UI (system webview)            Native surfaces                                   │
│  panels · timeline · onboarding          recording pill · take card · canvas surface       │
│        │  typed commands + events               │                                          │
│        ▼                                        ▼                                          │
│  ┌──────────────────────────── Rust core ─────────────────────────────┐                    │
│  │ control API ── the one API for the UI, CLI, MCP and scripts        │                    │
│  │ project model (.smx) · timeline · undo · autosave                  │                    │
│  │ recorder ── capture → encode → fragmented MP4 + telemetry          │                    │
│  │ renderer ── the export draws the editor's scene, frame for frame   │                    │
│  │ takes ── beats compiler, performer, refs, consent, hand-off        │                    │
│  │ analysis ── auto zoom, silence, captions (on demand)               │                    │
│  └───────┬──────────────┬──────────────┬──────────────┬───────────────┘                    │
│          │ traits       │              │              │                                    │
│   capture backend  encode backend  input backend  a11y backend  display backend            │
│   mac: SCK         VideoToolbox    CGEventTap     AXUIElement   CGVirtualDisplay           │
│   win: WGC         Media Found.    LL hooks       UI Automation (IddCx, later)             │
└──────────────────────────────────────────────┬─────────────────────────────────────────────┘
                                               │ local socket / named pipe + session token
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
docs/                  architecture, design principles, schemas, guides
```

Platform code lives behind traits in each crate, under `macos/` and `windows/` modules gated by `cfg(target_os)`.
Adding Linux means adding a third module, not touching the editor.

## Performance architecture

These are the rules the code is reviewed against.

### Recording

- **Zero-copy from capture to encoder.** ScreenCaptureKit hands us `IOSurface`-backed `CVPixelBuffer`s that go
  straight into VideoToolbox. Windows.Graphics.Capture hands us D3D11 textures that go straight into a Media
  Foundation hardware encoder. Frames never touch the CPU or the webview.
- **Hardware HEVC, fragmented MP4.** Encoding runs on the media engine (Apple media engine, NVENC, Quick Sync,
  AMF), with H.264 as the fallback. Fragments are flushed every second, so a crash loses at most a second.
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
  windows of their own above its window.
- **Other Spaces.** macOS only pictures the Space that's showing, so windows elsewhere are listed by their app's
  icon, and recording one takes the Mac to its Space first: full-screen apps (each has a Space of its own) and, from
  inside a full-screen app, windows on the desktop. Which Space a window is on comes from the window server's own
  calls (SkyLight's, looked up when first needed; without them, full-screen apps are told by their shape), which
  also keep out windows on no Space: minimised, hidden, tabs behind others and helpers. Only windows macOS will go
  to are listed: an app with a window showing here stays here when it comes forward. The record dialog opens over a
  full-screen app, and the recording pill and take card show there.

### Editing and preview

- **One scene for preview and export.** The editor draws its canvas in the webview. The export draws the same scene
  (`render/src/scene.rs` mirrors the canvas maths): with Core Image on macOS, and on the CPU with tiny-skia on
  Windows. Both blend in sRGB as the browser does, so what you see is what you export, and a test holds the two
  export renderers to each other.
- **Render on demand.** The preview redraws only when the playhead moves or a property changes. When paused and
  idle, the GPU does nothing.
- **Decode with hardware.** Preview decodes through VideoToolbox / Media Foundation into GPU textures, at preview
  resolution, not source resolution.
- **Preview resolution follows the window.** A 1,000 px wide canvas renders 1,000 px wide (times the backing
  scale), and drops further during scrubbing.
- **Proxies only when needed.** Sources above 4K, or slow machines, get a low-resolution proxy generated in the
  background (plugged in, not hot).
- **The UI layer stays light.** Svelte compiles to small, direct DOM updates. The timeline is drawn on a canvas
  and virtualised. No animation runs while idle.

### Thermal and power awareness

- The core watches thermal state (`NSProcessInfo.thermalState`, Windows power and thermal APIs) and battery.
- When the machine is warm or on battery, Low power mode turns on: preview fps drops, background jobs pause and UI
  motion becomes fades. **The recording itself is never throttled.**

### Process model

- With no windows open, only the tray process runs (target: under 30 MB RAM, ~0% CPU).
- Windows are created on demand and destroyed when closed, not hidden.
- The MCP server is a mode of the same binary (`shotmaxxing mcp`), not a separate Node runtime.

## The project format: `.smx`

A project is a folder bundle (macOS shows it as one file; Windows shows it as a folder with an icon):

```
Launch demo.smx/
├ project.json      timeline, styles, zooms, edits (versioned schema)
├ media/            screen.mp4, camera.mp4, mic.m4a, system.m4a (fragmented MP4 / M4A)
├ telemetry/        cursor.bin (compact samples), events.jsonl (clicks, keys, focus, screen changes)
├ cache/            thumbnails, scrub sprites, waveforms (safe to delete)
└ history/          snapshots for Version history
```

- `project.json` is the only file that changes during editing. Saves are atomic (write a temp file, then rename).
- The schema is versioned and documented in `docs/schemas/`, so agents and scripts can read and edit projects
  directly.

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
- **Background first.** Accessibility actions (`AXPress`, UIA Invoke) run without touching the user's mouse. Keys
  are delivered to the target process. On macOS the take runs on a virtual display, so the user can keep working.
- **The app does the work, after asking.** The command sends anything that takes the system's permissions to the
  running app (opening it when needed), which does it with its own: listing windows, preparing, recording and
  performing. It asks the person on the take card once per agent session before it sees or records the screen, and
  before every take. The card names the caller from the connection itself: the peer process (the socket's
  `LOCAL_PEERPID` on macOS, the pipe's client process on Windows), the agent that started it, the app that runs in
  and that app's code signature. On macOS a signer is named only when Apple vouches for it (Apple's own code, the
  Mac App Store or a Developer ID, never a self-signed or development certificate, which anyone can get in any name);
  on Windows when the program's Authenticode signature, its own or a system catalog's, checks out to a trusted root.
  Typing into password fields and deleting files outside projects are always blocked. A Stop control is always on
  screen, and pressing Esc three times aborts any take.
- **Hand-off.** When a step needs a person (signing in, a CAPTCHA), the take pauses and the take card asks the
  user. Only the user's physical "Hand back" resumes it.

Three modes share this machinery: **planned takes** (best quality), **step mode** (act, look, act, with pauses cut
out afterwards) and **computer-use mode** (record an agent's own computer-use actions, then clean them up).

## Editor intelligence

- **Auto zoom** combines clicks, typing, accessibility focus (the bounds of the control in use), screen-change
  regions and cursor dwell. It frames the whole control or changed region, not just the pointer, merges nearby
  activity and records a reason for every zoom ("Typing in Search · 2.1x · 1.8 s"). Only what happens on the
  recorded area counts: a click beside a recorded window, typing into another app, or a moment too near the end to
  zoom into (the click that stops the take) makes no zoom, and clicks on Shotmaxxing's own windows, which are never
  in the video, aren't recorded at all.
- **Cursor** is redrawn from telemetry: size, smoothing, native styles, snap-to-click, click effects, motion blur,
  arcs, loop and spotlight.
- **Local AI helpers** (captions with Whisper, voice cleanup, silence cutting) run on device, only when asked, and
  download their models on first use.

## Plugins

- **Effect plugins:** WebAssembly modules plus WGSL shaders, run sandboxed inside the compositor.
- **Content packs:** declarative bundles of wallpapers, cursor sets, device frames, presets and export targets.

No plugin gets unrestricted access to the system.

## Privacy

Nothing leaves the machine. There is no telemetry, analytics or account. The app's one network request downloads the
speech model, once, the first time someone asks for captions; the sound is transcribed on the computer and never
sent. Crash logs stay local; "Report a problem" pre-fills a GitHub issue that the user reviews before sending.

## Platform support

| | macOS | Windows |
| --- | --- | --- |
| Minimum | 14 Sonoma | 10 20H1 (build 19041) |
| Capture | ScreenCaptureKit | Windows.Graphics.Capture |
| Audio | ScreenCaptureKit / Core Audio | WASAPI loopback |
| Encode | VideoToolbox | Media Foundation (NVENC, Quick Sync, AMF) + software fallback |
| GPU | Metal | Direct3D 12 |
| Accessibility | AXUIElement | UI Automation |
| Isolated agent display | CGVirtualDisplay | Planned (IddCx driver) |
| Distribution | GitHub Releases, Homebrew | GitHub Releases, winget, Microsoft Store |
