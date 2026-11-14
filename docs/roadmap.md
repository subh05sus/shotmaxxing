# Roadmap

Shotmaxxing is built in milestones. Each one ships something usable on macOS and Windows.

## Milestone 0: Foundations (in progress)

- [x] Design principles and architecture
- [ ] Monorepo: Cargo workspace and pnpm workspace
- [ ] Desktop app shell (Tauri 2 + Svelte 5), with design tokens and motion
- [ ] Onboarding, Home (Library, Learn, Agents) and editor layout, using sample data
- [ ] CI on macOS and Windows

## Milestone 1: Record and play back

- [ ] macOS capture: ScreenCaptureKit → VideoToolbox HEVC → fragmented MP4
- [ ] Windows capture: Windows.Graphics.Capture → Media Foundation → fragmented MP4
- [ ] Cursor, click and key telemetry
- [ ] Microphone, system audio and camera tracks
- [ ] Recording pill, record dialog, permissions checks
- [ ] Open a recording in the editor and play it back through the compositor

## Milestone 2: Studio editor

- [ ] Compositor: background, padding, corners, shadows, aspect ratios
- [ ] Rendered cursor: size, smoothing, styles, click effects, snap, motion blur, spotlight
- [ ] Auto zoom (multi-signal, explainable) and manual zooms
- [ ] Timeline: trim, split, speed, text, arrows, blur, extra audio
- [ ] Webcam bubble
- [ ] Export presets and background export queue (MP4, GIF)
- [ ] Autosave and version history
- [ ] Styled screenshots

## Milestone 3: Agents

- [ ] Control API (Unix socket / named pipe)
- [ ] `shotmaxxing mcp` and the CLI
- [ ] Take script schema, beats compiler, performer
- [ ] Accessibility refs with fingerprints (AX, UIA)
- [ ] Consent, trust policy, take card and hand-off
- [ ] macOS virtual display
- [ ] One-click connect for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode, Gemini CLI
- [ ] Agent skill

## Milestone 4: Local AI and polish

- [ ] On-device captions, voice cleanup, silence cutting
- [ ] Learn tab guides filmed by Shotmaxxing itself
- [ ] Plugins: WASM effects and content packs
- [ ] Signed releases, auto-updates, Homebrew, winget, Microsoft Store

## Later

- 3D device scenes
- Multiple screen sources in one recording
- Windows virtual display (IddCx)
- Linux
