import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** The repository root, from apps/site. */
export const ROOT = join(process.cwd(), '..', '..');
export const REPO = 'subh05sus/shotmaxxing';
export const GITHUB = `https://github.com/${REPO}`;
const RAW = `https://raw.githubusercontent.com/${REPO}/main`;

/**
 * The release the installers point at. install.sh is the source of truth (CI checks that the PowerShell installer,
 * the cask, winget and the README name the same one), so the site can't advertise another.
 */
export function release() {
  const script = readFileSync(join(ROOT, 'install.sh'), 'utf8');
  const version = /^VERSION="([^"]+)"$/m.exec(script)?.[1];
  if (!version) throw new Error('install.sh has no VERSION line');
  const base = `${GITHUB}/releases/download/v${version}`;
  const file = (suffix: string) => ({
    name: `Shotmaxxing_${version}_${suffix}`,
    url: `${base}/Shotmaxxing_${version}_${suffix}`,
  });
  return {
    version,
    notes: `${GITHUB}/releases/tag/v${version}`,
    all: `${GITHUB}/releases`,
    files: {
      macArm: file('aarch64.dmg'),
      macIntel: file('x64.dmg'),
      winSetup: file('x64-setup.exe'),
      winMsi: file('x64_en-US.msi'),
    },
    commands: {
      mac: `curl -fsSL ${RAW}/install.sh | bash`,
      windows: `irm ${RAW}/install.ps1 | iex`,
      brew: [`brew tap ${REPO} https://github.com/${REPO}`, `brew install --cask ${REPO}/shotmaxxing`],
    },
  };
}

export type Release = ReturnType<typeof release>;
