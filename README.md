<p align="center">
  <img src="assets/brand/app-icon.svg" width="88" height="88" alt="">
</p>

<h1 align="center">Shotmaxxing</h1>

<p align="center">
  Record demos that edit themselves. Record your screen by hand, or let an AI agent such as Claude or Codex film it.
  Shotmaxxing adds the zooms, smooths the cursor and keeps your computer cool while it works.
</p>

<p align="center">
  <a href="https://github.com/subh05sus/shotmaxxing/releases">Download</a> ·
  <a href="docs/agents.md">Agents</a> ·
  <a href="docs/architecture.md">Architecture</a> ·
  <a href="docs/roadmap.md">Roadmap</a>
</p>

<picture>
  <source media="(prefers-color-scheme: light)" srcset="docs/images/editor-light.png">
  <img src="docs/images/editor.png" alt="The Shotmaxxing editor with the sample project: the Background tool on the left, the recording in the middle on a green wallpaper, and the timeline with its zooms and effects below.">
</picture>

Open source (AGPL 3.0) and local first: recordings and projects stay on your computer. For macOS, with Windows on
the way.

## What it does

### Record

- **A display, a window or an area**, with the computer's sound, a microphone and a camera.
- **Window recordings keep their menus.** A dropdown or a sheet that opens over the window is recorded with it, and
  the recording follows the window if you move it.
- **Full-screen apps and Stage Manager** are handled: a full-screen app is recorded with its toolbars, and a window
  Stage Manager parked comes back to the stage first, at its own size.
- **Screenshots** with ⌥⇧4 (Alt+Shift+4 on Windows): a full-resolution picture of a screen or a window, opened in the
  editor to frame and mark up, then copied or saved as a PNG.
- ⌥⇧5 (Alt+Shift+5) starts a recording. A small pill shows while you record: stop with it, with ⌥⇧5 again, or from
  the menu bar. Shotmaxxing's own windows never appear in a recording, and clicks on them aren't recorded either.

<p align="center">
  <img src="docs/images/record.png" width="640" alt="The record dialog: Video or Screenshot, Screen, Window or Area, the windows to choose from, and switches for the microphone, the camera and the computer's sound.">
</p>

### Edit

- **Auto zoom** follows what you click and type, and says why each zoom is there. Choose how far it zooms and whether
  it glides or cuts, or add, move and remove zooms by hand.
- **A rendered cursor:** larger, smoothed, with click effects, a spotlight, motion blur and click sounds.
- **The look:** wallpapers, padding, rounded corners, shadow, and 16:9, 4:3, 1:1 or 9:16.
- **A timeline** to trim, split and speed up, with text, arrows and blur on top.
- **Sound:** voice cleanup and silence cutting on device, a voice-over recorded in the editor, and music under the
  whole video.
- **Captions** transcribed on your computer with Whisper, editable word by word, drawn into the video or saved as SRT
  or WebVTT.
- **A camera bubble**, autosave and version history.
- **Export** to MP4 (H.264 or HEVC) or GIF, or any frame as a PNG. Exports wait in a queue and render one at a time.

<p align="center">
  <img src="docs/images/editor-zoom.png" alt="The Zoom tool: auto zoom on, how far and how smooth it zooms, and the four zooms in the video, each with the reason it is there.">
</p>

### Let an agent film it

Connect Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode or Gemini CLI from the **Agents**
page, then ask for a demo: "Film a 30-second demo of my app's sign-up flow and export it for X."

- Nothing happens until you allow it: the take card asks before an agent sees or records your screen, and before
  every take.
- The agent opens the app, finds each control and plans the take, then performs it in one smooth pass.
- The take card shows every step as it happens. Pause, take over or stop at any time: pressing Esc three times or
  moving the mouse stops a take straight away.
- Agents never type into password fields. Sign-ins come to you, and the agent waits until you hand back.
- The finished take opens in the editor with its zooms already added.

<p align="center">
  <img src="docs/images/agents.png" width="49%" alt="The Agents page: the agents you can connect, prompts to try, and how a take works.">
  <img src="docs/images/take-card.png" width="34%" alt="The take card while Claude Code films: the steps of the take, with Pause, Take over and Stop.">
</p>

Agents talk to Shotmaxxing through MCP (`shotmaxxing mcp`) and the same command line you can use yourself. See
[Agents](docs/agents.md) and the [agent skill](skills/shotmaxxing/SKILL.md).

### Stays cool

Shotmaxxing was built not to heat up your computer. Nothing animates or redraws while the app sits idle, the video
is encoded by the computer's media engine, and Low power mode turns the rest of the work down on battery or when the
computer runs warm. The recording itself is never slowed down.

## Install

Download the latest build from [Releases](https://github.com/subh05sus/shotmaxxing/releases), or on a Mac use
Homebrew, which also puts the `shotmaxxing` command on your PATH:

```sh
brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing
brew install --cask subh05sus/shotmaxxing/shotmaxxing
```

- **macOS 14 or later:** the `.dmg` for Apple silicon (`aarch64`) or Intel (`x64`). The builds aren't notarized yet, so
  macOS blocks the app the first time it opens: open **System Settings → Privacy & Security** and choose **Open
  Anyway**.
- **Windows 10 (version 2004) or later, 64-bit:** the `-setup.exe` installer, or the `.msi`. It isn't signed yet, so
  SmartScreen may warn about it: choose **More info → Run anyway**. Recording, export and agent takes on Windows are written but
  haven't been tested on a Windows computer yet, so expect rough edges.

On macOS, Shotmaxxing needs Screen Recording to record. The microphone and the camera are asked for when you turn
them on, and Accessibility lets auto zoom notice clicks and typing and lets agents press buttons. The onboarding
walks you through each one.

## Build from source

You need Rust 1.90 or later, Node 22 or later, pnpm and CMake, with the Xcode command line tools on macOS.
[CONTRIBUTING.md](CONTRIBUTING.md) lists what Windows needs and the checks a pull request runs.

```sh
pnpm install
pnpm tauri dev                                       # the app
pnpm dev                                             # the UI alone in a browser, with sample data
cargo run -p shotmaxxing-cli -- record --app Safari  # record Safari's window from the terminal; Ctrl-C stops it
cargo run -p shotmaxxing-cli -- export ~/Movies/Shotmaxxing/Recording.smx
pnpm bundle                                          # the installable app, with the shotmaxxing command inside
```

## Privacy

Everything happens on your computer. Recordings, projects and captions are never uploaded, and agents reach the app
through a local connection only this user can open. The app goes online for two things: once a day it asks GitHub
whether a newer release is out (you can turn that off in Settings), and the first time you make captions it
downloads the 60 MB Whisper model from Hugging Face, checked against a pinned hash.

## Documentation

- [Agents](docs/agents.md): connecting agents, the MCP tools, the command line and a typical take
- [Architecture](docs/architecture.md): how the recorder, the editor and the agent layer fit together
- [Design principles](docs/design/principles.md): motion, interaction and craft rules
- [Roadmap](docs/roadmap.md) and [changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md) and [security](SECURITY.md)

## Platforms

| Platform | Target | Status |
| --- | --- | --- |
| macOS | 14 Sonoma and later, Apple silicon and Intel | Preview |
| Windows | Windows 10 version 2004 (build 19041) and later, 64-bit | Written, waiting for testing |

## License

Shotmaxxing is licensed under the [GNU Affero General Public License v3.0](LICENSE).

Icons are from [Pikaicons](https://pikaicons.com/free-icons) and [Hugeicons](https://hugeicons.com), both under the
MIT license.
