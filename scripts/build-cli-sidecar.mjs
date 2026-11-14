// Builds the `shotmaxxing` command in release mode and puts it where the app bundle picks it up as a sidecar:
// apps/desktop/src-tauri/binaries/shotmaxxing-<target triple>. Agents start this command to reach Shotmaxxing, and
// the app connects them to the copy inside it.
//
// Usage: node scripts/build-cli-sidecar.mjs [--target <triple>]

import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const at = process.argv.indexOf('--target');
const target = at > 0 ? process.argv[at + 1] : undefined;
const host = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (\S+)$/m)?.[1];
const triple = target ?? host;
if (!triple) throw new Error('Couldn’t tell which platform to build for. Pass --target <triple>.');

const args = ['build', '--release', '--locked', '-p', 'shotmaxxing-cli'];
execFileSync('cargo', target ? [...args, '--target', target] : args, { cwd: root, stdio: 'inherit' });

const exe = triple.includes('windows') ? '.exe' : '';
const built = join(root, 'target', ...(target ? [target] : []), 'release', `shotmaxxing${exe}`);
const dir = join(root, 'apps/desktop/src-tauri/binaries');
mkdirSync(dir, { recursive: true });
copyFileSync(built, join(dir, `shotmaxxing-${triple}${exe}`));
console.log(`The command is ready for bundling: binaries/shotmaxxing-${triple}${exe}`);
