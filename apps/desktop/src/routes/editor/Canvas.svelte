<script lang="ts">
  import { UserRound } from '@shotmaxxing/ui/icons';
  import { Icon } from '@shotmaxxing/ui';
  import Scene from '../../components/Scene.svelte';
  import { CAPTION_SIZE, pageAt, pages, rowUnits } from '../../lib/captions';
  import { effectDefaults } from '../../lib/project';
  import type { EffectBlock } from '../../lib/sample';
  import { wallpaper } from '../../lib/wallpapers';
  import { ASPECTS, cameraAt, clamp, cursorAt, placeWindow } from './camera';
  import type { EditorState } from './store.svelte';

  /**
   * The live preview. The recording plays in a <video> (decoded in hardware), and the camera, cursor, clicks and
   * overlays are layers moved with transforms only, so the preview costs about what playing the video costs. The
   * built-in sample draws a stand-in scene instead of a video. Exports are drawn natively from the same settings.
   * A screenshot shows its picture instead, with no cursor and every effect. Titles, arrows and blurs can be dragged
   * into place, or nudged with the arrow keys once focused.
   */
  let { ed }: { ed: EditorState } = $props();

  let video = $state<HTMLVideoElement | null>(null);
  $effect(() => {
    ed.attachVideo(video);
    return () => ed.attachVideo(null);
  });
  let cameraVideo = $state<HTMLVideoElement | null>(null);
  $effect(() => {
    ed.attachCamera(cameraVideo);
    return () => ed.attachCamera(null);
  });

  // Smoothing, snapping and looping change the path the cursor takes; redraw it once a slider settles.
  $effect(() => {
    const { smoothing, snap, loop } = ed.project.cursor;
    void [smoothing, snap, loop, ed.path];
    const id = setTimeout(() => void ed.refreshCursorPath(), 150);
    return () => clearTimeout(id);
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
  $effect(() => {
    if (k > 0) ed.framePoints = { width: frame.width / k, height: frame.height / k };
  });
  const cursor = $derived(cursorAt(ed.time, ed.cursorPath));

  // Motion blur, as the export draws it: copies from where the cursor was 15 ms ago to where it is, added together
  // (plus-lighter is the browser's version of the export's additive blend) with more weight at the front.
  const SMEAR_S = 0.3 * 0.05;
  const smear = $derived.by(() => {
    if (!p.cursor.motionBlur) return null;
    const back = cursorAt(Math.max(0, ed.time - SMEAR_S), ed.cursorPath);
    const dx = (back.x - cursor.x) * k;
    const dy = (back.y - cursor.y) * k;
    const distance = Math.hypot(dx, dy);
    if (distance < k) return null;
    const count = Math.min(10, Math.max(2, Math.ceil(distance / (cursorPx * 0.12))));
    const weights = Array.from({ length: count }, (_, i) => 1 + (2 * i) / (count - 1));
    const total = weights.reduce((a, b) => a + b, 0);
    return weights.map((w, i) => {
      const behind = 1 - i / (count - 1);
      return { x: dx * behind, y: dy * behind, opacity: w / total };
    });
  });
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
  // The arrow is 20 units wide; at size 1 a unit is a capture point, about the real cursor's size.
  const cursorPx = $derived(20 * p.cursor.size * k);
  const unit = $derived(cursorPx / 20);
  // Rings and the dot's edge scale with the cursor, as in the export.
  const ringPx = $derived(cursorPx / 11);
  const spots = $derived(effectDefaults(capture));
  // The privacy blur is part of the picture, not interface translucency, and only covers its own small box.
  const privacyBlur = (px: number) =>
    `backdrop-filter: blur(${px}px); -webkit-backdrop-filter: blur(${px}px)`; // design-guard-allow backdrop

  // A still shows every effect; a video shows the ones placed at this moment.
  const showing = (e: EffectBlock) => ed.still || (ed.time >= e.start && ed.time <= e.end);
  const blur = $derived(p.effects.filter((e) => e.kind === 'blur' && showing(e)));
  const titles = $derived(p.effects.filter((e) => e.kind === 'text' && showing(e)));
  const arrows = $derived(p.effects.filter((e) => e.kind === 'arrow' && showing(e)));

  let box = $state<HTMLDivElement | null>(null);
  type Part = 'move' | 'from' | 'to' | 'size';
  /** An effect being dragged: which part moves, where the pointer started, and the effect as it was then. */
  let drag: { id: string; part: Part; x: number; y: number; start: Required<EffectBlock> } | null = null;

  /** A pointer position in capture points, through the zoom camera. */
  function inCapture(ev: PointerEvent): { x: number; y: number } {
    const r = box?.getBoundingClientRect();
    if (!r) return { x: 0, y: 0 };
    const s = cam.scale;
    const fx = (ev.clientX - r.left - (frame.width / 2 - viewX * s)) / s;
    const fy = (ev.clientY - r.top - (frame.height / 2 - viewY * s)) / s;
    return { x: (fx - win.x) / k, y: (fy - win.y) / k };
  }

  /** A pointer position as a fraction of the frame: titles stay put while the camera zooms. */
  function inFrame(ev: PointerEvent): { x: number; y: number } {
    const r = box?.getBoundingClientRect();
    if (!r) return { x: 0, y: 0 };
    return { x: (ev.clientX - r.left) / frame.width, y: (ev.clientY - r.top) / frame.height };
  }

  function grab(ev: PointerEvent, effect: EffectBlock, part: Part) {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    ed.selection = { kind: 'effect', id: effect.id };
    (ev.currentTarget as Element).setPointerCapture(ev.pointerId);
    const at = effect.kind === 'text' ? inFrame(ev) : inCapture(ev);
    drag = {
      id: effect.id,
      part,
      x: at.x,
      y: at.y,
      start: {
        ...effect,
        pos: effect.pos ?? { x: 0.05, y: 0.06 },
        rect: effect.rect ?? spots.blur,
        strength: effect.strength ?? 1,
        from: effect.from ?? spots.from,
        to: effect.to ?? spots.to,
      },
    };
  }

  function pull(ev: PointerEvent) {
    const d = drag;
    const effect = d && p.effects.find((e) => e.id === d.id);
    if (!d || !effect) return;
    const s = d.start;
    const { width: cw, height: ch } = capture;
    if (effect.kind === 'text') {
      const at = inFrame(ev);
      const pos = { x: clamp(s.pos.x + at.x - d.x, 0, 0.95), y: clamp(s.pos.y + at.y - d.y, 0, 0.95) };
      ed.change('Move title', () => (effect.pos = pos));
      return;
    }
    const at = inCapture(ev);
    const [dx, dy] = [at.x - d.x, at.y - d.y];
    const moved = (q: { x: number; y: number }) => ({ x: clamp(q.x + dx, 0, cw), y: clamp(q.y + dy, 0, ch) });
    if (effect.kind === 'arrow') {
      const from = d.part === 'to' ? s.from : moved(s.from);
      const to = d.part === 'from' ? s.to : moved(s.to);
      ed.change('Move arrow', () => {
        effect.from = from;
        effect.to = to;
      });
    } else {
      const r = s.rect;
      const rect =
        d.part === 'size'
          ? { ...r, width: clamp(r.width + dx, 8, cw - r.x), height: clamp(r.height + dy, 8, ch - r.y) }
          : { ...r, x: clamp(r.x + dx, 0, cw - r.width), y: clamp(r.y + dy, 0, ch - r.height) };
      ed.change(d.part === 'size' ? 'Resize blur' : 'Move blur', () => (effect.rect = rect));
    }
  }

  function drop() {
    drag = null;
  }

  /** Arrow keys move a focused effect: a point at a time, ten with Shift. */
  function nudgeKey(ev: KeyboardEvent, effect: EffectBlock) {
    const step = ev.shiftKey ? 10 : 1;
    const moves: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const move = moves[ev.key];
    if (!move) return;
    ev.preventDefault();
    ev.stopPropagation();
    ed.selection = { kind: 'effect', id: effect.id };
    ed.nudge(...move);
  }

  const select = (effect: EffectBlock) => (ed.selection = { kind: 'effect', id: effect.id });
  $effect(() => {
    const el = box;
    if (!el) return;
    const away = (ev: PointerEvent) => {
      const mark = (ev.target as Element).closest('[role="button"], button');
      if (!mark && ed.selection?.kind === 'effect') ed.selection = null;
    };
    el.addEventListener('pointerdown', away);
    return () => el.removeEventListener('pointerdown', away);
  });
  const selected = (effect: EffectBlock) => ed.selection?.kind === 'effect' && ed.selection.id === effect.id;

  // The caption showing, broken into rows as the export breaks it: a long one shows a page of two rows at a time.
  // A caption's pages are worked out once, not every frame.
  let paged = { key: '', list: [] as string[][] };
  const caption = $derived.by(() => {
    const now = p.captions.show ? ed.captionNow : null;
    if (!now || frame.height <= 0) return null;
    const max = rowUnits(frame.width / frame.height, p.captions.style);
    const key = `${max}|${now.line.text}`;
    if (paged.key !== key) paged = { key, list: pages(now.line.text, max) };
    return paged.list[pageAt(paged.list, now.fraction)] ?? null;
  });
  let captionH = $state(0);
  // A keystroke sits above the caption, half a line clear, when its usual place would overlap it.
  const keyBottom = $derived(
    caption && captionH > 0
      ? Math.max(frame.height * 0.06, frame.height * 0.05 + captionH + frame.height * 0.035 * 0.5)
      : frame.height * 0.06,
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
  // The bubble eases down to 70% as a zoom goes in, so it never covers the action. The exporter uses the same
  // sizes, ring and shadow, in fractions of the frame height.
  const shrink = $derived(p.webcam.shrink ? 1 - 0.3 * clamp((cam.scale - 1) / 0.5, 0, 1) : 1);
  const bubble = $derived(frame.height * p.webcam.size * shrink);
  const bubbleRadius = $derived(
    p.webcam.shape === 'circle' ? '50%' : p.webcam.shape === 'rounded' ? '22%' : '6%',
  );
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
      bind:this={box}
      class="relative overflow-hidden rounded-[4px] shadow-[var(--elev-3)]"
      style:width="{frame.width}px"
      style:height="{frame.height}px"
      role="group"
      aria-label={ed.still
        ? `Preview of ${p.title}`
        : `Preview of ${p.title} at ${ed.time.toFixed(1)} seconds`}
    >
      <div
        class="absolute left-0 top-0 origin-top-left will-change-transform"
        style:width="{frame.width}px"
        style:height="{frame.height}px"
        style:transform
      >
        <div
          class="absolute inset-0"
          style:background={p.canvas.wallpaperCss ?? wallpaper(p.canvas.wallpaper).css}
        ></div>
        <div
          class="absolute overflow-hidden"
          style:left="{win.x}px"
          style:top="{win.y}px"
          style:width="{win.width}px"
          style:height="{win.height}px"
          style:border-radius="{radius}px"
          style:box-shadow={shadow}
        >
          {#if ed.media && ed.still}
            <img src={ed.media} alt="" class="absolute inset-0 size-full object-cover" draggable="false" />
          {:else if ed.media}
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
            {@const r = b.rect ?? spots.blur}
            <div
              role="button"
              tabindex="0"
              aria-label="Blur. Drag it, or use the arrow keys, to move it."
              class={[
                'absolute cursor-move touch-none outline-none focus-visible:mark-selected',
                selected(b) && 'mark-selected',
              ]}
              style={privacyBlur(frame.height * 0.016 * (b.strength ?? 1))}
              style:left="{r.x * k}px"
              style:top="{r.y * k}px"
              style:width="{r.width * k}px"
              style:height="{r.height * k}px"
              style:border-radius="{frame.height * 0.012}px"
              onpointerdown={(ev) => grab(ev, b, 'move')}
              onpointermove={pull}
              onpointerup={drop}
              onpointercancel={drop}
              onfocus={() => select(b)}
              onkeydown={(ev) => nudgeKey(ev, b)}
            ></div>
            {#if selected(b)}
              <button
                type="button"
                tabindex="-1"
                aria-label="Resize the blur"
                class="absolute size-3 cursor-nwse-resize touch-none rounded-[3px] border-[1.5px] border-[var(--cursor-dark)] bg-[var(--cursor-light)]"
                style:left="{(r.x + r.width) * k - 14}px"
                style:top="{(r.y + r.height) * k - 14}px"
                onpointerdown={(ev) => grab(ev, b, 'size')}
                onpointermove={pull}
                onpointerup={drop}
                onpointercancel={drop}
              ></button>
            {/if}
          {/each}
          {#each arrows as a (a.id)}
            {@const from = a.from ?? spots.from}
            {@const to = a.to ?? spots.to}
            <svg
              class="pointer-events-none absolute inset-0 overflow-visible"
              width={win.width}
              height={win.height}
              aria-hidden="true"
            >
              <path
                d={arrowPath(from, to)}
                stroke="var(--accent)"
                stroke-width={Math.max(3, 7 * k)}
                stroke-linecap="round"
                stroke-linejoin="round"
                fill="none"
              />
            </svg>
            <!-- What you grab: the shaft, and its two ends once it's selected. -->
            <svg
              class="pointer-events-none absolute inset-0 overflow-visible"
              width={win.width}
              height={win.height}
            >
              <path
                role="button"
                tabindex="0"
                aria-label="Arrow. Drag it, or use the arrow keys, to move it."
                d="M{from.x * k} {from.y * k} L{to.x * k} {to.y * k}"
                stroke="transparent"
                stroke-width={Math.max(16, 22 * k)}
                stroke-linecap="round"
                fill="none"
                pointer-events="stroke"
                class="cursor-move touch-none outline-none"
                onpointerdown={(ev) => grab(ev, a, 'move')}
                onpointermove={pull}
                onpointerup={drop}
                onpointercancel={drop}
                onfocus={() => select(a)}
                onkeydown={(ev) => nudgeKey(ev, a)}
              />
              {#if selected(a)}
                {#each [{ part: 'from', at: from, label: 'start' }, { part: 'to', at: to, label: 'tip' }] as end (end.part)}
                  <circle
                    role="button"
                    tabindex="-1"
                    aria-label="Move the arrow's {end.label}"
                    cx={end.at.x * k}
                    cy={end.at.y * k}
                    r="7"
                    fill="var(--cursor-light)"
                    stroke="var(--cursor-dark)"
                    stroke-width="2"
                    pointer-events="all"
                    class="cursor-grab touch-none"
                    onpointerdown={(ev) => grab(ev, a, end.part as Part)}
                    onpointermove={pull}
                    onpointerup={drop}
                    onpointercancel={drop}
                  />
                {/each}
              {/if}
            </svg>
          {/each}
        </div>

        {#if !idle && !ed.still}
          <!-- The cursor rides inside the camera, so zooms make it bigger too. -->
          <div
            class="pointer-events-none absolute left-0 top-0"
            style:transform="translate({win.x + cursor.x * k}px, {win.y + cursor.y * k}px) scale({pressed
              ? 0.85
              : 1})"
          >
            {#if click && p.cursor.click === 'ripple'}
              <span
                class="absolute rounded-full border-solid border-[var(--cursor-light)]"
                style:left="{-cursorPx}px"
                style:top="{-cursorPx}px"
                style:width="{cursorPx * 2}px"
                style:height="{cursorPx * 2}px"
                style:border-width="{ringPx}px"
                style:opacity={1 - clickAge}
                style:transform="scale({0.35 + clickAge * 0.9})"
                style:filter="drop-shadow(0 0 {ringPx * 0.6}px var(--cursor-shadow))"
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
            {#if smear}
              <div class="absolute left-0 top-0 isolate">
                {#each smear as copy, i (i)}
                  <div
                    class="absolute left-0 top-0 mix-blend-plus-lighter"
                    style:transform="translate({copy.x}px, {copy.y}px)"
                    style:opacity={copy.opacity}
                  >
                    {@render pointer()}
                  </div>
                {/each}
              </div>
            {:else}
              {@render pointer()}
            {/if}
          </div>
        {/if}
      </div>

      {#each titles as title (title.id)}
        <span
          role="button"
          tabindex="0"
          aria-label="Title “{title.label}”. Drag it, or use the arrow keys, to move it."
          style:left="{(title.pos?.x ?? 0.05) * 100}%"
          style:top="{(title.pos?.y ?? 0.06) * 100}%"
          class={[
            'absolute cursor-move touch-none select-none rounded-full bg-accent px-[1.4em] py-[0.5em] leading-[1.25] font-semibold whitespace-nowrap text-accent-fg shadow-[var(--stage-shadow)] outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-fg',
            selected(title) && 'ring-2 ring-inset ring-accent-fg/70',
          ]}
          style:font-size="{frame.height * 0.035}px"
          onpointerdown={(ev) => grab(ev, title, 'move')}
          onpointermove={pull}
          onpointerup={drop}
          onpointercancel={drop}
          onfocus={() => select(title)}
          onkeydown={(ev) => nudgeKey(ev, title)}>{title.label}</span
        >
      {/each}

      {#if caption}
        <span
          bind:offsetHeight={captionH}
          class={[
            'absolute bottom-[5%] left-1/2 flex -translate-x-1/2 flex-col items-center text-center leading-[1.25] whitespace-nowrap text-[color:var(--caption-ink)]',
            p.captions.style === 'bar'
              ? 'rounded-[0.4em] bg-[var(--caption-scrim)] px-[0.7em] py-[0.25em] font-medium'
              : 'font-extrabold [text-shadow:var(--caption-outline)]',
          ]}
          style:font-size="{frame.height * CAPTION_SIZE[p.captions.style]}px"
        >
          {#each caption as row, i (i)}<span>{row}</span>{/each}
        </span>
      {/if}

      {#if key}
        <span
          class="absolute left-1/2 -translate-x-1/2 rounded-[0.5em] bg-[var(--stage-scrim)] px-[0.8em] py-[0.35em] leading-[1.25] font-semibold whitespace-nowrap text-fg-on-stage"
          style:bottom="{keyBottom}px"
          style:font-size="{frame.height * 0.035}px">{key.label}</span
        >
      {/if}

      {#if p.webcam.on}
        <span
          class="absolute grid place-items-center overflow-hidden border-solid border-[var(--cursor-light)] bg-accent-tint text-accent-text"
          style:left="{corner.left}px"
          style:top="{corner.top}px"
          style:width="{bubble}px"
          style:height="{bubble}px"
          style:border-radius={bubbleRadius}
          style:border-width="{frame.height * 0.004}px"
          style:box-shadow={p.webcam.shadow
            ? `0 ${frame.height * 0.008}px ${frame.height * 0.024}px var(--bubble-shadow)`
            : 'none'}
        >
          {#if ed.camera}
            <video
              bind:this={cameraVideo}
              src={ed.camera.url}
              class="size-full object-cover"
              style:transform={p.webcam.mirror ? 'scaleX(-1)' : undefined}
              muted
              playsinline
              preload="auto"
              disablepictureinpicture
            ></video>
          {:else}
            <Icon icon={UserRound} size={bubble * 0.5} aria-hidden="true" />
          {/if}
        </span>
      {/if}
    </div>
  {/if}
</div>

{#snippet pointer()}
  {#if p.cursor.style === 'dot'}
    <span
      class="absolute rounded-full border-solid border-[var(--cursor-light)] bg-[var(--cursor-dark)]"
      style:left="{-cursorPx * 0.35}px"
      style:top="{-cursorPx * 0.35}px"
      style:width="{cursorPx * 0.7}px"
      style:height="{cursorPx * 0.7}px"
      style:border-width="{ringPx}px"
    ></span>
  {:else}
    <!-- The arrow's tip sits at (2, 1.5) in its box, so the box is moved back by that much. -->
    <svg
      width={cursorPx}
      height={cursorPx * 1.4}
      viewBox="0 0 20 28"
      class="absolute overflow-visible"
      style:left="{-2 * unit}px"
      style:top="{-1.5 * unit}px"
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
{/snippet}
