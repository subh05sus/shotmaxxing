# The command line

The `shotmaxxing` command records, exports and inspects projects from Terminal or PowerShell, and serves AI agents
over MCP. The install line and Homebrew put it on your PATH; inside the app it is
`/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing`.

Every command takes `--json` for machine-readable output, and `--help` for its options.

## Commands

| Command | What it does |
| --- | --- |
| `shotmaxxing sources` | Lists the screens and windows you can record, with their ids |
| `shotmaxxing devices` | Lists the microphones and cameras you can record |
| `shotmaxxing record` | Records a screen or a window into a new project |
| `shotmaxxing export <project>` | Exports a project, with every edit applied |
| `shotmaxxing info <project>` | Shows what's in a project |
| `shotmaxxing zoom <project>` | Plans auto zoom again from the project's recorded clicks and typing |
| `shotmaxxing open <project>` | Opens a project in the editor, opening the app first if it isn't running |
| `shotmaxxing take …` | Checks, prepares and films take scripts |
| `shotmaxxing pack …` | Checks and lists content packs |
| `shotmaxxing mcp` | Serves AI agents over MCP. Agents start this themselves once connected |

## Record

```sh
shotmaxxing record --app Safari -d 30s --name "Sign-up flow"
```

| Option | What it does |
| --- | --- |
| `--display <id>` | A display from `shotmaxxing sources`. The main display if nothing is chosen |
| `--window <id>` | A window from `shotmaxxing sources` |
| `--app <name>` | The largest window of this app, such as `Safari` |
| `-d`, `--duration` | Stop after this long: `30s`, `1:30` or `2m`. Without it, press <kbd>Ctrl</kbd><kbd>C</kbd> to stop |
| `--name` | The project's name. `Recording` if not given |
| `--out <folder>` | The folder to put the project in |
| `--fps` | The most frames per second, 60 if not given. The real rate follows what changes on screen |
| `--hide-desktop-icons` | Leaves the desktop icons out of a screen recording |
| `--system-audio` | Records what the computer plays |
| `--mic [id]` | Records a microphone: an id from `shotmaxxing devices`, or the system's choice |
| `--camera [id]` | Films you with a camera: an id from `shotmaxxing devices`, or the system's choice |

## Export

```sh
shotmaxxing export "Sign-up flow.smx" -o signup.mp4 --size 1080 --fps 60
```

| Option | What it does |
| --- | --- |
| `-o`, `--out <file>` | The file to write. Without it, the video goes next to the project, named after it |
| `--format` | `mp4` plays everywhere, `hevc` is smaller, `gif` loops in chats and docs, `png` is one styled frame. MP4 if not given, or PNG for a screenshot |
| `--at <seconds>` | For a PNG: the moment to show, in seconds into the recording |
| `--size <pixels>` | The shorter side, such as 720, 1080, 1440 or 2160. The other side follows the shape. 1080 if not given, or a screenshot's own pixels |
| `--fps` | Frames per second, 60 if not given |
| `--mbps` | The bitrate in Mbit/s. The default suits the size |

## Zoom

```sh
shotmaxxing zoom "Sign-up flow.smx" --depth bold
```

`--depth` is `subtle`, `standard` (the default) or `bold`.

## Take scripts

A take script is a YAML or JSON file of steps that Shotmaxxing performs while it records. Agents write them; you can
too. See [Agents](../agents.md) for the format.

| Command | What it does |
| --- | --- |
| `shotmaxxing take validate <file>` | Checks a take script and lists any problems |
| `shotmaxxing take schema` | Prints the JSON Schema for take scripts |
| `shotmaxxing take prepare --app <name>` | Opens an app and lists the controls a take script can name by ref |
| `shotmaxxing take check <file>` | Checks a take script against the app on screen now: what would stop the take, with something to try instead |
| `shotmaxxing take run <file>` | Films it. `--dry-run` shows the timeline without moving anything. Press <kbd>Esc</kbd> three times to stop |

`take run` also takes `--out <folder>`, `--mic [id]` for narration and `--system-audio`. `take prepare` and
`take check` take `--window <title>` when the app has several windows.

## Content packs

| Command | What it does |
| --- | --- |
| `shotmaxxing pack check <file>` | Checks a content pack the way the app does before adding it, and says what's in it |
| `shotmaxxing pack list` | Lists the content packs added in the app, with the ids of their wallpapers |

See [Content packs](../content-packs.md).
