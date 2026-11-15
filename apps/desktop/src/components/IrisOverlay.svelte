<script lang="ts">
  /**
   * The logo's iris over the whole window (see lib/iris.svelte.ts). Three timelines share one drawing:
   *
   * - intro (3.2 s, first launch): a point of lime light is born in the middle, rises to the rim and draws the
   *   ring; six blades swing in, turning, and hit their stop with a soft click; light floods out from the iris and
   *   fills the icon's tile, and the name appears. Then the iris opens and the view flies through it onto the
   *   welcome.
   * - launch (0.7 s, every start): the ring and blades snap together and open onto the Library. No sound.
   * - finale (1.5 s, end of setup): the shutter closes in from the edges, clicks, and opens on the Library.
   *
   * It draws with requestAnimationFrame only while playing and stops at the end: nothing runs while you read. Any
   * key or click skips the intro and the launch build. Reduce Motion gets a fade, Low Power Mode nothing at all.
   */
  import { untrack } from 'svelte';
  import { env } from '@shotmaxxing/ui';
  import { iris, irisEvent, type IrisKind } from '../lib/iris.svelte';
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
  } from '../lib/aperture';
  import { playClick } from '../lib/sound';

  interface Frame {
    iris: Iris;
    /** How much of the ring is drawn, 0 to 1. */
    ring: number;
    blades: number;
    /** The mark in lime on the stage (the fade shows only the finished icon). */
    mark: number;
    hole: boolean;
    /** How far the light has spread from the iris over the tile, 0 to 1 (1 fills it). */
    lit: number;
    /** The tile's opacity once lit (it fades as the view flies through). */
    tile: number;
    tileScale: number;
    glow: number;
    glowScale: number;
    /** The point of light: how bright, how big, and how far from the middle to the ring's drawing tip. */
    pen: number;
    penSize: number;
    penRise: number;
    shock: number;
    shockR: number;
    word: number;
    wordY: number;
    /** The whole overlay's opacity (the Reduce Motion fade). */
    all: number;
  }

  type Cue = { at: number; event: 'click' | 'reveal' | 'closed' | 'done' };

  /** When the blades hit their stop in the intro. */
  const HIT = 1540;
  /** How far the finale's shutter closes: the blades' strokes cover what's left of the opening. */
  const SHUT = 1.7;

  const CUES: Record<IrisKind | 'fade', Cue[]> = {
    intro: [
      { at: HIT, event: 'click' },
      { at: 2540, event: 'reveal' },
      { at: 3240, event: 'done' },
    ],
    launch: [
      { at: 330, event: 'reveal' },
      { at: 690, event: 'done' },
    ],
    finale: [
      { at: 490, event: 'click' },
      { at: 540, event: 'closed' },
      { at: 760, event: 'reveal' },
      { at: 1320, event: 'done' },
    ],
    fade: [
      { at: 1250, event: 'reveal' },
      { at: 1560, event: 'done' },
    ],
  };

  let width = $state(window.innerWidth);
  let height = $state(window.innerHeight);
  let t = $state(0);
  let skipping = $state(false);
  /** The timeline playing: a kind, or the Reduce Motion fade of the intro. */
  let mode = $state<IrisKind | 'fade' | null>(null);

  let raf = 0;
  let fired = new Set<string>();
  let paused = false;

  function fire(cue: Cue) {
    if (fired.has(cue.event)) return;
    fired.add(cue.event);
    if (cue.event === 'click') playClick(mode === 'finale' ? 'shutter' : 'lock');
    else irisEvent(cue.event);
  }

  function cueUpTo(now: number) {
    if (!mode) return;
    for (const cue of CUES[mode]) if (now >= cue.at) fire(cue);
  }

  // A new play: pick the timeline, or skip it when motion should stay out of the way.
  $effect(() => {
    const kind = iris.kind;
    void iris.run;
    cancelAnimationFrame(raf);
    fired = new Set();
    skipping = false;
    paused = false;
    t = 0;
    if (!kind) {
      mode = null;
      return;
    }
    // Read once: a power reading arriving mid-play mustn't start it over (the effect below ends it instead).
    const { lowPower, reducedMotion } = untrack(() => ({
      lowPower: env.lowPower,
      reducedMotion: env.reducedMotion,
    }));
    if (lowPower || (reducedMotion && kind !== 'intro')) {
      // Straight on: the finale still swaps what's behind it, without the shutter.
      mode = null;
      if (kind === 'finale') irisEvent('closed');
      irisEvent('done');
      return;
    }
    mode = reducedMotion ? 'fade' : kind;
    const start = performance.now();
    const tick = (now: number) => {
      if (paused || !mode) return;
      t = now - start;
      cueUpTo(t);
      if (fired.has('done')) return;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  // Low Power Mode turning on mid-intro ends it at once.
  $effect(() => {
    if (env.lowPower && mode && mode !== 'finale') skip();
  });

  function skip() {
    if (!mode || mode === 'finale' || skipping) return;
    skipping = true;
    irisEvent('reveal');
    setTimeout(() => {
      cancelAnimationFrame(raf);
      mode = null;
      irisEvent('done');
    }, 200);
  }

  // Frames ----------------------------------------------------------------------------------------------------------

  /** Starts slowly and arrives still moving, so the blades hit their stop rather than drift into it. */
  const strike = (x: number) => (1 - Math.cos(Math.PI * 0.85 * x)) / (1 - Math.cos(Math.PI * 0.85));
  /** The quick, damped wobble after the hit: 0 at the hit, inward first, gone in about a third of a second. */
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

  /** The centre, the mark's size, and how far it scales to clear the window. */
  function stage(kind: 'big' | 'small') {
    const unit =
      kind === 'big'
        ? Math.min(6, Math.max(3.6, Math.min(width, height) * 0.0072))
        : Math.min(4.4, Math.max(3, Math.min(width, height) * 0.0052));
    const cx = width / 2;
    const cy = kind === 'big' ? height * 0.46 : height / 2;
    // The corner farthest from the centre sets how far the ring must grow.
    const fly = flyScale(width, 2 * Math.max(cy, height - cy), unit);
    return { unit, cx, cy, fly };
  }

  function introFrame(t: number): Frame {
    const { unit, cx, cy, fly } = stage('big');
    // The blades swing in, turning, hit their stop and settle with a small wobble.
    const close = seg(t, 860, HIT, strike);
    let r = lerp(OPEN_R, LOGO_R, close) - 0.22 * rebound(t - HIT);
    let phi = lerp(-70, 0, close) + 4 * rebound(t - HIT);
    // Then the iris opens and the view flies through it.
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
      // Light floods out from the iris over the tile, and draws back into the lens as it opens.
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

  function launchFrame(t: number): Frame {
    const { unit, cx, cy, fly } = stage('small');
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

  function finaleFrame(t: number): Frame {
    const { unit, cx, cy, fly } = stage('small');
    let r: number;
    let phi: number;
    let scale: number;
    // The blades close almost to a point (their strokes cover what's left of the opening), hold a beat while the
    // page behind changes, and open again.
    if (t <= 660) {
      const k = seg(t, 0, 500, ease.inOutCubic);
      r = lerp(13.5, SHUT, k);
      phi = lerp(30, 0, k);
      scale = zoom(fly, 1 - seg(t, 0, 500, ease.outCubic));
    } else {
      const k = seg(t, 660, 1260, ease.inOutCubic);
      r = lerp(SHUT, 13.5, k);
      phi = 30 * k;
      scale = zoom(fly, seg(t, 660, 1300, ease.inCubic));
    }
    const shockK = seg(t, 490, 890, ease.outCubic);
    return {
      ...base(),
      iris: { cx, cy, unit, scale, r, phi },
      hole: r > SHUT + 0.1,
      shock: t >= 490 ? 0.45 * (1 - shockK) : 0,
      shockR: RING + 8 * shockK,
    };
  }

  /** Reduce Motion: the finished icon fades in, holds, and fades away. */
  function fadeFrame(t: number): Frame {
    const { unit, cx, cy } = stage('big');
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

  const frame = $derived.by((): Frame | null => {
    if (!mode) return null;
    if (mode === 'intro') return introFrame(t);
    if (mode === 'launch') return launchFrame(t);
    if (mode === 'finale') return finaleFrame(t);
    return fadeFrame(t);
  });

  // The drawing, in pixels.
  const drawn = $derived.by(() => {
    if (!frame) return null;
    const i = frame.iris;
    const k = i.unit * i.scale;
    const R = RING * k;
    const hole = frame.hole ? holePath(i) : '';
    const circumference = 2 * Math.PI * R;
    const tip = ((-90 + 360 * frame.ring) * Math.PI) / 180;
    const tileSide = TILE * k * frame.tileScale;
    return {
      stage: `M0 0H${width}V${height}H0Z${hole}`,
      // The tile is lit from the iris outward: a disc of light that ends past its rounded corners.
      tile: frame.lit > 0 && frame.tile > 0 ? tilePath(i, frame.tileScale) + hole : '',
      litR: 0.6 * tileSide * frame.lit,
      blades: frame.blades > 0 ? bladePath(i) : '',
      R,
      stroke: STROKE * k,
      dash: `${circumference * frame.ring} ${circumference}`,
      penX: i.cx + R * Math.cos(tip) * frame.penRise,
      penY: i.cy + R * Math.sin(tip) * frame.penRise,
      k,
      wordTop: i.cy + (TILE / 2 + 5) * i.unit,
    };
  });

  // Development: freeze any moment to look at it (window.__iris.seek(ms)), and see the cues fire as they would.
  if (import.meta.env.DEV) {
    Object.assign(window, {
      __iris: {
        seek(ms: number) {
          paused = true;
          cancelAnimationFrame(raf);
          t = ms;
          if (mode)
            for (const cue of CUES[mode])
              if ((cue.event === 'reveal' || cue.event === 'closed') && ms >= cue.at) fire(cue);
        },
      },
    });
  }
</script>

<svelte:window onkeydown={() => skip()} onpointerdown={() => skip()} />

{#if frame && drawn}
  <div
    class="iris fixed inset-0 z-[950]"
    class:skipping
    style:opacity={frame.all}
    bind:clientWidth={width}
    bind:clientHeight={height}
    aria-hidden="true"
  >
    <svg {width} {height} viewBox="0 0 {width} {height}" class="absolute inset-0">
      <defs>
        <radialGradient id="iris-glow">
          <stop offset="0" class="stop-accent" stop-opacity="0.5" />
          <stop offset="0.45" class="stop-accent" stop-opacity="0.16" />
          <stop offset="1" class="stop-accent" stop-opacity="0" />
        </radialGradient>
        <linearGradient id="iris-tile" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" class="stop-tile-top" />
          <stop offset="1" class="stop-tile-bottom" />
        </linearGradient>
        <clipPath id="iris-lit">
          <circle cx={frame.iris.cx} cy={frame.iris.cy} r={drawn.litR} />
        </clipPath>
      </defs>

      <path d={drawn.stage} class="fill-stage" fill-rule="evenodd" />
      {#if frame.glow > 0}
        <circle
          cx={frame.iris.cx}
          cy={frame.iris.cy}
          r={30 * drawn.k * frame.glowScale}
          fill="url(#iris-glow)"
          opacity={frame.glow}
        />
      {/if}

      <!-- The mark in lime on the stage; where the tile is lit, the tile covers it and the mark is drawn in ink. -->
      <g fill="none" stroke-linecap="round" stroke-width={drawn.stroke}>
        <g class="stroke-accent" opacity={frame.mark}>
          <circle
            visibility={frame.ring > 0 ? 'visible' : 'hidden'}
            cx={frame.iris.cx}
            cy={frame.iris.cy}
            r={drawn.R}
            stroke-dasharray={drawn.dash}
            transform="rotate(-90 {frame.iris.cx} {frame.iris.cy})"
          />
          {#if drawn.blades}<path d={drawn.blades} opacity={frame.blades} />{/if}
        </g>
        {#if drawn.tile}
          <g clip-path="url(#iris-lit)" opacity={frame.tile}>
            <path d={drawn.tile} fill="url(#iris-tile)" stroke="none" fill-rule="evenodd" />
            <g class="stroke-accent-fg">
              <circle cx={frame.iris.cx} cy={frame.iris.cy} r={drawn.R} />
              {#if drawn.blades}<path d={drawn.blades} />{/if}
            </g>
          </g>
        {/if}
        {#if frame.shock > 0}
          <circle
            cx={frame.iris.cx}
            cy={frame.iris.cy}
            r={frame.shockR * drawn.k}
            class="stroke-accent"
            stroke-width={0.5 * drawn.k}
            opacity={frame.shock}
          />
        {/if}
      </g>

      {#if frame.pen > 0}
        <circle
          cx={drawn.penX}
          cy={drawn.penY}
          r={6 * drawn.k * frame.penSize}
          fill="url(#iris-glow)"
          opacity={frame.pen}
        />
        <circle
          cx={drawn.penX}
          cy={drawn.penY}
          r={1.15 * drawn.k * frame.penSize}
          class="fill-accent"
          opacity={frame.pen}
        />
      {/if}
    </svg>

    {#if frame.word > 0}
      <p
        class="absolute inset-x-0 text-center text-[22px] font-semibold tracking-[-0.02em] text-fg-on-stage"
        style:top="{drawn.wordTop}px"
        style:opacity={frame.word}
        style:transform="translateY({frame.wordY}px)"
      >
        Shotmaxxing
      </p>
    {/if}
  </div>
{/if}

<style>
  .iris.skipping {
    opacity: 0 !important;
    transition: opacity 200ms var(--ease-out);
  }
  .stop-accent {
    stop-color: var(--color-accent);
  }
  .stop-tile-top {
    stop-color: var(--icon-tile-top);
  }
  .stop-tile-bottom {
    stop-color: var(--icon-tile-bottom);
  }
</style>
