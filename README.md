# Shotmaxxing

An open-source screen recorder and studio editor for demos, walkthroughs and product videos.
You can drive it by hand, or let an AI agent such as Claude or Codex film the demo for you.

## Goals

- **Record anything on screen:** a display, a window, an area or several sources, with system audio,
  microphone and camera.
- **Studio editor:** intelligent auto zoom, a large smooth cursor with click effects, styled backgrounds,
  a timeline, captions and annotations, with export to MP4 and GIF.
- **Agent-native:** a first-class control API and MCP server, so an agent can stage an app, perform a
  scripted take, edit the result and export it.
- **Cross-platform from day one:** macOS and Windows, built so more platforms can be added later.
- **Local first:** recordings and projects stay on your machine.

## Status

Early development, macOS first. No release is available yet; you can build it from source. Working today on macOS:

- recording a display or a window with the computer's sound, a microphone and a camera;
- the editor, with automatic zooms, a rendered cursor, cuts, speed, text, arrows, blur and a camera bubble;
- voice cleanup and silence cutting, on device;
- export to MP4 and GIF;
- an MCP server and one-click connect for AI agents.

Windows recording and export are written and waiting for testing on a Windows computer; sound and the camera on
Windows come next. See the [roadmap](docs/roadmap.md).

## Build from source

You need Rust (stable), Node 22 or later, pnpm and the Xcode command line tools.

```sh
pnpm install
pnpm tauri dev                                       # the app
cargo run -p shotmaxxing-cli -- record --app Safari  # a take from the terminal; Ctrl-C stops it
cargo run -p shotmaxxing-cli -- export ~/Movies/Shotmaxxing/Recording.smx
```

## Use it from an AI agent

Connect Claude Code, Codex, Cursor or another agent from the app's Agents page, or add `shotmaxxing mcp` to the
agent's MCP settings. Then ask, for example: "Film a 30-second demo of the export flow in Safari".
See [Agents](docs/agents.md) and the [agent skill](skills/shotmaxxing/SKILL.md).

## Documentation

- [Agents](docs/agents.md): connecting agents, the MCP tools and a typical take
- [Architecture](docs/architecture.md): how the recorder, editor and agent layer fit together
- [Design principles](docs/design/principles.md): motion, interaction laws and craft rules
- [Roadmap](docs/roadmap.md)

## Platforms

| Platform | Target |
| --- | --- |
| macOS | 14 Sonoma and later |
| Windows | Windows 10 20H1 (build 19041) and later |

## License

Shotmaxxing is licensed under the [GNU Affero General Public License v3.0](LICENSE).
