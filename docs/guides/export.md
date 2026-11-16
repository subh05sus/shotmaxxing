# Export

Press <kbd>⌘</kbd><kbd>E</kbd> in the editor, pick where the video is going, and export. Exports wait in a queue and
render one at a time, so you can keep editing.

## Presets

| Preset | Shape and size | Frame rate | Format |
| --- | --- | --- | --- |
| YouTube and the web | The project's shape, 1080p | 60 fps | MP4, H.264 |
| X and LinkedIn | 1:1, 1080p | 30 fps | MP4, H.264 |
| Shorts, Reels and TikTok | 9:16, 1080p | 30 fps | MP4, H.264 |
| GIF | 800 px on the long side | 15 fps | GIF |
| 4K | The project's shape, 2160p | 60 fps | MP4, HEVC |
| This frame as an image | 1440p | | PNG |

When a preset's shape differs from the project's, the sheet offers to use the preset's shape.

## Quality, frame rate and folder

- **Quality:** **Smaller**, **Balanced** or **Higher**. Smaller files load faster; higher keeps fine detail such as
  small text.
- **Frame rate:** **30 fps** or **60 fps**.
- **Save to:** the folder the file goes in. **Use default** goes back to the usual one.

Press **Export**, or <kbd>Return</kbd>.

> [!TIP]
> Video is encoded by the computer's media engine, so exporting doesn't heat the computer up or slow other apps down
> much.

## From the command line

```sh
shotmaxxing export "Launch demo.smx" -o launch.mp4
shotmaxxing export "Launch demo.smx" --format gif --size 480 --fps 15
shotmaxxing export "Launch demo.smx" --format png --at 12.5
```

See [the command line](command-line.md) for every option.
