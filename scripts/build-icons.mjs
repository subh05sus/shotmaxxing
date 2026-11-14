#!/usr/bin/env node
/**
 * Renders the Shotmaxxing aperture mark to PNG: the 1024 px app icon source (fed to `tauri icon`) and the
 * menu-bar / tray template images. Run with `pnpm icons`.
 */
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { Resvg } from '@resvg/resvg-js';

const root = new URL('..', import.meta.url).pathname;
const out = `${root}apps/desktop/src-tauri/icons`;
mkdirSync(out, { recursive: true });

function render(svg, size, file) {
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
  writeFileSync(file, png);
}

render(readFileSync(`${root}assets/brand/app-icon.svg`, 'utf8'), 1024, `${out}/app-icon.png`);
const tray = readFileSync(`${root}assets/brand/tray-template.svg`, 'utf8');
render(tray, 44, `${out}/tray-template.png`);
render(tray, 32, `${out}/tray-32.png`);

// Generate every platform size (icns, ico, Windows Store tiles) from the 1024 source.
execFileSync('pnpm', ['exec', 'tauri', 'icon', `${out}/app-icon.png`, '--output', out], {
  cwd: `${root}apps/desktop`,
  stdio: 'inherit',
});
console.log('Icons written to apps/desktop/src-tauri/icons');
