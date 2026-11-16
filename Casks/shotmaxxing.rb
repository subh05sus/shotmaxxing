# The Shotmaxxing cask. This repository is its own tap:
#
#   brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing
#   brew install --cask shotmaxxing
#
# Each release changes `version` and both hashes (GitHub shows each asset's SHA-256 on the release page).
cask "shotmaxxing" do
  arch arm: "aarch64", intel: "x64"

  version "0.2.0"
  sha256 arm:   "bf33dece0089b81c75dec8f4675ec922d48160cf20017825dcd52297874de6f9",
         intel: "299c565b4f8721f3673eca8ada3fdccaa74b02aeb34fd0184a3af88064490b42"

  url "https://github.com/subh05sus/shotmaxxing/releases/download/v#{version}/Shotmaxxing_#{version}_#{arch}.dmg"
  name "Shotmaxxing"
  desc "Screen recorder and studio editor for demos, drivable by AI agents"
  homepage "https://github.com/subh05sus/shotmaxxing"

  depends_on macos: ">= :sonoma"

  app "Shotmaxxing.app"
  # The command agents use, on the PATH too.
  binary "#{appdir}/Shotmaxxing.app/Contents/MacOS/shotmaxxing"

  zap trash: [
    "~/Library/Application Support/dev.shotmaxxing.desktop",
    "~/Library/Caches/dev.shotmaxxing.desktop",
    "~/Library/HTTPStorages/dev.shotmaxxing.desktop",
    "~/Library/WebKit/dev.shotmaxxing.desktop",
  ]

  caveats <<~EOS
    Shotmaxxing isn't notarized yet, so macOS blocks it the first time it opens. Open System Settings →
    Privacy & Security and choose Open Anyway.
  EOS
end
