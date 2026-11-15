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
      clicks and shortcuts are written and await testing on Windows; sound and the camera are next)
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
- [ ] Windows export: MP4, GIF and PNG through Media Foundation and the portable compositor are written and await
      testing on Windows; the sound mix is next
- [x] Background export queue (exports wait in line and render one at a time)
- [x] Autosave and version history
- [x] Styled stills: any frame as a PNG, from the editor, the CLI and MCP
- [ ] Screenshot mode in the record dialog (a styled still without recording first)

## Milestone 3: Agents

- [x] Control API for the running app (Unix socket on macOS and Linux)
- [x] Control API on Windows (a named pipe only this user can open)
- [x] `shotmaxxing mcp` and the CLI
- [x] Take script schema and checks
- [x] Beats compiler and performer (macOS)
- [x] Accessibility refs with fingerprints (macOS)
- [ ] Accessibility refs on Windows (UI Automation)
- [x] Safety stops (Esc three times, the person moving the mouse, password fields) and hand-off (⌥⌘Return)
- [x] Take card in the app, and a per-session consent prompt
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
