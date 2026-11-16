# Record your screen

Press <kbd>⌥</kbd><kbd>⇧</kbd><kbd>5</kbd>, pick a screen, a window or an area, and show your thing. When you stop,
the recording opens in the editor with its zooms already added.

## Start a recording

1. Press <kbd>⌥</kbd><kbd>⇧</kbd><kbd>5</kbd> (<kbd>Alt</kbd><kbd>Shift</kbd><kbd>5</kbd> on Windows), or choose
   Record from the Shotmaxxing icon in the menu bar. The record dialog opens.
2. Keep **Video** chosen (the other choice is **Screenshot**, see [Screenshots](screenshots.md)).
3. Choose what to film:
   - **Screen:** a whole display.
   - **Window:** one window. Click it in the list.
   - **Area:** a part of the screen you drag out.
4. Turn on what else to record: **Microphone**, **Camera** and **System audio** (the computer's sound). Each has a
   list to pick the device from.
5. Press **Record**, or <kbd>Return</kbd>. With the countdown on, recording starts after three seconds; otherwise it
   starts right away. <kbd>Esc</kbd> closes the dialog without recording.

## While you record

A small pill shows while you record. Stop with its **Stop and edit** button, with <kbd>⌥</kbd><kbd>⇧</kbd><kbd>5</kbd>
again, or from the menu bar. The pill's **More** menu can **Restart recording** or **Discard recording**.

- Shotmaxxing's own windows never appear in a recording, and clicks on them aren't recorded either.
- Use the app as you normally would. Shotmaxxing records where the cursor is and draws a smooth one later, so you
  don't have to move carefully.
- Only what you record counts for auto zoom: a click somewhere else, or the click that stops the recording, makes no
  zoom.

## Windows, full-screen apps and Stage Manager

- **Window recordings keep their menus.** A dropdown or a sheet that opens over the window is recorded with it, and
  the recording follows the window if you move it.
- **Full-screen apps** are recorded with their toolbars. Pick one from the Desktop, and the Mac goes to its Space.
- **Stage Manager:** a window it parked comes back to the stage first, and records at its own size.

## Recording settings

In **Settings → Recording**:

| Setting | What it does |
| --- | --- |
| Count down before recording | Three seconds to get ready after you press Record |
| Show the recording pill | The pill with Stop, while you record |
| Hide desktop icons while recording | Leaves the desktop icons out of screen recordings |
| Frame rate | The most frames per second. The real rate follows what changes on screen |
| Save recordings to | Where projects go: Movies/Shotmaxxing in your home folder (Videos/Shotmaxxing on Windows) unless you choose another folder |

Each recording is a project, a `.smx` folder with the video, the sound and everything you change in the editor.

> [!TIP]
> Shotmaxxing stays cool while it records: the video is encoded by the computer's media engine, and **Settings →
> Performance → Low power** turns the extras down on battery or when the computer runs warm. The recording itself is
> never slowed down.

## From the command line

`shotmaxxing record --app Safari` records Safari's largest window until you press <kbd>Ctrl</kbd><kbd>C</kbd>. See
[the command line](command-line.md) for every option.
