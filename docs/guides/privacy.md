# Privacy

Everything happens on your computer. Recordings, projects and captions are never uploaded, and there is no account.

## What goes online

Shotmaxxing goes online for two things, and nothing else:

| What | When | Turn it off |
| --- | --- | --- |
| A check for a newer release, asked of GitHub | Once a day | **Settings → Privacy → Check for updates** |
| The speech model for captions, about 60 MB from Hugging Face | Once, the first time you make captions. It's checked against a pinned hash | Don't make captions |

## What stays on your computer

- **Recordings and projects**, in Movies/Shotmaxxing in your home folder (Videos/Shotmaxxing on Windows) unless you
  chose another folder.
- **Captions, voice cleanup and silence cutting** all run on this computer.
- **Agents** reach the app through a local connection only your user can open.

## What a recording leaves out

- Shotmaxxing's own windows never appear in a recording, and clicks on them aren't recorded.
- **Show keystrokes** never shows typing in password fields, and an agent's take never types into one.
- Blur hides emails, tokens and names before you share. See [Annotate](annotate.md).

## Agents

The take card asks before an agent sees or records your screen, and again before every take, with its plan. A thin
red edge goes round the screen while an agent moves the pointer and types, and <kbd>Esc</kbd> three times or moving the
mouse stops it at once. See [Agents](../agents.md#permissions).

Shotmaxxing is open source under the AGPL 3.0, so all of this can be checked in
[the code](https://github.com/subh05sus/shotmaxxing).
