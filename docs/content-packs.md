# Content packs

A content pack adds wallpapers and looks to the editor's Background tool. It is one JSON file: no code, no pictures and
nothing that reaches the network, so it is safe to pass around.

## Add one

In **Settings → Content packs**, choose **Add…** and pick the file. The app checks it first and says what's wrong if
anything is. Adding a pack with the same `id` as one you have replaces it, and **Remove** takes one out (with Undo).

Added packs live in your data folder:

- macOS: `~/Library/Application Support/dev.shotmaxxing.desktop/packs`
- Windows: `%APPDATA%\dev.shotmaxxing.desktop\packs`

A project keeps its own copy of the wallpaper it uses, so it looks the same on another computer, in an export, and after
the pack is removed.

## Make one

Start from the example, [northwind.json](content-packs/northwind.json):

```json
{
  "shotmaxxing_pack": 1,
  "id": "northwind",
  "name": "Northwind",
  "author": "Northwind Labs",
  "wallpapers": [
    {
      "id": "sunrise",
      "name": "Sunrise",
      "css": "radial-gradient(90% 70% at 20% 10%, oklch(0.9 0.1 85) 0%, transparent 60%), linear-gradient(135deg, oklch(0.78 0.14 60), oklch(0.5 0.17 30))"
    }
  ],
  "looks": [
    { "id": "launch", "name": "Launch", "wallpaper": "sunrise", "padding": 0.1, "corner_radius": 16, "shadow": 0.7 }
  ]
}
```

| Field              | What it is                                                                                          |
| ------------------ | --------------------------------------------------------------------------------------------------- |
| `shotmaxxing_pack` | Always `1`, the format's version. A pack with a newer number asks you to update the app.            |
| `id`               | 1 to 40 lowercase letters, digits and dashes, not starting with a dash.                             |
| `name`             | Shown in Settings and the Background tool. Up to 60 characters.                                     |
| `author`           | Optional. Up to 60 characters.                                                                      |
| `wallpapers`       | Up to 48.                                                                                           |
| `looks`            | Up to 24. A pack needs at least one wallpaper or look.                                              |

Fields the format doesn't have are refused, so a typo shows up instead of being ignored. The whole file can be up to
256 KB.

### Wallpapers

Each wallpaper has an `id` (the same rule as the pack's), a `name` of up to 40 characters, and its `css`. Projects and
agents call it `<pack id>.<wallpaper id>`, such as `northwind.sunrise`.

Two renderers draw the CSS: the editor's preview and the exporter. So it may use only what both draw the same way:

- **Layers**, separated by commas, the first on top, 8 at most:
  - `radial-gradient(W% H% at X% Y%, stops…)`: an ellipse with radii W and H around the point X, Y, all as percentages
    of the picture.
  - `linear-gradient(Adeg, stops…)`: 0deg points up and 90deg to the right. Without an angle it runs top to bottom
    (180deg).
- **Stops**, at least two per gradient: `oklch(L C H)`, `oklch(L C H / A)` or `transparent`, each optionally followed
  by a position such as `60%`. L and A are numbers from 0 to 1 or percentages; H is in degrees, as a plain number.
- **Opaque**: the finished picture can't be see-through anywhere, so end with a layer of solid colours, usually a
  `linear-gradient`.
- **Nothing else**: no `url()`, hex or named colours, semicolons or other CSS. The text may hold only lowercase letters,
  digits, spaces and `. , % ( ) / -`, up to 4,000 characters.

### Looks

A look applies several Background settings with one click. Each has an `id`, a `name` of up to 40 characters, and at
least one of:

| Field           | What it sets                                                                                  |
| --------------- | --------------------------------------------------------------------------------------------- |
| `wallpaper`     | One of this pack's wallpapers by its id, or a built-in one (below).                           |
| `padding`       | The space around the recording, from 0 to 0.2 of the picture.                                 |
| `corner_radius` | From 0 to 40 pixels.                                                                          |
| `shadow`        | From 0 to 1.                                                                                  |
| `aspect`        | `16:9`, `4:3`, `1:1`, `9:16`, or `original` (Fit, which only screenshots use).                |

The built-in wallpapers are `aurora-lime`, `graphite`, `dusk`, `ocean`, `peach`, `mint`, `violet-haze`, `paper` and
`midnight`.

## Check a pack

The `shotmaxxing` command checks a file the same way the app does, and lists what's added:

```sh
shotmaxxing pack check my-pack.json
shotmaxxing pack list
```

Homebrew puts the command on your PATH. It's also inside the app, at
`/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing`. Agents connected over MCP can pick pack wallpapers too:
the `style_project` tool lists them by their `<pack id>.<wallpaper id>`.
