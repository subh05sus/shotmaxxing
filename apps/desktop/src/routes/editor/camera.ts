/**
 * Preview maths for the editor, matching crates/core (camera.rs and cursor.rs): zoom regions blend in log space with
 * smootherstep, neighbouring zooms glide into each other instead of zooming out and back in, and the viewport never
 * leaves the frame. The native compositor uses the Rust versions; these keep the browser preview honest.
 */

import type { ZoomBlock } from '../../lib/sample';

export interface Camera {
  scale: number;
  x: number;
  y: number;
}

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

export function smootherstep(t: number): number {
  const x = clamp(t, 0, 1);
  return x * x * x * (x * (x * 6 - 15) + 10);
}

/** Zooms closer together than this glide from one to the next. */
const GLIDE_GAP = 1.4;

function clampCamera(c: Camera, w: number, h: number): Camera {
  const hw = w / (2 * c.scale);
  const hh = h / (2 * c.scale);
  return { scale: c.scale, x: clamp(c.x, hw, w - hw), y: clamp(c.y, hh, h - hh) };
}

export function cameraAt(
  t: number,
  zooms: ZoomBlock[],
  size: { width: number; height: number },
  {
    motion = 'smooth',
    follow,
    cursor,
  }: { motion?: 'smooth' | 'instant'; follow?: boolean; cursor?: { x: number; y: number } } = {},
): Camera {
  const { width: w, height: h } = size;
  const ramp = motion === 'instant' ? 0.001 : 0.7;
  const sorted = [...zooms].sort((a, b) => a.start - b.start);
  for (let i = 0; i < sorted.length; i++) {
    const z = sorted[i];
    const prev = sorted[i - 1];
    const next = sorted[i + 1];
    const joinPrev = !!prev && z.start - prev.end < GLIDE_GAP;
    const joinNext = !!next && next.start - z.end < GLIDE_GAP;
    if (joinNext && t > z.end && t < next.start) {
      const u = smootherstep((t - z.end) / Math.max(0.001, next.start - z.end));
      return clampCamera(
        {
          scale: Math.exp(lerp(Math.log(z.scale), Math.log(next.scale), u)),
          x: lerp(z.focus.x, next.focus.x, u),
          y: lerp(z.focus.y, next.focus.y, u),
        },
        w,
        h,
      );
    }
    if (t < z.start || t > z.end) continue;
    const length = z.end - z.start;
    const r = Math.min(ramp, length / 2);
    const weight = Math.min(
      joinPrev ? 1 : smootherstep((t - z.start) / r),
      joinNext ? 1 : smootherstep((z.end - t) / r),
    );
    let fx = z.focus.x;
    let fy = z.focus.y;
    if (follow && cursor) {
      fx = lerp(fx, cursor.x, 0.35);
      fy = lerp(fy, cursor.y, 0.35);
    }
    return clampCamera(
      { scale: Math.exp(weight * Math.log(z.scale)), x: lerp(w / 2, fx, weight), y: lerp(h / 2, fy, weight) },
      w,
      h,
    );
  }
  return { scale: 1, x: w / 2, y: h / 2 };
}

const minJerk = (u: number) => u * u * u * (10 - 15 * u + 6 * u * u);

/** The cursor along its recorded path: Catmull–Rom through the samples with minimum-jerk timing between them. */
export function cursorAt(t: number, path: { t: number; x: number; y: number }[]): { x: number; y: number } {
  if (!path.length) return { x: 0, y: 0 };
  if (t <= path[0].t) return { x: path[0].x, y: path[0].y };
  const last = path[path.length - 1];
  if (t >= last.t) return { x: last.x, y: last.y };
  let i = 0;
  while (i < path.length - 2 && path[i + 1].t <= t) i++;
  const p0 = path[i - 1] ?? path[i];
  const p1 = path[i];
  const p2 = path[i + 1];
  const p3 = path[i + 2] ?? p2;
  const u = minJerk((t - p1.t) / (p2.t - p1.t));
  const cr = (a: number, b: number, c: number, d: number) =>
    0.5 * (2 * b + (-a + c) * u + (2 * a - 5 * b + 4 * c - d) * u * u + (-a + 3 * b - 3 * c + d) * u * u * u);
  return { x: cr(p0.x, p1.x, p2.x, p3.x), y: cr(p0.y, p1.y, p2.y, p3.y) };
}

/** Where the window sits in the output frame: centred, padded, keeping the capture's aspect. */
export function placeWindow(
  frame: { width: number; height: number },
  capture: { width: number; height: number },
  padding: number,
) {
  const pad = Math.min(frame.width, frame.height) * padding;
  const availW = frame.width - pad * 2;
  const availH = frame.height - pad * 2;
  const aspect = capture.width / capture.height;
  let width = availW;
  let height = width / aspect;
  if (height > availH) {
    height = availH;
    width = height * aspect;
  }
  return { x: (frame.width - width) / 2, y: (frame.height - height) / 2, width, height };
}

export const ASPECTS: Record<string, number | null> = {
  '16:9': 16 / 9,
  '4:3': 4 / 3,
  '1:1': 1,
  '9:16': 9 / 16,
  '4:5': 4 / 5,
  original: null,
};

/** A size in the canvas's shape whose shorter side is `short` pixels, with even sides. */
export function sizeForShort(aspect: number, short: number): { width: number; height: number } {
  const even = (v: number) => Math.max(16, Math.round(v / 2) * 2);
  return aspect >= 1
    ? { width: even(short * aspect), height: even(short) }
    : { width: even(short), height: even(short / aspect) };
}
