<script lang="ts">
  import { UserRound } from '@lucide/svelte';
  import Scene from '../../components/Scene.svelte';
  import { wallpaper } from '../../lib/wallpapers';
  import { ASPECTS, cameraAt, clamp, cursorAt, placeWindow } from './camera';
  import type { EditorState } from './store.svelte';

  /**
   * The live preview. The recording plays in a <video> (decoded in hardware), and the camera, cursor, clicks and
   * overlays are layers moved with transforms only, so the preview costs about what playing the video costs. The
   * built-in sample draws a stand-in scene instead of a video. Exports are drawn natively from the same settings.
   */
  let { ed }: { ed: EditorState } = $props();

  let video = $state<HTMLVideoElement | null>(null);
  $effect(() => {
    ed.attachVideo(video);
    return () => ed.attachVideo(null);
  });

  let stageW = $state(0);
  let stageH = $state(0);

  const p = $derived(ed.project);
  const capture = $derived(p.capture);

  const frame = $derived.by(() => {
    const aspect = ASPECTS[p.canvas.aspect] ?? capture.width / capture.height;
    const maxW = Math.max(0, stageW - 48);
    const maxH = Math.max(0, stageH - 48);
    let width = maxW;
    let height = width / aspect;
    if (height > maxH) {
      height = maxH;
      width = height * aspect;
    }
    return { width: Math.round(width), height: Math.round(height) };
  });

  const win = $derived(placeWindow(frame, capture, p.canvas.padding));
  const k = $derived(win.width / capture.width);
  const cursor = $derived(cursorAt(ed.time, ed.cursorPath));
  const cam = $derived(
    cameraAt(ed.time, p.zooms, capture, { motion: p.zoom.motion, follow: p.zoom.follow, cursor }),
  );

  // The camera zooms the whole composition around the focus point, clamped so the frame stays filled.
  const focusX = $derived(win.x + cam.x * k);
  const focusY = $derived(win.y + cam.y * k);
  const viewX = $derived(
    clamp(focusX, frame.width / (2 * cam.scale), frame.width - frame.width / (2 * cam.scale)),
  );
  const viewY = $derived(
    clamp(focusY, frame.height / (2 * cam.scale), frame.height - frame.height / (2 * cam.scale)),
  );
  const transform = $derived(
    `translate(${frame.width / 2 - viewX * cam.scale}px, ${frame.height / 2 - viewY * cam.scale}px) scale(${cam.scale})`,
  );

  const radius = $derived((p.canvas.radius * frame.height) / 1080);
  const shadow = $derived(
    `0 ${frame.height * 0.02}px ${frame.height * 0.06}px color-mix(in oklch, var(--shadow-ink) ${Math.round(p.canvas.shadow * 55)}%, transparent)`,
  );

  // Clicks: a ring that grows and fades, computed from time so scrubbing shows it too.
  const click = $derived(ed.clicks.find((c) => ed.time >= c.t && ed.time < c.t + 0.5));
  const clickAge = $derived(click ? (ed.time - click.t) / 0.5 : 0);
  const pressed = $derived(p.cursor.click === 'press' && !!click && clickAge < 0.3);

  // The cursor fades out after it has rested for a while, and comes back on the next move or click.
  const lastActive = $derived.by(() => {
    const path = ed.cursorPath;
    let lo = 0;
    let hi = path.length - 1;
    let found = -Infinity;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (path[mid].t <= ed.time) {
        found = path[mid].t;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    const lastClick = ed.clicks.findLast((c) => c.t <= ed.time)?.t ?? -Infinity;
    return Math.max(found, lastClick);
  });
  const idle = $derived(p.cursor.hideIdle && ed.time - lastActive > 2.5 && ed.time > 2.5);
  const cursorPx = $derived(24 * p.cursor.size * k * (capture.width / 1440) * (frame.height / 520));

  const blur = $derived(p.effects.filter((e) => e.kind === 'blur' && ed.time >= e.start && ed.time <= e.end));
  const titles = $derived(
    p.effects.filter((e) => e.kind === 'text' && ed.time >= e.start && ed.time <= e.end),
  );
  const arrows = $derived(
    p.effects.filter((e) => e.kind === 'arrow' && ed.time >= e.start && ed.time <= e.end),
  );

  // The sample's captions; real projects get theirs from local transcription.
  const CAPTIONS = [
    { start: 0.4, end: 2.8, text: 'Here’s how sign-up works in Northwind.' },
    { start: 2.9, end: 6.4, text: 'Start with Get started, top right.' },
    { start: 9.0, end: 14.6, text: 'Then enter your work email.' },
    { start: 15.3, end: 18.6, text: 'Pick a password you’ll remember.' },
    { start: 18.8, end: 23.6, text: 'Create the account, and you’re in.' },
    { start: 30.0, end: 35.0, text: 'Pricing is one click away.' },
  ];
  const caption = $derived(
    !ed.path && p.captions.state === 'ready' && p.captions.show
      ? CAPTIONS.find((c) => ed.time >= c.start && ed.time <= c.end)
      : undefined,
  );

  const key = $derived(
    p.keystrokes ? ed.keys.findLast((x) => ed.time >= x.t && ed.time < x.t + 0.9) : undefined,
  );

  /** Arrow geometry in window pixels: a shaft and a two-stroke head at `to`. */
  function arrowPath(from: { x: number; y: number }, to: { x: number; y: number }): string {
    const [x1, y1, x2, y2] = [from.x * k, from.y * k, to.x * k, to.y * k];
    const angle = Math.atan2(y2 - y1, x2 - x1);
    const head = Math.max(10, 26 * k);
    const a1 = angle + Math.PI - 0.5;
    const a2 = angle + Math.PI + 0.5;
    return `M${x1} ${y1} L${x2} ${y2} M${x2 + head * Math.cos(a1)} ${y2 + head * Math.sin(a1)} L${x2} ${y2} L${x2 + head * Math.cos(a2)} ${y2 + head * Math.sin(a2)}`;
  }

  const SPOTLIGHT =
    'radial-gradient(circle, color-mix(in oklch, var(--cursor-light) 28%, transparent), transparent 70%)';
  const zoomed = $derived(cam.scale > 1.05);
  const bubble = $derived(frame.height * p.webcam.size * (p.webcam.shrink && zoomed ? 0.7 : 1));
  const corner = $derived({
    left: p.webcam.corner.endsWith('l') ? frame.height * 0.04 : frame.width - bubble - frame.height * 0.04,
    top: p.webcam.corner.startsWith('t') ? frame.height * 0.04 : frame.height - bubble - frame.height * 0.04,
  });
</script>

<div
  class="relative grid min-h-0 flex-1 place-items-center bg-stage"
  bind:clientWidth={stageW}
  bind:clientHeight={stageH}
  data-tour="canvas"
>
  {#if frame.width > 0}
    <div
      class="relative overflow-hidden rounded-[4px] shadow-[var(--elev-3)]"
      style:width="{frame.width}px"
      style:height="{frame.height}px"
      role="img"
      aria-label="Preview of {p.title} at {ed.time.toFixed(1)} seconds"
    >
      <div
        class="absolute left-0 top-0 origin-top-left will-change-transform"
        style:width="{frame.width}px"
        style:height="{frame.height}px"
        style:transform
      >
        <div class="absolute inset-0" style:background={wallpaper(p.canvas.wallpaper).css}></div>
        <div
          class="absolute overflow-hidden"
          style:left="{win.x}px"
          style:top="{win.y}px"
          style:width="{win.width}px"
          style:height="{win.height}px"
          style:border-radius="{radius}px"
          style:box-shadow={shadow}
        >
          {#if ed.media}
            <video
              bind:this={video}
              src={ed.media}
              class="absolute inset-0 size-full object-cover"
              muted
              playsinline
              preload="auto"
              disablepictureinpicture
              onended={() => ed.pause()}
            ></video>
          {:else}
            <Scene variant={0} width={win.width} t={ed.time} />
          {/if}
          {#each blur as b (b.id)}
            <div
              class="scn-blur absolute"
              style:left="{(b.rect?.x ?? 840) * k}px"
              style:top="{(b.rect?.y ?? 396) * k}px"
              style:width="{(b.rect?.width ?? 440) * k}px"
              style:height="{(b.rect?.height ?? 48) * k}px"
            ></div>
          {/each}
          {#each arrows.filter((a) => a.from && a.to) as a (a.id)}
            <svg
              class="absolute inset-0 overflow-visible"
              width={win.width}
              height={win.height}
              aria-hidden="true"
            >
              <path
                d={arrowPath(a.from!, a.to!)}
                stroke="var(--accent)"
                stroke-width={Math.max(3, 7 * k)}
                stroke-linecap="round"
                stroke-linejoin="round"
                fill="none"
              />
            </svg>
          {/each}
          {#each arrows.filter((a) => !a.from || !a.to) as a (a.id)}
            <svg
              class="absolute"
              style:left="{1120 * k}px"
              style:top="{150 * k}px"
              width={140 * k}
              height={110 * k}
              viewBox="0 0 140 110"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M130 100 C 90 90, 50 60, 22 18"
                stroke="var(--accent)"
                stroke-width="7"
                stroke-linecap="round"
              />
              <path
                d="M10 40 L22 14 L46 26"
                stroke="var(--accent)"
                stroke-width="7"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          {/each}
        </div>

        {#if !idle}
          <!-- The cursor rides inside the camera, so zooms make it bigger too. -->
          <div
            class="pointer-events-none absolute left-0 top-0"
            style:transform="translate({win.x + cursor.x * k}px, {win.y + cursor.y * k}px) scale({pressed
              ? 0.85
              : 1})"
          >
            {#if click && p.cursor.click === 'ripple'}
              <span
                class="absolute rounded-full border-2 border-[var(--cursor-light)]"
                style:left="{-cursorPx}px"
                style:top="{-cursorPx}px"
                style:width="{cursorPx * 2}px"
                style:height="{cursorPx * 2}px"
                style:opacity={1 - clickAge}
                style:transform="scale({0.35 + clickAge * 0.9})"
              ></span>
            {/if}
            {#if p.cursor.spotlight}
              <span
                class="absolute rounded-full"
                style:left="{-cursorPx * 3}px"
                style:top="{-cursorPx * 3}px"
                style:width="{cursorPx * 6}px"
                style:height="{cursorPx * 6}px"
                style:background={SPOTLIGHT}
              ></span>
            {/if}
            {#if p.cursor.style === 'dot'}
              <span
                class="absolute rounded-full border-2 border-[var(--cursor-light)] bg-[var(--cursor-dark)]"
                style:left="{-cursorPx * 0.35}px"
                style:top="{-cursorPx * 0.35}px"
                style:width="{cursorPx * 0.7}px"
                style:height="{cursorPx * 0.7}px"
              ></span>
            {:else}
              <svg
                width={cursorPx}
                height={cursorPx * 1.4}
                viewBox="0 0 20 28"
                class="absolute left-[-1px] top-[-1px] overflow-visible"
                aria-hidden="true"
              >
                <path
                  d="M2 1.5 L2 22 L7.2 17.4 L10.6 25.6 L14 24.2 L10.7 16.2 L17.6 16.2 Z"
                  fill={p.cursor.style === 'mac' ? 'var(--cursor-dark)' : 'var(--cursor-light)'}
                  stroke={p.cursor.style === 'mac' ? 'var(--cursor-light)' : 'var(--cursor-dark)'}
                  stroke-width="1.6"
                  stroke-linejoin="round"
                />
              </svg>
            {/if}
          </div>
        {/if}
      </div>

      {#each titles as title (title.id)}
        <span
          style:left="{(title.pos?.x ?? 0.05) * 100}%"
          style:top="{(title.pos?.y ?? 0.06) * 100}%"
          class="absolute rounded-full bg-accent px-[1.4em] py-[0.5em] font-semibold text-accent-fg shadow-[var(--elev-2)]"
          style:font-size="{frame.height * 0.035}px">{title.label}</span
        >
      {/each}

      {#if key}
        <span
          class="absolute bottom-[6%] left-1/2 -translate-x-1/2 rounded-[0.5em] bg-scrim px-[0.8em] py-[0.35em] font-semibold text-fg-on-stage"
          style:font-size="{frame.height * 0.035}px">{key.label}</span
        >
      {/if}

      {#if caption}
        <span
          class={[
            'absolute bottom-[5%] left-1/2 max-w-[80%] -translate-x-1/2 text-center text-fg-on-stage',
            p.captions.style === 'bar'
              ? 'rounded-[0.4em] bg-scrim px-[0.7em] py-[0.25em] font-medium'
              : 'font-extrabold [text-shadow:0_2px_8px_var(--scrim)]',
          ]}
          style:font-size="{frame.height * (p.captions.style === 'bar' ? 0.038 : 0.05)}px"
          >{caption.text}</span
        >
      {/if}

      {#if p.webcam.on}
        <span
          class={[
            'absolute grid place-items-center overflow-hidden border-2 border-[var(--cursor-light)] bg-accent-tint text-accent-text shadow-[var(--elev-2)] transition-transform duration-[var(--dur-slow)] ease-[var(--ease-spring)]',
            p.webcam.shape === 'circle'
              ? 'rounded-full'
              : p.webcam.shape === 'rounded'
                ? 'rounded-[22%]'
                : 'rounded-[6%]',
          ]}
          style:left="{corner.left}px"
          style:top="{corner.top}px"
          style:width="{bubble}px"
          style:height="{bubble}px"
        >
          <UserRound size={bubble * 0.5} strokeWidth={1.5} aria-hidden="true" />
        </span>
      {/if}
    </div>
  {/if}
</div>
