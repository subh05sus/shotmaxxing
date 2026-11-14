# Contributing to Shotmaxxing

Thanks for helping. This page covers how to set up the project, how the code is organised, and what a pull request
needs before it can be merged.

## Set up

You need:

- Rust (stable, 1.88 or later), from [rustup.rs](https://rustup.rs)
- Node.js 22 or later and pnpm (`corepack enable` gives you the right pnpm version)
- macOS 14 or later, or Windows 10 (version 2004) or later
- On Windows, the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (already installed on
  Windows 11)

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
| `apps/desktop/src-tauri` | The desktop app: windows, menu bar, permissions, and the bridge to the core. |
| `apps/desktop/src` | The Svelte UI for every window. |
| `packages/ui` | The design system: tokens, motion and shared components. |
| `docs/` | Architecture, roadmap and design principles. |

[docs/architecture.md](docs/architecture.md) explains how the pieces fit together.

## Before you open a pull request

```sh
pnpm lint             # design guard and formatting
pnpm check            # Svelte and TypeScript
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks on macOS and Windows.

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
