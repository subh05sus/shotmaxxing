# The Shotmaxxing cask. This repository is its own tap:
#
#   brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing
#   brew install --cask subh05sus/shotmaxxing/shotmaxxing
#
# Each release changes `version` and both hashes (GitHub shows each asset's SHA-256 on the release page).
cask "shotmaxxing" do
  arch arm: "aarch64", intel: "x64"

  version "0.3.3"
  sha256 arm:   "4d18d67db7467e7fadcae95a0a9bdc4f3ced98225ef00429da7d38179a33b8fb",
         intel: "67c3a0f95ff01eeb23f790461d70687ae19e6f8b7dae7e89512b402d66b88e52"

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
