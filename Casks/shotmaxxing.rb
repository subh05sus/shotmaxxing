# The Shotmaxxing cask. This repository is its own tap:
#
#   brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing
#   brew install --cask subh05sus/shotmaxxing/shotmaxxing
#
# Each release changes `version` and both hashes (GitHub shows each asset's SHA-256 on the release page).
cask "shotmaxxing" do
  arch arm: "aarch64", intel: "x64"

  version "0.2.1"
  sha256 arm:   "d1542a0c0bd2b6e0668eab03f734afa3eaceef51dc2d41ed470203f8c7e80a94",
         intel: "37210fdf0402aba972a07a3a28a3c7dce4c8b9bf705e78558e9b26fd55fed978"

  url "https://github.com/subh05sus/shotmaxxing/releases/download/v#{version}/Shotmaxxing_#{version}_#{arch}.dmg"
  name "Shotmaxxing"
  desc "Screen recorder and studio editor for demos, drivable by AI agents"
  homepage "https://github.com/subh05sus/shotmaxxing"

  # macOS 14 Sonoma or later.
  depends_on macos: :sonoma

  app "Shotmaxxing.app"
  # The command agents use, on the PATH too.
  binary "#{appdir}/Shotmaxxing.app/Contents/MacOS/shotmaxxing"

  # The builds aren't notarized yet, and macOS stops an app like that the first time it opens when it carries the
  # quarantine mark Homebrew puts on every cask download. The download was checked against the sha256 above, so the
  # mark comes off and the app opens on the first click.
  postflight_steps do
    run "/usr/bin/xattr",
        args:         ["-dr", "com.apple.quarantine", "{{appdir}}/Shotmaxxing.app"],
        must_succeed: false
  end

  zap trash: [
    "~/Library/Application Support/dev.shotmaxxing.desktop",
    "~/Library/Caches/dev.shotmaxxing.desktop",
    "~/Library/HTTPStorages/dev.shotmaxxing.desktop",
    "~/Library/WebKit/dev.shotmaxxing.desktop",
  ]
end
