<script lang="ts">
  /**
   * Ten seconds of what Shotmaxxing does, acted out once on the welcome screen. A pretend sign-up page records
   * itself: the cursor glides to the email field and clicks, the view zooms in on the typing with a chip that says
   * why, zooms back out, the cursor clicks Create account, and the take is saved. One clock drives it while it plays;
   * then it rests on its last frame (Watch again plays it once more). With reduced motion it shows that last frame.
   */
  import { onDestroy } from 'svelte';
  import { Icon, env } from '@shotmaxxing/ui';
  import { Check, RefreshCw, ZoomIn } from '@shotmaxxing/ui/icons';
  import Scene from '../../components/Scene.svelte';
  import { wallpaper } from '../../lib/wallpapers';
  import { ease, lerp, seg } from '../../lib/aperture';

  let { play = true }: { play?: boolean } = $props();

  /** Seconds. */
  const END = 9.2;
  const still = $derived(env.reducedMotion || env.lowPower);

  let T = $state(0);
  let width = $state(560);
  let raf = 0;
  let started = false;

  function run() {
    cancelAnimationFrame(raf);
    const start = performance.now();
    const tick = (now: number) => {
      T = Math.min(END, (now - start) / 1000);
      if (T < END) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }

  // Starts once the welcome is showing, a beat after the logo has opened onto it.
  $effect(() => {
    if (!play || started) return;
    started = true;
    if (still) {
      T = END;
      return;
    }
    const wait = setTimeout(run, 450);
    return () => clearTimeout(wait);
  });
  onDestroy(() => cancelAnimationFrame(raf));

  // The pretend page lives in a 1440 × 900 capture, inside a wallpaper canvas like the editor's.
  const height = $derived(width * 0.625);
  const winW = $derived(width * 0.86);
  const winH = $derived((winW * 900) / 1440);
  const winX = $derived(width * 0.07);
  const winY = $derived((height - winH) / 2);
  const k = $derived(winW / 1440);
  const at = (x: number, y: number) => [winX + k * x, winY + k * y] as const;

  const EMAIL = [985, 442] as const;
  const CREATE = [1080, 602] as const;
  const START = [560, 790] as const;

  /** Where the cursor is, in capture pixels: it glides on a gentle arc between the places it clicks. */
  const cursor = $derived.by(() => {
    const arc = (a: readonly [number, number], b: readonly [number, number], p: number, lift: number) => {
      const cx = lerp(a[0], b[0], 0.35);
      const cy = Math.min(a[1], b[1]) - lift;
      const u = 1 - p;
      return [u * u * a[0] + 2 * u * p * cx + p * p * b[0], u * u * a[1] + 2 * u * p * cy + p * p * b[1]];
    };
    if (T < 5.6) return arc(START, EMAIL, seg(T, 0.5, 1.9, ease.inOutCubic), 60);
    return arc(EMAIL, CREATE, seg(T, 5.6, 6.55, ease.inOutCubic), 30);
  });
  const cursorAt = $derived(at(cursor[0], cursor[1]));
  const press = $derived(
    1 - 0.16 * (seg(T, 1.86, 1.95) - seg(T, 1.95, 2.1) + seg(T, 6.52, 6.61) - seg(T, 6.61, 6.76)),
  );
  const ripples = $derived(
    [
      { from: 1.95, point: EMAIL },
      { from: 6.61, point: CREATE },
    ].map((r) => ({
      ...r,
      k: seg(T, r.from, r.from + 0.55, ease.outCubic),
      on: T >= r.from && T < r.from + 0.55,
    })),
  );

  /** The pretend page's own clock: empty, the email, the password, a beat after the click, then signed up. */
  const pageT = $derived(
    T < 2.1
      ? 0
      : T < 4.6
        ? lerp(9.4, 15.39, (T - 2.1) / 2.5)
        : T < 5.6
          ? lerp(15.4, 17.6, T - 4.6)
          : T < 7
            ? 18.7
            : 20.3,
  );

  /** The camera: how far in, and on what (capture pixels). */
  const camera = $derived.by(() => {
    const centre = [720, 450];
    const field = [1060, 440];
    const card = [1060, 450];
    const zoomTo = (from: number, to: number, a: number[], za: number, b: number[], zb: number) => {
      const p = seg(T, from, to, ease.inOutCubic);
      return { z: lerp(za, zb, p), f: [lerp(a[0], b[0], p), lerp(a[1], b[1], p)] };
    };
    if (T < 2.8) return zoomTo(2.0, 2.8, centre, 1, field, 2);
    if (T < 5.7) return zoomTo(4.9, 5.7, field, 2, centre, 1);
    if (T < 7.5) return zoomTo(6.8, 7.5, centre, 1, card, 1.45);
    return zoomTo(8.0, 8.8, card, 1.45, centre, 1);
  });
  const world = $derived.by(() => {
    const { z, f } = camera;
    const [fx, fy] = at(f[0], f[1]);
    const tx = Math.min(0, Math.max(width - z * width, width / 2 - z * fx));
    const ty = Math.min(0, Math.max(height - z * height, height / 2 - z * fy));
    return `translate(${tx}px, ${ty}px) scale(${z})`;
  });

  const chips = $derived(
    [
      { from: 2.35, to: 4.95, zoom: '2×', why: 'Typing in Work email' },
      { from: 6.95, to: 8.3, zoom: '1.45×', why: 'Click on Create account' },
    ].map((c) => ({
      ...c,
      k: seg(T, c.from, c.from + 0.25, ease.outCubic) * (1 - seg(T, c.to - 0.25, c.to)),
    })),
  );
  const saved = $derived(T >= 8.7);
  const clock = $derived(`0:0${Math.min(8, Math.floor(T))}`);

  function replay() {
    T = 0;
    run();
  }

  // Development: freeze any moment of the show (window.__show.seek(seconds)).
  if (import.meta.env.DEV) {
    Object.assign(window, {
      __show: {
        seek(seconds: number) {
          cancelAnimationFrame(raf);
          started = true;
          T = seconds;
        },
      },
    });
  }
</script>

<div
  class="show relative w-full overflow-hidden rounded-[16px] bg-stage shadow-[var(--elev-3)]"
  style:height="{height}px"
  bind:clientWidth={width}
  role="img"
  aria-label="A short demo: Shotmaxxing zooms in where you type and click, then saves the take."
>
  <div
    class="absolute left-0 top-0 origin-top-left"
    style:width="{width}px"
    style:height="{height}px"
    style:transform={world}
  >
    <div class="absolute inset-0" style:background={wallpaper('aurora-lime').css}></div>
    <div
      class="absolute overflow-hidden rounded-[6px] shadow-[var(--elev-3)]"
      style:left="{winX}px"
      style:top="{winY}px"
      style:width="{winW}px"
      style:height="{winH}px"
    >
      <Scene variant={0} t={pageT} width={winW} />
    </div>
    {#each ripples as r (r.from)}
      {#if r.on}
        {@const [x, y] = at(r.point[0], r.point[1])}
        <span
          class="absolute rounded-full border-2 border-accent"
          style:left="{x}px"
          style:top="{y}px"
          style:width="{70 * k}px"
          style:height="{70 * k}px"
          style:opacity={0.85 * (1 - r.k)}
          style:transform="translate(-50%, -50%) scale({0.3 + 1.2 * r.k})"
        ></span>
      {/if}
    {/each}
    <svg
      class="absolute"
      viewBox="0 0 16 24"
      style:left="{cursorAt[0]}px"
      style:top="{cursorAt[1]}px"
      style:width="{30 * k * 1.4}px"
      style:opacity={seg(T, 0.2, 0.5)}
      style:transform="scale({press})"
      style:transform-origin="0 0"
      aria-hidden="true"
    >
      <path
        d="M1 1v19.5l5-4.6 3.6 7.6 3.1-1.4-3.5-7.4h6.7z"
        class="fill-[var(--cursor-dark)] stroke-[var(--cursor-light)]"
        stroke-width="1.6"
        stroke-linejoin="round"
      />
    </svg>
  </div>

  <!-- What a recording shows on top: REC and the time, the zoom's reason, and the viewfinder's corners. -->
  <div class="pointer-events-none absolute inset-0">
    <svg class="absolute inset-0 size-full" aria-hidden="true">
      {#each [[10, 10, 1, 1], [width - 10, 10, -1, 1], [10, height - 10, 1, -1], [width - 10, height - 10, -1, -1]] as [x, y, sx, sy] (`${sx}${sy}`)}
        <path
          d="M{x} {y + sy * 16}V{y + sy * 5}Q{x} {y} {x + sx * 5} {y}H{x + sx * 16}"
          fill="none"
          class="stroke-accent"
          stroke-width="2"
          stroke-linecap="round"
          opacity="0.9"
        />
      {/each}
    </svg>
    <span
      class="absolute left-8 top-6 flex items-center gap-1.5 rounded-full bg-stage/90 shadow-[var(--elev-2)] px-2 py-0.5 text-caption font-semibold text-fg-on-stage tabular"
    >
      {#if saved}
        <Icon icon={Check} size={12} class="text-accent" aria-hidden="true" /> Saved · 0:08
      {:else}
        <span class="size-1.5 rounded-full bg-record"></span> REC {clock}
      {/if}
    </span>
    {#each chips as chip (chip.why)}
      {#if chip.k > 0}
        <span
          class="absolute bottom-6 left-1/2 flex items-center gap-1.5 whitespace-nowrap rounded-full bg-stage/90 shadow-[var(--elev-2)] py-1 pl-2 pr-2.5 text-caption font-medium text-fg-on-stage"
          style:opacity={chip.k}
          style:transform="translate(-50%, {(1 - chip.k) * 6}px)"
        >
          <Icon icon={ZoomIn} size={12} class="text-accent" aria-hidden="true" />
          <span class="font-semibold text-accent">{chip.zoom}</span>
          {chip.why}
        </span>
      {/if}
    {/each}
  </div>

  {#if T >= END && !still}
    <button
      type="button"
      class="press absolute bottom-6 right-8 flex items-center gap-1.5 rounded-full bg-stage/90 shadow-[var(--elev-2)] px-2.5 py-1 text-caption font-medium text-fg-on-stage hover:bg-stage"
      onclick={replay}
    >
      <Icon icon={RefreshCw} size={12} aria-hidden="true" /> Watch again
    </button>
  {/if}
</div>
