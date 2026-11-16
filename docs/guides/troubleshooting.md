# Troubleshooting

The problems people run into most, and what fixes them. If yours isn't here,
[report a bug](https://github.com/subh05sus/shotmaxxing/issues/new/choose) with your Shotmaxxing version and your
system's version.

## Installing

### "Shotmaxxing.app Not Opened" on a Mac

That's macOS checking a file downloaded in a browser. Choose **Done**, then **System Settings → Privacy & Security →
Open Anyway**. Or install with the one-line installer or Homebrew, which don't get the warning. See
[Install](install.md#download-the-installer-yourself).

### "Windows protected your PC"

Choose **More info → Run anyway**. The PowerShell install line doesn't get the warning.

### `shotmaxxing: command not found`

The install line links the command into `/usr/local/bin` or `/opt/homebrew/bin` when one of them is on your PATH and
yours to change. If neither is, use the full path, `/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing`, or
install with Homebrew, which puts the command on your PATH.

## Recording

### Recordings come out blank, or setup keeps asking for Screen Recording

Shotmaxxing needs the **Screen Recording** permission, and macOS lets it use it only after it reopens. Choose
**Reopen Shotmaxxing** in setup.

If the switch is on in System Settings but setup still asks, macOS is matching it to an earlier copy of the app.
Remove Shotmaxxing from the list with the **−** button, then choose **Allow** again. See
[Permissions](permissions.md#its-on-in-system-settings-but-setup-still-asks).

### "Screen Recording is off for this version" at launch

macOS saw the update as a new app. Choose **Fix**, and allow it once. From 0.2.1 on, updates keep the permission.

### There are no zooms on my clicks

Auto zoom notices clicks and typing through the **Accessibility** permission. Turn it on in **System Settings →
Privacy & Security → Accessibility**, then use **Redo auto zoom** in the Zoom tool. Clicks outside the recorded area,
and the click that stops the recording, never make a zoom. See [Auto zoom](auto-zoom.md).

### The computer gets warm

**Settings → Performance → Low power** turns the extras down: previews run at 30 fps, animations become fades, and
AI jobs wait for power. Left on **Automatic**, it turns on by itself with battery saver, when the computer gets warm,
or with Reduce Motion. The recording itself is never slowed down.

## Editing

### Generate captions isn't there

Captions are made from your voice, and the take has none. Record a voice-over in the Audio tool, then make captions.
See [Captions](captions.md).

## Agents

### My agent doesn't see Shotmaxxing

1. Open **Agents** in Shotmaxxing and check the agent shows as connected. If not, choose **Connect**.
2. Start a **new** chat in the agent. A chat that was already open doesn't pick up new tools.
3. If the agent's settings file has comments in it, Shotmaxxing doesn't rewrite it, and offers to copy the lines so
   you can add them yourself.

To add it by hand, give the agent a server that runs `shotmaxxing mcp`. See [Agents](../agents.md#connect-an-agent).

### A take stopped by itself

Moving the mouse during an agent's take stops it at once, so you're always in control. So does pressing
<kbd>Esc</kbd> three times. Ask the agent to run the take again, and keep your hands off the mouse until it's done.
