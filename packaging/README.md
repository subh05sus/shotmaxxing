# Packaging

Manifests for package managers, filled in for each release from the hashes on its GitHub release page.

- **Homebrew:** [`Casks/shotmaxxing.rb`](../Casks/shotmaxxing.rb), at the root so this repository works as its own
  tap: `brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing`, then
  `brew install --cask subh05sus/shotmaxxing/shotmaxxing`.
- **winget:** [`winget/manifests`](winget/manifests), laid out as in
  [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs). They're ready to submit there, which the
  maintainers do by hand; until then, `winget install --manifest packaging/winget/manifests/s/subh05sus/Shotmaxxing/0.1.0`
  installs from them on a computer with local manifests allowed.

For a new release, change the version and the SHA-256 of each installer, which GitHub shows on the release page.
