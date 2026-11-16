/**
 * What fills the frame behind the window, beyond the gradient wallpapers: the photos that ship with the app, a
 * picture someone chose, a solid colour or a gradient they made, and a blur over any of them. The exporter draws the
 * same (crates/render/src/backdrop.rs): photos come from the same files, solid colours and gradients blend in Oklab
 * as `linear-gradient(in oklab …)` does, and the blur is a Gaussian of the same size.
 *
 * The cursor shapes live here too (crates/render/assets/cursors.json), drawn as SVG in the editor and natively in
 * the export.
 */

import PHOTO_LIST from '../../../../crates/render/assets/photos.json';
import CURSOR_LIST from '../../../../crates/render/assets/cursors.json';

export interface Photo {
  id: string;
  name: string;
}

export const PHOTOS: Photo[] = PHOTO_LIST;

const FULL = import.meta.glob<string>('../../../../crates/render/assets/photos/*.jpg', {
  eager: true,
  query: '?url',
  import: 'default',
});
const THUMBS = import.meta.glob<string>('../assets/photos/*.jpg', {
  eager: true,
  query: '?url',
  import: 'default',
});

const byId = (files: Record<string, string>, id: string) =>
  Object.entries(files).find(([path]) => path.endsWith(`/${id}.jpg`))?.[1] ?? '';

/** The photo at full size (2560 × 1440), for the preview. */
export const photoUrl = (id: string) => byId(FULL, id);
/** A small copy (320 × 180), for the picker. */
export const photoThumb = (id: string) => byId(THUMBS, id);

/** A background other than a gradient wallpaper. Without one, the wallpaper shows. */
export type Backdrop =
  | { kind: 'photo'; id: string }
  | { kind: 'image'; path: string }
  | { kind: 'solid'; color: string }
  | { kind: 'gradient'; from: string; to: string; angle: number };

/** The most blur the editor offers, in pixels at 1080p (as `MOST_BLUR` in the exporter). */
export const MOST_BLUR = 40;

export const isHex = (v: string) => /^#[0-9a-f]{6}$/i.test(v);

/** A two-colour gradient as CSS, blended in Oklab like the export. */
export const gradientCss = (from: string, to: string, angle: number) =>
  `linear-gradient(in oklab ${Math.round(((angle % 360) + 360) % 360)}deg, ${from}, ${to})`;

/**
 * The CSS `background` for `backdrop`. `fileUrl` turns a picture's path inside the project into a URL the web view
 * can load; `fallback` is the wallpaper, shown when the backdrop can't be.
 */
export function backdropCss(
  backdrop: Backdrop | undefined,
  fallback: string,
  fileUrl: (path: string) => string,
) {
  switch (backdrop?.kind) {
    case 'photo': {
      const url = photoUrl(backdrop.id);
      return url ? `center / cover no-repeat url("${url}")` : fallback;
    }
    case 'image': {
      const url = fileUrl(backdrop.path);
      return url ? `#000 center / cover no-repeat url("${url}")` : fallback;
    }
    case 'solid':
      return isHex(backdrop.color) ? backdrop.color : fallback;
    case 'gradient':
      return isHex(backdrop.from) && isHex(backdrop.to)
        ? gradientCss(backdrop.from, backdrop.to, backdrop.angle)
        : fallback;
    default:
      return fallback;
  }
}

/** Colours offered as one-tap swatches for a solid background, and the gradients offered as starting points. */
export const SWATCHES = [
  '#0b0b0f',
  '#1f2937',
  '#f5f5f4',
  '#ffffff',
  '#c6ff00',
  '#22c55e',
  '#14b8a6',
  '#0ea5e9',
  '#2563eb',
  '#7c3aed',
  '#db2777',
  '#ef4444',
  '#f97316',
  '#facc15',
] as const;

/** The colour and gradient the Color and Gradient tabs start from. */
export const FIRST_SOLID = '#f5f5f4';
export const FIRST_GRADIENT = { from: '#c6ff00', to: '#0f766e', angle: 135 } as const;

export const GRADIENTS = [
  { from: '#c6ff00', to: '#0f766e', angle: 135 },
  { from: '#38bdf8', to: '#1e3a8a', angle: 160 },
  { from: '#f472b6', to: '#7c3aed', angle: 135 },
  { from: '#fb923c', to: '#be123c', angle: 120 },
  { from: '#fde68a', to: '#f97316', angle: 180 },
  { from: '#a7f3d0', to: '#0e7490', angle: 200 },
  { from: '#e0e7ff', to: '#6366f1', angle: 135 },
  { from: '#1f2937', to: '#030712', angle: 180 },
] as const;

// Cursors ------------------------------------------------------------------------------------------------------

export type CursorStyle = 'mac' | 'mac-white' | 'windows' | 'outline' | 'dot' | 'ring' | 'bold' | 'hand';

export interface CursorLayer {
  d?: string;
  circle?: [number, number, number];
  fill?: 'light' | 'dark';
  stroke?: 'light' | 'dark';
  width?: number;
  opacity?: number;
}

export interface CursorShape {
  id: CursorStyle;
  name: string;
  /** Where the click lands, in the shape's 20-wide units. */
  hotspot: [number, number];
  layers: CursorLayer[];
}

export const CURSORS = CURSOR_LIST as CursorShape[];

export function cursorShape(id: string): CursorShape {
  return CURSORS.find((c) => c.id === id) ?? CURSORS[0];
}

/**
 * The cursor's size `age` seconds after a press: down to `scale` in the first fifth of `seconds`, then easing back
 * to 1, as `press_at` in the exporter (crates/render/src/scene.rs).
 */
export function pressAt(age: number, scale: number, seconds: number): number {
  if (!(age >= 0 && age < seconds) || seconds <= 0) return 1;
  const u = age / seconds;
  const depth =
    u < 0.2 ? Math.sin((u / 0.2) * (Math.PI / 2)) : 0.5 + 0.5 * Math.cos(((u - 0.2) / 0.8) * Math.PI);
  return 1 - (1 - scale) * depth;
}

/** As `SWAY_MAX_DEG`, `SWAY_WINDOW_S` and `SWAY_SPEED` in the exporter. */
export const SWAY_MAX_DEG = 25;
export const SWAY_WINDOW_S = 0.06;
const SWAY_SPEED = 1200;

/** The lean, in degrees clockwise, for a sideways speed of `vx` capture points a second (as `sway_at`). */
export function swayAngle(vx: number, sway: number): number {
  return Math.min(1, Math.max(0, sway)) * SWAY_MAX_DEG * Math.tanh(vx / SWAY_SPEED);
}
