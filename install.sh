#!/bin/bash
# Installs Shotmaxxing on a Mac. Paste this line into Terminal:
#
#   curl -fsSL https://raw.githubusercontent.com/subh05sus/shotmaxxing/main/install.sh | bash
#
# It downloads the app from the GitHub release named below, checks it against the SHA-256 written here, puts
# Shotmaxxing.app in /Applications (in ~/Applications when /Applications isn't yours to change), links the
# shotmaxxing command and opens the app. Run the same line again to update.
#
# Why the app opens on the first click: the builds aren't notarized yet, and macOS stops an app like that the first
# time it opens ("Shotmaxxing.app Not Opened") only when the app carries the quarantine mark that browsers and
# Homebrew casks put on what they download. curl doesn't add that mark. Nothing on the Mac is switched off: the
# download is checked against the hash below instead.
#
# Optional settings:
#   SHOTMAXXING_APP_DIR=~/Applications   where the app goes
#   SHOTMAXXING_NO_OPEN=1                don't open the app at the end
#
# Each release changes VERSION and both hashes. GitHub shows each SHA-256 on the release page, next to the
# .app.tar.gz files; the CI checks that this script, install.ps1, the cask and the README name the same release.
set -euo pipefail

VERSION="0.3.0"
SHA256_AARCH64="18cc78bea486003279ba9a1b8630e4458bd07e176e3fe48188100aac4b792601"
SHA256_X64="035db35b071c0f881889f33e10875c2c329ad41113b5a2773e72cac4d270f444"
RELEASES="https://github.com/subh05sus/shotmaxxing/releases/download"

tmp=""
cleanup() { if [ -n "$tmp" ]; then rm -rf "$tmp"; fi; }
trap cleanup EXIT

say() { printf '%s\n' "$*"; }
fail() {
  printf '\nShotmaxxing was not installed: %s\n' "$*" >&2
  exit 1
}

# Everything runs from here, so a download of this script cut short never runs half of it.
main() {
  [ "$(uname -s)" = Darwin ] || fail "this line is for macOS. For Windows, see https://github.com/subh05sus/shotmaxxing#install"
  local os major
  os="$(sw_vers -productVersion)"
  major="${os%%.*}"
  [ "$major" -ge 14 ] 2>/dev/null || fail "it needs macOS 14 Sonoma or later, and this Mac has macOS $os."

  # The Apple silicon build even when this Terminal runs under Rosetta, where uname -m says x86_64.
  local arch sha kind
  if [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
    arch=aarch64 sha="$SHA256_AARCH64" kind="Apple silicon"
  else
    arch=x64 sha="$SHA256_X64" kind="Intel"
  fi

  local dir="${SHOTMAXXING_APP_DIR:-}"
  if [ -z "$dir" ]; then
    if [ -w /Applications ]; then dir=/Applications; else dir="$HOME/Applications"; fi
  fi
  mkdir -p "$dir" || fail "couldn't make the folder $dir."
  local app="$dir/Shotmaxxing.app"
  if [ -e "$app" ] && [ ! -w "$app" ]; then
    fail "$app belongs to another user. Remove it, or set SHOTMAXXING_APP_DIR to another folder, then run the line again."
  fi
  if pgrep -f "$app/Contents/MacOS/shotmaxxing-desktop" >/dev/null 2>&1; then
    fail "Shotmaxxing is open. Quit it first (Quit Shotmaxxing in its menu-bar menu, or Command-Q), then run the line again."
  fi

  tmp="$(mktemp -d)"
  local file="Shotmaxxing_${VERSION}_${arch}.app.tar.gz"
  say "Downloading Shotmaxxing $VERSION for $kind..."
  curl -fL --proto '=https' --tlsv1.2 --retry 3 --progress-bar -o "$tmp/$file" "$RELEASES/v$VERSION/$file" \
    || fail "the download didn't finish. Check the internet connection, then run the line again."
  local got
  got="$(shasum -a 256 "$tmp/$file" | cut -d' ' -f1)"
  [ "$got" = "$sha" ] || fail "the download doesn't match its SHA-256 (expected $sha, got $got), so it was thrown away."

  mkdir "$tmp/unpacked"
  tar -xzf "$tmp/$file" -C "$tmp/unpacked" || fail "couldn't unpack the download."
  local new="$tmp/unpacked/Shotmaxxing.app"
  [ -x "$new/Contents/MacOS/shotmaxxing-desktop" ] || fail "the download doesn't hold Shotmaxxing.app."
  codesign --verify --deep --strict "$new" 2>/dev/null || fail "the app's signature doesn't check out."

  # The copy already here moves aside first, and comes back if the new one can't go in.
  if [ -e "$app" ]; then
    mv "$app" "$tmp/previous.app" || fail "couldn't move the copy already in $dir aside."
  fi
  if ! mv "$new" "$app"; then
    if [ -e "$tmp/previous.app" ]; then mv "$tmp/previous.app" "$app"; fi
    fail "couldn't put the app in $dir."
  fi

  # The command agents and scripts use, linked into the first folder on the PATH that's yours to change. A file of
  # someone else's is left alone; a link (this script's, or Homebrew's to the same app) is replaced.
  local cli="$app/Contents/MacOS/shotmaxxing" linked="" bin
  for bin in /usr/local/bin /opt/homebrew/bin; do
    case ":$PATH:" in *":$bin:"*) ;; *) continue ;; esac
    if [ ! -d "$bin" ] || [ ! -w "$bin" ]; then continue; fi
    if [ -e "$bin/shotmaxxing" ] && [ ! -L "$bin/shotmaxxing" ]; then continue; fi
    if ln -sfn "$cli" "$bin/shotmaxxing"; then
      linked="$bin/shotmaxxing"
      break
    fi
  done

  say ""
  say "Shotmaxxing $VERSION is installed in $dir."
  if [ -n "$linked" ]; then
    say "The shotmaxxing command is linked at $linked."
  else
    say "The shotmaxxing command is $cli."
  fi
  if [ -n "${SHOTMAXXING_NO_OPEN:-}" ]; then
    say "To update it later, run the same line again."
  else
    say "Opening it now. To update it later, run the same line again."
    open "$app" || say "Open it from $dir."
  fi
}

main "$@"
