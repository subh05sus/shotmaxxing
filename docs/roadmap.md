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
- [x] Window takes keep their menus: the window's app is recorded on its display, cropped to the window and
      following it, so a dropdown shows at full size instead of shrinking the picture; menus another process draws
      over the window (System Settings' panes are app extensions) are added as they appear, never a password or
      sign-in sheet; with Stage Manager on, a parked window comes back to the stage first, at its own size, and the
      picker shows parked windows by their app's icon
- [x] Full-screen apps (macOS): listed from their own Spaces and recorded with their toolbars; recording one goes
      to its Space, and the record dialog, pill and take card show over full-screen apps
- [ ] Windows capture: Windows.Graphics.Capture → Media Foundation → fragmented MP4 (the picture, the pointer path,
      clicks, shortcuts, the computer's sound without Shotmaxxing's own (Windows 11 and later), microphones, the
      camera and the source picker's thumbnails are written and await testing on Windows)
- [x] Cursor, click and key telemetry
- [x] Microphone, system audio and camera tracks (macOS)
- [x] Recording pill, record dialog, permissions checks
- [x] Shotmaxxing's own windows and menu-bar icon stay out of recordings, and clicks on them aren't recorded
- [x] Open a recording in the editor and play it back, with its sound and camera

## Milestone 2: Studio editor

- [x] Compositor: background, padding, corners, shadows, aspect ratios (Core Image on macOS, and a portable CPU
      compositor that draws the same frames)
- [x] Rendered cursor: size, smoothing, styles, click effects, snap, spotlight
- [x] Cursor motion blur and click sounds
- [x] Auto zoom (multi-signal, explainable) and manual zooms, replanned as its settings change
- [x] Timeline: trim, split, speed, text, arrows, blur
- [x] Music under the whole video: through cuts, fading out at the end, in the editor, the export and MCP
- [x] Voice-over recorded in the editor: the video plays silently from the playhead while the microphone records
      (AVCaptureSession on macOS, WASAPI on Windows); each take plays on the finished timeline like music, so later
      cuts don't shift it, with its own volume (to try live with a microphone)
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
      the take card names the caller as the system reports it (peer process, host app, its code signature)
- [x] Check the caller's app signature on Windows too: Authenticode, the program's own or in Windows's catalogs
- [x] Windows: check the package signature of Store and MSIX apps whose programs aren't signed themselves, by where
      Windows installed the package from (with Windows, the Microsoft Store, or an organisation, naming its
      publisher); CI reads Windows's own packaged apps and Windows Terminal from the Store
- [ ] macOS virtual display
- [x] One-click connect for Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode, Gemini CLI
- [x] Agent skill ([skills/shotmaxxing](../skills/shotmaxxing/SKILL.md))
- [x] Agents plan from the code: the server's instructions and a `plan-demo` prompt have them learn the flow from
      the app's code and show a plan in plain steps first; `check_take` and `shotmaxxing take check` test a take
      script against the screen and suggest what to try instead; the plan shows on the take card
- [x] Allow everything: one switch lets the `shotmaxxing` command's requests through without the take card asking,
      kept in memory until the app quits, with a red dot on the menu-bar icon; a red edge goes round every screen
      while a take drives (Windows: written, awaiting testing on Windows)

## Milestone 4: Local AI and polish

- [x] Voice cleanup and silence cutting, on device
- [x] On-device captions: Whisper (whisper.cpp, on the GPU on Macs) transcribes the take's microphone and any
      voice-overs on this computer, after a one-time 60 MB model download checked against a pinned hash. Captions
      follow cuts and speed changes, every word can be fixed in the editor, they come in two styles, are drawn into
      exports exactly as previewed, and save as SRT or WebVTT (Windows: written, awaiting testing on Windows)
- [ ] Learn tab guides filmed by Shotmaxxing itself
- [x] Content packs: JSON files of wallpapers and looks, added in Settings and checked strictly (only gradients both
      renderers draw the same), shown in the Background tool, kept with each project that uses them, and checked by
      `shotmaxxing pack check` ([format](content-packs.md))
- [ ] Packs with cursor sets, device frames and export targets; WASM effect plugins
- [x] Preview releases on GitHub for macOS (Apple silicon and Intel) and Windows, built by CI from a tag, with a
      daily check in the app that offers newer ones ([0.2.0](https://github.com/subh05sus/shotmaxxing/releases/tag/v0.2.0))
- [x] A Homebrew cask (this repository is its own tap) and winget manifests ready to submit
      ([packaging](../packaging/README.md))
- [ ] Signed and notarized releases, auto-updates, winget-pkgs and Microsoft Store listings

## Later

- Window recordings that keep a menu whole when it reaches past the window's edge (today it's cut at that edge)
- 3D device scenes
- Multiple screen sources in one recording
- Windows virtual display (IddCx)
- Linux
