# Contributing to Shotmaxxing

Thanks for helping. This page covers how to set up the project, how the code is organised, and what a pull request
needs before it can be merged.

## Set up

You need:

- Rust 1.90 or later, from [rustup.rs](https://rustup.rs)
- Node.js 22 or later and pnpm (`corepack enable` gives you the right pnpm version)
- [CMake](https://cmake.org), which builds whisper.cpp for the captions
- macOS 14 or later with the Xcode command line tools (`xcode-select --install`), or Windows 10 (version 2004) or later
- On Windows:
  - Visual Studio's C++ build tools;
  - [LLVM](https://llvm.org), whose libclang generates whisper.cpp's Rust bindings;
  - the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/), already installed on Windows 11.

```sh
pnpm install
pnpm tauri dev        # the desktop app, with hot reload for the UI
pnpm dev              # the UI alone in a browser, with sample data (http://localhost:1420)
cargo test --workspace
```

In a browser, add `?os=windows` or `?theme=light` to the address to preview the other platform or theme.

## How the code is organised

| Path | What lives there |
| --- | --- |
| `crates/core` | The project format, take scripts, auto zoom, camera and cursor maths, power policy. No platform code. |
| `crates/capture` | Recording: the screen, windows, sound, camera, the pointer and input, and screenshots. |
| `crates/render` | Export: the compositor, the sound mix and voice tools, MP4, GIF and PNG. |
| `crates/captions` | On-device captions with whisper.cpp. |
| `crates/control` | The control API between the `shotmaxxing` command and the running app. |
| `crates/agent` | The take performer: accessibility refs, the human-paced pointer and typing, safety stops and hand-off. |
| `crates/cli` | The `shotmaxxing` command and its MCP server. |
| `apps/desktop/src-tauri` | The desktop app: windows, menu bar, permissions, and the bridge to the crates. |
| `apps/desktop/src` | The Svelte UI for every window. |
| `packages/ui` | The design system: tokens, motion and shared components. |
| `skills/shotmaxxing` | The agent skill that teaches agents the workflow. |
| `scripts/` | The design guard, the icon and CLI sidecar builds, and the docs' screenshots. |
| `docs/` | Architecture, agents, roadmap and design principles. |

[docs/architecture.md](docs/architecture.md) explains how the pieces fit together.

## Before you open a pull request

```sh
pnpm lint             # design guard and formatting
pnpm check            # Svelte and TypeScript
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks on macOS and Windows. On Windows it also runs the capture and agent tests that need a real
desktop (`cargo test -p shotmaxxing-capture -- --ignored`, and the same for `shotmaxxing-agent`), and reports how they
went without failing the build, since hosted runners may lack a screen to capture.

### House rules

- **Idle means idle.** Nothing may run while the app is idle: no timers that never stop, no animation loops, no
  polling without a reason. Recording quality is never traded for UI polish.
- **Pixels stay native.** Video frames never pass through the web view. The UI sends intent; the core does the work.
- **Motion animates transform and opacity only,** stays at or under 300 ms, and falls back to fades with Reduce Motion
  or Low power. `scripts/design-guard.mjs` checks this and a few other rules. Fix the design rather than silencing
  the guard.
- **Colours come from tokens.** See `packages/ui/src/styles/tokens.css`.
- **Plain words in the UI.** Sentence case, verbs on buttons, and say what happens next.
- **Clean room.** Shotmaxxing learns from other recorders' ideas, never their code or assets. Don't paste code from
  projects whose licence isn't compatible with the AGPL.

The full design principles are in [docs/design/principles.md](docs/design/principles.md).

### Commits

Write commit messages in the imperative mood with a short first line ("Add cursor smoothing to the export path"),
then explain the why in the body if it isn't obvious. Keep each commit focused on one change.

## Licence

Shotmaxxing is licensed under the GNU Affero General Public License 3.0. By contributing, you agree that your
contributions are licensed under the same terms.
