# Packaging

The one-line installers and the manifests for package managers, filled in for each release from the hashes on its
GitHub release page.

- **One-line installers:** [`install.sh`](../install.sh) (macOS, `curl -fsSL …/install.sh | bash`) and
  [`install.ps1`](../install.ps1) (Windows, `irm …/install.ps1 | iex`), at the root so their addresses stay short.
  They download with curl and PowerShell, which don't put the "downloaded from the internet" mark on what they fetch,
  so the app opens on the first click although the builds aren't notarized (macOS) or signed (Windows) yet. Each
  checks the download against the SHA-256 it pins.
- **Homebrew:** [`Casks/shotmaxxing.rb`](../Casks/shotmaxxing.rb), at the root so this repository works as its own
  tap: `brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing`, then
  `brew install --cask subh05sus/shotmaxxing/shotmaxxing`. Homebrew marks every cask download as quarantined, so the
  cask takes the mark off the installed app after Homebrew has checked the download's SHA-256.
- **winget:** [`winget/manifests`](winget/manifests), laid out as in
  [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). They're ready to submit there, which the
  maintainers do by hand; until then, `winget install --manifest packaging/winget/manifests/s/subh05sus/Shotmaxxing/0.3.0`
  installs from them on a computer with local manifests allowed.

For a new release, once it's published:

1. In `install.sh`, change `VERSION` and the SHA-256 of both `.app.tar.gz` files.
2. In `install.ps1`, change `$Version` and the SHA-256 of the `-setup.exe`.
3. In the cask, change `version` and the SHA-256 of both `.dmg` files.
4. Copy the winget manifests to a folder for the new version, and change the version, the address and the SHA-256.
5. In the README, change the download links under **Or download the installer yourself**.

GitHub shows each file's SHA-256 on the release page. CI checks that all five name the same release, and the
[Install workflow](../.github/workflows/install.yml) checks every hash against the release and installs from the
installers and the cask on clean Macs and Windows computers.

## Installer pictures

The disk image window and the Windows installers have their own pictures, drawn in
[`art/installer-art.html`](art/installer-art.html) in the app's colours and kept as files in
[`apps/desktop/src-tauri/installer`](../apps/desktop/src-tauri/installer):

| File | Where it shows | Size |
| --- | --- | --- |
| `dmg-background.tiff` | The disk image window: a viewfinder with a drag from the app to Applications, and how to open the app the first time | 660 × 440 points, 1x and 2x |
| `nsis-sidebar.bmp` | The setup `.exe`'s welcome and finish pages | 164 × 314 |
| `nsis-header.bmp` | The left of the setup `.exe`'s page header | 150 × 57 |
| `wix-dialog.bmp` | The `.msi`'s welcome and finish pages; the installer writes on the white right side | 493 × 312 |
| `wix-banner.bmp` | The top of the `.msi`'s other pages; the installer writes on the white left side | 493 × 58 |

After changing the HTML, redraw them with `python3 scripts/installer-art.py` (macOS, with Google Chrome and
Pillow). Open the HTML with `#dmg-preview` at the end of its address to see the disk image window as Finder draws
it, with the names in white (Dark Mode) and black (Light Mode).

The disk image window is set to 660 × 428 in `tauri.conf.json` because Finder counts its title bar in that size,
which leaves 660 × 400 for the picture; the picture is 440 tall so a taller title bar never shows a gap. The plates
under the names are the one mid tone that gives both white and black text 4.58 to 1.
