/**
 * The aperture in the Shotmaxxing mark, as geometry. The mark is a ring (radius 10) around a regular hexagon
 * (circumradius 3.6) whose six sides run on to the ring: those are the blade edges. Growing the hexagon opens the
 * iris (past the ring the blades have folded away), shrinking it closes it, and scaling everything up flies the
 * viewer through the opening. Units are the mark's own (24 across), mapped to pixels by `unit`.
 */

export const RING = 10;
export const LOGO_R = 3.6;
export const STROKE = 1.9;
/** The hexagon's size when its sides touch the ring: the blades have folded away, the iris is wide open. */
export const OPEN_R = RING / Math.cos(Math.PI / 6);
/** The icon's tile around the mark: the mark is 66% of the tile, its corners 28% of its side. */
export const TILE = 24 / 0.66;

export interface Iris {
  /** The centre, in pixels. */
  cx: number;
  cy: number;
  /** Pixels per unit of the mark. */
  unit: number;
  /** Everything scaled up around the centre: 1 is the mark at its size. */
  scale: number;
  /** The hexagon's circumradius, in units. */
  r: number;
  /** How far the hexagon has turned, in degrees. */
  phi: number;
}

type Point = [number, number];

const f = (n: number) => (Math.round(n * 100) / 100).toString();

export function ringRadius(iris: Iris): number {
  return RING * iris.unit * iris.scale;
}

function vertices(iris: Iris): Point[] {
  const k = iris.r * iris.unit * iris.scale;
  return Array.from({ length: 6 }, (_, n) => {
    const a = ((-90 + 60 * n + iris.phi) * Math.PI) / 180;
    return [iris.cx + k * Math.cos(a), iris.cy + k * Math.sin(a)] as Point;
  });
}

/** Where the line from `p` along `d` crosses the ring, as two distances along `d`; null when it misses. */
function crossings(p: Point, d: Point, iris: Iris): [number, number] | null {
  const R = ringRadius(iris);
  const px = p[0] - iris.cx;
  const py = p[1] - iris.cy;
  const a = d[0] * d[0] + d[1] * d[1];
  if (a < 1e-9) return null;
  const b = 2 * (px * d[0] + py * d[1]);
  const c = px * px + py * py - R * R;
  const disc = b * b - 4 * a * c;
  if (disc <= 0) return null;
  const s = Math.sqrt(disc);
  return [(-b - s) / (2 * a), (-b + s) / (2 * a)];
}

const along = (p: Point, d: Point, t: number): Point => [p[0] + d[0] * t, p[1] + d[1] * t];

export function circlePath(cx: number, cy: number, r: number): string {
  return `M${f(cx - r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx + r)} ${f(cy)}A${f(r)} ${f(r)} 0 1 1 ${f(cx - r)} ${f(cy)}Z`;
}

/**
 * The opening: the hexagon, cut off by the ring where its corners pass it, and the whole disc once the blades have
 * folded away. An SVG subpath, to cut out of a fill with `fill-rule="evenodd"`.
 */
export function holePath(iris: Iris): string {
  const R = ringRadius(iris);
  const r = iris.r * iris.unit * iris.scale;
  if (r < 0.05) return '';
  if (r * Math.cos(Math.PI / 6) >= R) return circlePath(iris.cx, iris.cy, R);
  const v = vertices(iris);
  if (r <= R) return `M${v.map((p) => `${f(p[0])} ${f(p[1])}`).join('L')}Z`;
  // The corners stick out past the ring: each side's chord inside it, joined by arcs of the ring (clockwise, the
  // way the corners go round).
  const chords = v.map((p, n) => {
    const q = v[(n + 1) % 6];
    const d: Point = [q[0] - p[0], q[1] - p[1]];
    const [enter, leave] = crossings(p, d, iris) ?? [0, 1];
    return [along(p, d, enter), along(p, d, leave)] as const;
  });
  let path = '';
  chords.forEach(([enter, leave], n) => {
    path +=
      n === 0 ? `M${f(enter[0])} ${f(enter[1])}` : `A${f(R)} ${f(R)} 0 0 1 ${f(enter[0])} ${f(enter[1])}`;
    path += `L${f(leave[0])} ${f(leave[1])}`;
  });
  const first = chords[0][0];
  return `${path}A${f(R)} ${f(R)} 0 0 1 ${f(first[0])} ${f(first[1])}Z`;
}

/** The six blade edges: each side of the hexagon, run on to the ring. Empty once the iris is wide open. */
export function bladePath(iris: Iris): string {
  const v = vertices(iris);
  let path = '';
  v.forEach((p, n) => {
    const q = v[(n + 1) % 6];
    const d: Point = [q[0] - p[0], q[1] - p[1]];
    const hit = crossings(p, d, iris);
    if (!hit) return;
    const from = Math.max(0, hit[0]);
    if (hit[1] <= from + 1e-4) return;
    const a = along(p, d, from);
    const b = along(p, d, hit[1]);
    path += `M${f(a[0])} ${f(a[1])}L${f(b[0])} ${f(b[1])}`;
  });
  return path;
}

/** The icon's rounded tile around the mark, scaled with it. */
export function tilePath(iris: Iris, size = 1): string {
  const side = TILE * iris.unit * iris.scale * size;
  const h = side / 2;
  const k = side * 0.28;
  const x = iris.cx - h;
  const y = iris.cy - h;
  return (
    `M${f(x + k)} ${f(y)}H${f(x + side - k)}A${f(k)} ${f(k)} 0 0 1 ${f(x + side)} ${f(y + k)}` +
    `V${f(y + side - k)}A${f(k)} ${f(k)} 0 0 1 ${f(x + side - k)} ${f(y + side)}` +
    `H${f(x + k)}A${f(k)} ${f(k)} 0 0 1 ${f(x)} ${f(y + side - k)}V${f(y + k)}A${f(k)} ${f(k)} 0 0 1 ${f(x + k)} ${f(y)}Z`
  );
}

/**
 * How much to scale the mark so its ring, stroke and all, clears every corner of a `width` × `height` view: the
 * inner edge of the ring (half a stroke inside it) ends a little past the farthest corner.
 */
export function flyScale(width: number, height: number, unit: number): number {
  return (1.06 * Math.hypot(width / 2, height / 2)) / ((RING - STROKE / 2) * unit);
}

// Easing ---------------------------------------------------------------------------------------------------------

export const ease = {
  linear: (x: number) => x,
  outCubic: (x: number) => 1 - (1 - x) ** 3,
  outQuart: (x: number) => 1 - (1 - x) ** 4,
  inCubic: (x: number) => x ** 3,
  inQuart: (x: number) => x ** 4,
  inOutCubic: (x: number) => (x < 0.5 ? 4 * x ** 3 : 1 - (-2 * x + 2) ** 3 / 2),
  inOutSine: (x: number) => -(Math.cos(Math.PI * x) - 1) / 2,
  /** Overshoots a little, then settles: a part locking into place. */
  outBack: (x: number) => {
    const c = 1.9;
    return 1 + (c + 1) * (x - 1) ** 3 + c * (x - 1) ** 2;
  },
};

/** Progress through [from, to] (in ms) at time `t`, 0 to 1, eased. */
export function seg(t: number, from: number, to: number, curve: (x: number) => number = ease.linear): number {
  if (t <= from) return 0;
  if (t >= to) return 1;
  return curve((t - from) / (to - from));
}

export const lerp = (a: number, b: number, k: number) => a + (b - a) * k;

/** Zooming by `k` of the way from scale 1 to `end`, evenly to the eye (geometric, not linear). */
export const zoom = (end: number, k: number) => Math.exp(Math.log(end) * k);
