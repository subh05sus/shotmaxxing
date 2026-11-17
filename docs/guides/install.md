# Install Shotmaxxing

One line in Terminal or PowerShell installs the current release, checks it and opens it. Homebrew works too, and so
does downloading the installer yourself.

| Computer | What you need |
| --- | --- |
| Mac | macOS 14 Sonoma or later, Apple silicon or Intel |
| Windows | Windows 10 version 2004 (build 19041) or later, 64-bit |

> [!NOTE]
> Recording, export and agent takes on Windows are written but haven't been tested on a Windows computer yet, so
> expect rough edges there. Linux isn't supported yet; it's on the [roadmap](../roadmap.md).

## Install in one line

Paste the line for your computer into Terminal (macOS) or PowerShell (Windows):

```sh title="macOS"
curl -fsSL https://shotmaxxing.app/install | bash
```

```powershell title="Windows"
irm https://shotmaxxing.app/install.ps1 | iex
```

Each line:

1. Downloads the current release from GitHub.
2. Checks the download against the SHA-256 it pins, and stops if they differ.
3. Installs it: in Applications on a Mac, for your user on Windows (no administrator rights needed).
4. Opens it, with no "Not Opened" dialog on the Mac and no SmartScreen warning on Windows.

On a Mac, the `shotmaxxing` command is also linked into a folder on your PATH when one is yours to change. Both
scripts are short, so you can read [install.sh](../../install.sh) or [install.ps1](../../install.ps1) before you run
them.

## With Homebrew

On a Mac with [Homebrew](https://brew.sh), this repository works as its own tap. The app opens on the first click
this way too, and the `shotmaxxing` command goes on your PATH:

```sh
brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing
brew install --cask subh05sus/shotmaxxing/shotmaxxing
```

## Download the installer yourself

Every version's files are on the [Releases](https://github.com/subh05sus/shotmaxxing/releases) page:

| Computer | File |
| --- | --- |
| Mac with Apple silicon | `Shotmaxxing_<version>_aarch64.dmg` |
| Mac with Intel | `Shotmaxxing_<version>_x64.dmg` |
| Windows, setup program | `Shotmaxxing_<version>_x64-setup.exe` |
| Windows, MSI | `Shotmaxxing_<version>_x64_en-US.msi` |

The builds aren't notarized by Apple or signed for Windows yet, so a file downloaded in a browser gets one warning
the first time it opens.

**On a Mac:**

1. Open the disk image, agree to the licence, and drag Shotmaxxing to Applications.
2. Open Shotmaxxing. When "Shotmaxxing.app Not Opened" appears, choose **Done**, not Move to Trash.
3. Open **System Settings → Privacy & Security**, scroll down and choose **Open Anyway**.

Or clear the download mark in Terminal instead:

```sh
xattr -dr com.apple.quarantine /Applications/Shotmaxxing.app
```

**On Windows:** when SmartScreen says "Windows protected your PC", choose **More info → Run anyway**.

> [!TIP]
> The install line and Homebrew skip all of this: they don't put the "downloaded from the internet" mark on what they
> fetch, and they check the download's SHA-256 instead.

## Update

Shotmaxxing updates itself. It looks for a newer version when it opens and every six hours, and downloads it in the
background. **Update to …** then appears at the bottom of the sidebar: it shows what changed in every version since
yours, and **Restart to update** installs it and opens the new version. It waits while you're recording or exporting,
**Skip this version** stops it offering that one, and **Settings → About → Check for updates** looks right away. The
first time the new version opens, it shows what's new. You can turn the automatic checks off in **Settings →
Privacy**.

Running the install line again, or `brew upgrade --cask shotmaxxing` with Homebrew, updates it too. Versions before
0.3.1 can't update themselves, so update those once that way.

From 0.2.1 on, the Mac app in every release is signed with the same identity, so macOS keeps Screen Recording and
Accessibility when Shotmaxxing updates.

## Uninstall

- **macOS:** quit Shotmaxxing and drag it from Applications to the Trash. If the install line linked the
  `shotmaxxing` command, remove the link it named, such as `rm /usr/local/bin/shotmaxxing`. With Homebrew:
  `brew uninstall --cask shotmaxxing`.
- **Windows:** **Settings → Apps → Installed apps → Shotmaxxing → Uninstall**.

Your recordings and projects stay where they are.

## Next

[Set up Shotmaxxing and record your first take](setup.md).
