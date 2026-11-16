'use client';

/**
 * The page loader: the app's first-launch iris (apps/desktop/src/components/IrisOverlay.svelte), drawn with the same
 * geometry (lib/aperture, shared with the app) and the same timeline.
 *
 * - intro (3.2 s, a visitor's first visit): a point of lime light is born in the middle, rises to the rim and draws
 *   the ring; six blades swing in, turning, and hit their stop; light floods out from the iris and fills the icon's
 *   tile, and the name appears. Then the iris opens and the view flies through it onto the page.
 * - launch (0.7 s, every later visit): the ring and blades snap together and open onto the page.
 * - fade (Reduce Motion, first visit): the finished icon fades in, holds and fades away. Later visits skip it.
 *
 * It draws with requestAnimationFrame only while playing and is gone at the end. Any click, key, wheel or touch skips
 * it, and it never holds the page's scrolling. The page is in the HTML underneath the whole time.
 */
import { useEffect, useRef, useState } from 'react';
import {
  LOGO_R,
  OPEN_R,
  RING,
  STROKE,
  TILE,
  bladePath,
  ease,
  flyScale,
  holePath,
  lerp,
  seg,
  tilePath,
  zoom,
  type Iris,
} from '@/lib/aperture';
import { markRevealed } from '@/lib/reveal';

type Mode = 'intro' | 'launch' | 'fade';

interface Frame {
  iris: Iris;
  ring: number;
  blades: number;
  mark: number;
  hole: boolean;
  lit: number;
  tile: number;
  tileScale: number;
  glow: number;
  glowScale: number;
  pen: number;
  penSize: number;
  penRise: number;
  shock: number;
  shockR: number;
  word: number;
  wordY: number;
  all: number;
}

const HIT = 1540;
const REVEAL: Record<Mode, number> = { intro: 2540, launch: 330, fade: 1250 };
const DONE: Record<Mode, number> = { intro: 3240, launch: 690, fade: 1560 };
const SEEN = 'shotmaxxing-site-intro';

const strike = (x: number) => (1 - Math.cos(Math.PI * 0.85 * x)) / (1 - Math.cos(Math.PI * 0.85));
const rebound = (ms: number) => (ms <= 0 ? 0 : Math.sin((2 * Math.PI * ms) / 240) * Math.exp(-ms / 90));

const base = (): Omit<Frame, 'iris'> => ({
  ring: 1,
  blades: 1,
  mark: 1,
  hole: false,
  lit: 0,
  tile: 1,
  tileScale: 1,
  glow: 0,
  glowScale: 1,
  pen: 0,
  penSize: 1,
  penRise: 1,
  shock: 0,
  shockR: RING,
  word: 0,
  wordY: 0,
  all: 1,
});

function stage(width: number, height: number, kind: 'big' | 'small') {
  const unit =
    kind === 'big'
      ? Math.min(6, Math.max(3.6, Math.min(width, height) * 0.0072))
      : Math.min(4.4, Math.max(3, Math.min(width, height) * 0.0052));
  const cx = width / 2;
  const cy = kind === 'big' ? height * 0.46 : height / 2;
  const fly = flyScale(width, 2 * Math.max(cy, height - cy), unit);
  return { unit, cx, cy, fly };
}

function introFrame(t: number, w: number, h: number): Frame {
  const { unit, cx, cy, fly } = stage(w, h, 'big');
  const close = seg(t, 860, HIT, strike);
  let r = lerp(OPEN_R, LOGO_R, close) - 0.22 * rebound(t - HIT);
  let phi = lerp(-70, 0, close) + 4 * rebound(t - HIT);
  const open = seg(t, 2480, 3180, ease.inOutCubic);
  if (t > 2480) {
    r = lerp(LOGO_R, 13.5, open);
    phi = 32 * open;
  }
  const pulse =
    t < HIT ? 0 : t < HIT + 70 ? seg(t, HIT, HIT + 70) : 1 - seg(t, HIT + 70, HIT + 260, ease.inOutSine);
  const scale = zoom(fly, seg(t, 2480, 3220, ease.inCubic)) * (1 + 0.03 * pulse);
  const shockK = seg(t, HIT, HIT + 460, ease.outCubic);
  return {
    ...base(),
    iris: { cx, cy, unit, scale, r, phi },
    ring: seg(t, 420, 1100, ease.inOutCubic),
    blades: seg(t, 860, 960),
    hole: t >= 2480,
    lit: seg(t, HIT + 50, HIT + 470, ease.outCubic) * (1 - seg(t, 2500, 2800, ease.inOutCubic)),
    tileScale: lerp(0.9, 1, seg(t, HIT + 50, HIT + 470, ease.outBack)),
    glow: 0.9 * seg(t, HIT + 40, HIT + 640, ease.outCubic) * (1 - seg(t, 2520, 2860)),
    glowScale: lerp(0.7, 1, seg(t, HIT + 40, HIT + 700, ease.outCubic)),
    pen: seg(t, 0, 200, ease.outCubic) * (1 - seg(t, 1040, 1180)),
    penSize: lerp(0.35, 1, seg(t, 0, 260, ease.outBack)),
    penRise: seg(t, 200, 440, ease.inOutCubic),
    shock: t >= HIT ? 0.5 * (1 - shockK) : 0,
    shockR: RING + 10 * shockK,
    word: seg(t, 1880, 2200, ease.outCubic) * (1 - seg(t, 2470, 2600)),
    wordY: lerp(10, 0, seg(t, 1880, 2240, ease.outCubic)),
  };
}

function launchFrame(t: number, w: number, h: number): Frame {
  const { unit, cx, cy, fly } = stage(w, h, 'small');
  const close = seg(t, 40, 300, ease.outCubic);
  let r = lerp(OPEN_R, LOGO_R, close);
  let phi = lerp(-45, 0, close);
  if (t > 300) {
    const open = seg(t, 300, 640, ease.inOutCubic);
    r = lerp(LOGO_R, 13.5, open);
    phi = 25 * open;
  }
  return {
    ...base(),
    iris: { cx, cy, unit, scale: zoom(fly, seg(t, 300, 670, ease.inCubic)), r, phi },
    ring: seg(t, 0, 250, ease.outCubic),
    blades: seg(t, 40, 120),
    hole: t >= 300,
    pen: seg(t, 0, 60) * (1 - seg(t, 200, 300)),
  };
}

function fadeFrame(t: number, w: number, h: number): Frame {
  const { unit, cx, cy } = stage(w, h, 'big');
  const shown = seg(t, 0, 300, ease.outCubic);
  return {
    ...base(),
    iris: { cx, cy, unit, scale: 1, r: LOGO_R, phi: 0 },
    mark: 0,
    lit: 1,
    tile: shown,
    word: shown,
    all: 1 - seg(t, 1250, 1550),
  };
}

const FRAMES = { intro: introFrame, launch: launchFrame, fade: fadeFrame };

function pickMode(): Mode | null {
  let seen = false;
  try {
    seen = localStorage.getItem(SEEN) === '1';
    localStorage.setItem(SEEN, '1');
  } catch {
    // Storage can be off (a private window): every visit is a first one then.
  }
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (reduced) return seen ? null : 'fade';
  return seen ? 'launch' : 'intro';
}

export function IrisLoader() {
  const root = useRef<HTMLDivElement>(null);
  const [gone, setGone] = useState(false);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const q = <T extends Element>(name: string) => el.querySelector<T>(`[data-part="${name}"]`)!;
    const svg = q<SVGSVGElement>('svg');
    const stagePath = q<SVGPathElement>('stage');
    const glow = q<SVGCircleElement>('glow');
    const markGroup = q<SVGGElement>('mark');
    const ring = q<SVGCircleElement>('ring');
    const blades = q<SVGPathElement>('blades');
    const tileGroup = q<SVGGElement>('tile');
    const tilePathEl = q<SVGPathElement>('tile-path');
    const tileRing = q<SVGCircleElement>('tile-ring');
    const tileBlades = q<SVGPathElement>('tile-blades');
    const litClip = q<SVGCircleElement>('lit');
    const shock = q<SVGCircleElement>('shock');
    const penGlow = q<SVGCircleElement>('pen-glow');
    const pen = q<SVGCircleElement>('pen');
    const word = q<HTMLParagraphElement>('word');

    const mode = pickMode();
    if (!mode) {
      markRevealed();
      setGone(true);
      return;
    }

    let raf = 0;
    let start = 0;
    let revealed = false;
    let finished = false;

    const set = (node: Element, attrs: Record<string, string | number>) => {
      for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
    };

    const draw = (t: number) => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      const frame = FRAMES[mode](t, w, h);
      const i = frame.iris;
      const k = i.unit * i.scale;
      const R = RING * k;
      const hole = frame.hole ? holePath(i) : '';
      const circumference = 2 * Math.PI * R;
      const tip = ((-90 + 360 * frame.ring) * Math.PI) / 180;
      const tileSide = TILE * k * frame.tileScale;
      const bladeD = frame.blades > 0 ? bladePath(i) : '';

      set(svg, { width: w, height: h, viewBox: `0 0 ${w} ${h}` });
      el.style.opacity = String(frame.all);
      stagePath.setAttribute('d', `M0 0H${w}V${h}H0Z${hole}`);

      glow.style.display = frame.glow > 0 ? '' : 'none';
      set(glow, { cx: i.cx, cy: i.cy, r: 30 * k * frame.glowScale, opacity: frame.glow });

      set(markGroup, { opacity: frame.mark, 'stroke-width': STROKE * k });
      ring.style.visibility = frame.ring > 0 ? 'visible' : 'hidden';
      set(ring, {
        cx: i.cx,
        cy: i.cy,
        r: R,
        'stroke-dasharray': `${circumference * frame.ring} ${circumference}`,
        transform: `rotate(-90 ${i.cx} ${i.cy})`,
      });
      set(blades, { d: bladeD, opacity: frame.blades });

      const showTile = frame.lit > 0 && frame.tile > 0;
      tileGroup.style.display = showTile ? '' : 'none';
      if (showTile) {
        set(tileGroup, { opacity: frame.tile, 'stroke-width': STROKE * k });
        tilePathEl.setAttribute('d', tilePath(i, frame.tileScale) + hole);
        set(tileRing, { cx: i.cx, cy: i.cy, r: R });
        tileBlades.setAttribute('d', bladeD);
        set(litClip, { cx: i.cx, cy: i.cy, r: 0.6 * tileSide * frame.lit });
      }

      shock.style.display = frame.shock > 0 ? '' : 'none';
      set(shock, { cx: i.cx, cy: i.cy, r: frame.shockR * k, 'stroke-width': 0.5 * k, opacity: frame.shock });

      const showPen = frame.pen > 0;
      penGlow.style.display = pen.style.display = showPen ? '' : 'none';
      if (showPen) {
        const px = i.cx + R * Math.cos(tip) * frame.penRise;
        const py = i.cy + R * Math.sin(tip) * frame.penRise;
        set(penGlow, { cx: px, cy: py, r: 6 * k * frame.penSize, opacity: frame.pen });
        set(pen, { cx: px, cy: py, r: 1.15 * k * frame.penSize, opacity: frame.pen });
      }

      word.style.display = frame.word > 0 ? '' : 'none';
      word.style.top = `${i.cy + (TILE / 2 + 5) * i.unit}px`;
      word.style.opacity = String(frame.word);
      word.style.transform = `translateY(${frame.wordY}px)`;
    };

    const finish = () => {
      if (finished) return;
      finished = true;
      cancelAnimationFrame(raf);
      removeListeners();
      if (!revealed) markRevealed();
      setGone(true);
    };

    const tick = (now: number) => {
      if (!start) start = now;
      const t = now - start;
      draw(t);
      if (!revealed && t >= REVEAL[mode]) {
        revealed = true;
        markRevealed();
      }
      if (t >= DONE[mode]) return finish();
      raf = requestAnimationFrame(tick);
    };

    // Any input skips it: a quick fade, like the app's.
    const skip = () => {
      if (finished) return;
      if (!revealed) {
        revealed = true;
        markRevealed();
      }
      el.dataset.skipping = '';
      window.setTimeout(finish, 200);
    };
    const events = ['pointerdown', 'keydown', 'wheel', 'touchmove'] as const;
    const removeListeners = () => events.forEach((e) => window.removeEventListener(e, skip));
    events.forEach((e) => window.addEventListener(e, skip, { passive: true }));

    draw(0);
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      removeListeners();
    };
  }, []);

  if (gone) return null;

  return (
    <div
      ref={root}
      className="iris-loader fixed inset-0 z-[1000] data-[skipping]:opacity-0! data-[skipping]:transition-opacity data-[skipping]:duration-200"
      aria-hidden="true"
    >
      <svg data-part="svg" className="absolute inset-0 h-full w-full">
        <defs>
          <radialGradient id="iris-glow">
            <stop offset="0" stopColor="var(--lime)" stopOpacity="0.5" />
            <stop offset="0.45" stopColor="var(--lime)" stopOpacity="0.16" />
            <stop offset="1" stopColor="var(--lime)" stopOpacity="0" />
          </radialGradient>
          <linearGradient id="iris-tile" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--icon-tile-top)" />
            <stop offset="1" stopColor="var(--icon-tile-bottom)" />
          </linearGradient>
          <clipPath id="iris-lit">
            <circle data-part="lit" />
          </clipPath>
        </defs>
        {/* Before the first frame the stage covers the page, so nothing flashes through. */}
        <path data-part="stage" d="M0 0H10000V10000H0Z" className="fill-stage" fillRule="evenodd" />
        <circle data-part="glow" fill="url(#iris-glow)" style={{ display: 'none' }} />
        <g fill="none" strokeLinecap="round">
          <g data-part="mark" className="stroke-lime">
            <circle data-part="ring" style={{ visibility: 'hidden' }} />
            <path data-part="blades" />
          </g>
          <g data-part="tile" clipPath="url(#iris-lit)" style={{ display: 'none' }}>
            <path data-part="tile-path" fill="url(#iris-tile)" stroke="none" fillRule="evenodd" />
            <g className="stroke-lime-ink">
              <circle data-part="tile-ring" />
              <path data-part="tile-blades" />
            </g>
          </g>
          <circle data-part="shock" className="stroke-lime" style={{ display: 'none' }} />
        </g>
        <circle data-part="pen-glow" fill="url(#iris-glow)" style={{ display: 'none' }} />
        <circle data-part="pen" className="fill-lime" style={{ display: 'none' }} />
      </svg>
      <p
        data-part="word"
        className="font-display absolute inset-x-0 text-center text-[22px] font-semibold tracking-[-0.02em] text-paper"
        style={{ display: 'none' }}
      >
        Shotmaxxing
      </p>
    </div>
  );
}
