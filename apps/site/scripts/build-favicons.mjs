#!/usr/bin/env node
/**
 * Draws the site's favicons and app icons from the Shotmaxxing mark (assets/brand/app-icon.svg): the lime tile with
 * the aperture in ink. Small sizes get the tile edge to edge, with no drop shadow, and a heavier stroke so the mark
 * still reads at 16 px.
 *
 *   node apps/site/scripts/build-favicons.mjs
 *
 * Writes app/icon.svg, app/favicon.ico and app/apple-icon.png (Next links them from every page), and
 * public/icon-192.png, icon-512.png and icon-maskable-512.png for the web manifest (app/manifest.ts).
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { Resvg } from '@resvg/resvg-js';

const site = join(import.meta.dirname, '..');

const blades = [0, 60, 120, 180, 240, 300]
  .map((a) => `<path d="M0 -3.6 L9.79 2.05" transform="rotate(${a})"/>`)
  .join('');

/** The tile: rounded (`radius` of 1024) or a full square for maskable icons, with the mark `scale` big. */
const tile = ({
  radius = 230,
  scale = 34,
  stroke = 1.9,
} = {}) => `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="fill" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#c9fb5a"/>
      <stop offset="1" stop-color="#9fe024"/>
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" rx="${radius}" fill="url(#fill)"/>
  <g transform="translate(512 512) scale(${scale})" fill="none" stroke="#18210c" stroke-width="${stroke}" stroke-linecap="round">
    <circle r="10"/>${blades}
  </g>
</svg>`;

const png = (svg, size) => new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();

/** An .ico holding PNGs, which every current browser reads. */
function ico(images) {
  const head = Buffer.alloc(6 + 16 * images.length);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(images.length, 4);
  let offset = head.length;
  images.forEach(({ size, data }, i) => {
    const at = 6 + 16 * i;
    head.writeUInt8(size >= 256 ? 0 : size, at);
    head.writeUInt8(size >= 256 ? 0 : size, at + 1);
    head.writeUInt16LE(1, at + 4);
    head.writeUInt16LE(32, at + 6);
    head.writeUInt32LE(data.length, at + 8);
    head.writeUInt32LE(offset, at + 12);
    offset += data.length;
  });
  return Buffer.concat([head, ...images.map((i) => i.data)]);
}

const small = tile({ stroke: 2.3 });
const regular = tile();
writeFileSync(join(site, 'app', 'icon.svg'), small);
writeFileSync(
  join(site, 'app', 'favicon.ico'),
  ico([16, 32, 48].map((size) => ({ size, data: png(small, size) }))),
);
// iOS rounds the corners itself, so the touch icon is a full square.
writeFileSync(join(site, 'app', 'apple-icon.png'), png(tile({ radius: 0, scale: 30 }), 180));
writeFileSync(join(site, 'public', 'icon-192.png'), png(regular, 192));
writeFileSync(join(site, 'public', 'icon-512.png'), png(regular, 512));
// Maskable: full bleed, the mark inside the middle 60 % that every mask keeps.
writeFileSync(join(site, 'public', 'icon-maskable-512.png'), png(tile({ radius: 0, scale: 24 }), 512));
console.log('Wrote app/icon.svg, favicon.ico, apple-icon.png and public/icon-*.png');
