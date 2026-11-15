# Roadmap

Shotmaxxing is built in milestones. Each one ships something usable on macOS and Windows.

## Milestone 0: Foundations

- [x] Design principles and architecture
- [x] Monorepo: Cargo workspace and pnpm workspace
- [x] Desktop app shell (Tauri 2 + Svelte 5), with design tokens and motion
- [x] Onboarding, Home (Library, Learn, Agents) and editor layout
- [x] CI on macOS and Windows

## Milestone 1: Record and play back

- [x] macOS capture: ScreenCaptureKit → VideoToolbox HEVC → fragmented MP4
- [ ] Windows capture: Windows.Graphics.Capture → Media Foundation → fragmented MP4 (the picture, the pointer path,
      clicks, shortcuts, the computer's sound, microphones, the camera and the source picker's thumbnails are written
      and await testing on Windows)
- [x] Cursor, click and key telemetry
- [x] Microphone, system audio and camera tracks (macOS)
- [x] Recording pill, record dialog, permissions checks
- [x] Open a recording in the editor and play it back, with its sound and camera

## Milestone 2: Studio editor

- [x] Compositor: background, padding, corners, shadows, aspect ratios (Core Image on macOS, and a portable CPU
      compositor that draws the same frames)
- [x] Rendered cursor: size, smoothing, styles, click effects, snap, spotlight
- [x] Cursor motion blur and click sounds
- [x] Auto zoom (multi-signal, explainable) and manual zooms, replanned as its settings change
- [x] Timeline: trim, split, speed, text, arrows, blur
- [x] Music under the whole video: through cuts, fading out at the end, in the editor, the export and MCP
- [ ] Voice-over recorded in the editor
- [x] Webcam bubble
- [x] Export: MP4 (H.264, HEVC) and GIF, with the sound mixed
- [ ] Windows export: MP4 (with the sound mixed), GIF and PNG through Media Foundation, the portable compositor
      and the portable mixer are written and await testing on Windows
- [x] Background export queue (exports wait in line and render one at a time)
- [x] Autosave and version history
- [x] Styled stills: any frame as a PNG, from the editor, the CLI and MCP
- [x] Screenshot mode: ⌥⇧4 or the record dialog takes a full-resolution picture of a screen or a window (no
      pointer), opens it in the editor to frame and mark up, and copies it at full resolution or saves a PNG. The
      next screenshot starts with the last one's look. macOS (ScreenCaptureKit) and Windows (GDI, PrintWindow; awaits
      testing on Windows)

## Milestone 3: Agents

- [x] Control API for the running app (Unix socket on macOS and Linux)
- [x] Control API on Windows (a named pipe only this user can open)
- [x] `shotmaxxing mcp` and the CLI
- [x] Take script schema and checks
- [x] Beats compiler and performer (macOS)
- [x] Accessibility refs with fingerprints (macOS)
- [ ] Takes on Windows: UI Automation refs, SendInput, the same safety stops and hand-off (Ctrl+Alt+Enter), written
      and awaiting testing on Windows
- [x] Safety stops (Esc three times, the person moving the mouse, password fields) and hand-off (⌥⌘Return)
- [x] Take card in the app, and a per-session consent prompt
- [x] Agents' takes, recordings and looks run in the app with its own permissions, after the person allows them;
      the take card names the caller as the system reports it (peer process, host app, macOS code signature)
- [ ] Check the caller's app signature on Windows too (including Store apps' catalog signatures)
- [ ] macOS virtual display
- [x] One-click connect for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode, Gemini CLI
- [x] Agent skill ([skills/shotmaxxing](../skills/shotmaxxing/SKILL.md))

## Milestone 4: Local AI and polish

- [x] Voice cleanup and silence cutting, on device
- [ ] On-device captions
- [ ] Learn tab guides filmed by Shotmaxxing itself
- [ ] Plugins: WASM effects and content packs
- [ ] Signed releases, auto-updates, Homebrew, winget, Microsoft Store

## Later

- 3D device scenes
- Multiple screen sources in one recording
- Windows virtual display (IddCx)
- Linux
