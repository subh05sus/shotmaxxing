# Packaging

Manifests for package managers, filled in for each release from the hashes on its GitHub release page.

- **Homebrew:** [`Casks/shotmaxxing.rb`](../Casks/shotmaxxing.rb), at the root so this repository works as its own
  tap: `brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing`, then
  `brew install --cask subh05sus/shotmaxxing/shotmaxxing`.
- **winget:** [`winget/manifests`](winget/manifests), laid out as in
  [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). They're ready to submit there, which the
  maintainers do by hand; until then, `winget install --manifest packaging/winget/manifests/s/subh05sus/Shotmaxxing/0.2.0`
  installs from them on a computer with local manifests allowed.

For a new release, change the version and the SHA-256 of each installer, which GitHub shows on the release page.

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
