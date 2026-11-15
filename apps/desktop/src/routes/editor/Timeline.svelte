<script lang="ts">
  import { Captions, Film, Layers, ZoomIn } from '@shotmaxxing/ui/icons';
  import { Icon, Tooltip } from '@shotmaxxing/ui';
  import { timecode } from '../../lib/format';
  import type { EditorState } from './store.svelte';

  /**
   * Lanes grow on demand: Clip and Zoom always, Effects and Captions once they have something in them.
   * Blocks move and resize with the pointer; their edges have 8 px grab zones.
   */
  let { ed }: { ed: EditorState } = $props();

  let trackW = $state(0);
  const p = $derived(ed.project);
  const pxPerSec = $derived(trackW / p.duration);
  const x = (t: number) => t * pxPerSec;

  const ticks = $derived.by(() => {
    const step = pxPerSec > 40 ? 1 : pxPerSec > 16 ? 2 : 5;
    const out: { t: number; major: boolean }[] = [];
    for (let t = 0; t <= p.duration + 0.001; t += step) out.push({ t, major: t % (step * 5) === 0 });
    return out;
  });

  // Scrubbing: the ruler and the playhead seek while dragged.
  function scrub(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    el.setPointerCapture(e.pointerId);
    ed.pause();
    const move = (ev: PointerEvent) => ed.seek(((ev.clientX - rect.left) / rect.width) * p.duration);
    move(e);
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  type Block = { id: string; start: number; end: number };

  /** Drag a block's body to move it, or an edge to resize it. */
  function drag(e: PointerEvent, kind: 'zoom' | 'effect', block: Block, mode: 'move' | 'start' | 'end') {
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    ed.selection = { kind, id: block.id };
    const startX = e.clientX;
    const orig = { start: block.start, end: block.end };
    const length = orig.end - orig.start;
    const move = (ev: PointerEvent) => {
      const dt = (ev.clientX - startX) / pxPerSec;
      ed.change(mode === 'move' ? 'Move' : 'Resize', () => {
        const list = kind === 'zoom' ? p.zooms : p.effects;
        const target = list.find((b) => b.id === block.id);
        if (!target) return;
        if (mode === 'move') {
          const s = Math.min(p.duration - length, Math.max(0, orig.start + dt));
          target.start = s;
          target.end = s + length;
        } else if (mode === 'start') target.start = Math.min(orig.end - 0.4, Math.max(0, orig.start + dt));
        else target.end = Math.max(orig.start + 0.4, Math.min(p.duration, orig.end + dt));
      });
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  const selectedId = $derived(ed.selection?.id);
  const effectIcon = { blur: 'Blur', text: 'Title', arrow: 'Arrow' };
  // Captions on the recording's timeline: a voice-over's are timed on the finished video, so they're placed where
  // that part of the video comes from.
  const captionBlocks = $derived(
    p.captions.lines.map((line) => {
      const start = ed.captionStart(line);
      const end = line.voiceOver ? ed.sourceTimeAt(line.end) : line.end;
      return { id: line.id, start, end: Math.max(start, end), text: line.text };
    }),
  );
  const captionId = $derived(ed.captionNow?.line.id);
</script>

<div class="flex shrink-0 flex-col border-t border-border bg-chrome" data-tour="timeline">
  <div class="flex">
    <div class="w-24 shrink-0"></div>
    <div
      class="relative h-6 flex-1 cursor-text select-none"
      bind:clientWidth={trackW}
      onpointerdown={scrub}
      role="slider"
      aria-label="Playhead"
      aria-valuemin={0}
      aria-valuemax={p.duration}
      aria-valuenow={Math.round(ed.time * 10) / 10}
      aria-valuetext={timecode(ed.time, true)}
      tabindex="0"
    >
      {#each ticks as tick (tick.t)}
        <span
          class={['absolute bottom-0 w-px', tick.major ? 'h-2.5 bg-fg-subtle' : 'h-1.5 bg-border-strong']}
          style:left="{x(tick.t)}px"
        ></span>
        {#if tick.major}
          <span
            class="absolute top-0.5 -translate-x-1/2 text-caption text-fg-subtle tabular"
            style:left="{x(tick.t)}px">{timecode(tick.t)}</span
          >
        {/if}
      {/each}
    </div>
    <div class="w-3 shrink-0"></div>
  </div>

  <div class="scroll-room relative max-h-48 overflow-y-auto pb-2 pt-1">
    <!-- Clip lane -->
    <div class="flex h-12 items-center">
      <span class="flex w-24 shrink-0 items-center gap-1.5 pl-3 text-footnote text-fg-muted"
        ><Icon icon={Film} size={13} aria-hidden="true" /> Clip</span
      >
      <div class="relative h-10 flex-1">
        {#each p.clips as clip (clip.id)}
          <button
            type="button"
            class={[
              'press-card absolute inset-y-0 overflow-hidden rounded-[7px] bg-fill-strong text-left',
              selectedId === clip.id && 'ring-2 ring-accent ring-inset',
            ]}
            style:left="{x(clip.start) + 1}px"
            style:width="{Math.max(4, x(clip.end - clip.start) - 2)}px"
            onclick={() => (ed.selection = { kind: 'clip', id: clip.id })}
            aria-label="Clip from {timecode(clip.start)} to {timecode(clip.end)}"
          >
            <span class="clip-film absolute inset-0 opacity-60" aria-hidden="true"></span>
            <span class="relative ml-2 mt-1 block truncate text-caption font-semibold text-fg-muted">
              {p.title}{#if clip.speed !== 1}
                · {clip.speed}×{/if}
            </span>
          </button>
        {/each}
      </div>
      <div class="w-3 shrink-0"></div>
    </div>

    <!-- Zoom lane -->
    <div class="flex h-9 items-center" data-tour="zoom-lane">
      <span class="flex w-24 shrink-0 items-center gap-1.5 pl-3 text-footnote text-fg-muted"
        ><Icon icon={ZoomIn} size={13} aria-hidden="true" /> Zoom</span
      >
      <div class="relative h-7 flex-1">
        {#each p.zooms as zoom (zoom.id)}
          <Tooltip
            label={zoom.reason}
            hint="{zoom.scale.toFixed(1)}× · {zoom.source === 'auto' ? 'added by auto zoom' : 'added by you'}"
            side="top"
          >
            <div
              role="button"
              tabindex="0"
              aria-label="Zoom {zoom.scale.toFixed(1)}×: {zoom.reason}"
              class={[
                'absolute inset-y-0 flex cursor-grab items-center overflow-hidden rounded-[6px] text-caption font-semibold active:cursor-grabbing',
                selectedId === zoom.id
                  ? 'bg-accent text-accent-fg'
                  : 'bg-accent-tint text-accent-text hover:brightness-105',
              ]}
              style:left="{x(zoom.start)}px"
              style:width="{Math.max(10, x(zoom.end - zoom.start))}px"
              onpointerdown={(e) => drag(e, 'zoom', zoom, 'move')}
              onkeydown={(e) => e.key === 'Enter' && ed.seek(zoom.start)}
            >
              <span
                class="absolute inset-y-0 left-0 w-2 cursor-ew-resize"
                onpointerdown={(e) => drag(e, 'zoom', zoom, 'start')}
                role="presentation"
              ></span>
              <span class="pointer-events-none truncate px-2.5 tabular"
                >{zoom.scale.toFixed(1)}× {zoom.reason}</span
              >
              <span
                class="absolute inset-y-0 right-0 w-2 cursor-ew-resize"
                onpointerdown={(e) => drag(e, 'zoom', zoom, 'end')}
                role="presentation"
              ></span>
            </div>
          </Tooltip>
        {/each}
      </div>
      <div class="w-3 shrink-0"></div>
    </div>

    {#if p.effects.length}
      <div class="flex h-9 items-center">
        <span class="flex w-24 shrink-0 items-center gap-1.5 pl-3 text-footnote text-fg-muted"
          ><Icon icon={Layers} size={13} aria-hidden="true" /> Effects</span
        >
        <div class="relative h-7 flex-1">
          {#each p.effects as effect (effect.id)}
            <div
              role="button"
              tabindex="0"
              aria-label="{effectIcon[effect.kind]}: {effect.label}"
              class={[
                'absolute inset-y-0 flex cursor-grab items-center overflow-hidden rounded-[6px] text-caption font-semibold',
                selectedId === effect.id ? 'bg-fg text-bg' : 'bg-fill-strong text-fg-muted hover:text-fg',
              ]}
              style:left="{x(effect.start)}px"
              style:width="{Math.max(10, x(effect.end - effect.start))}px"
              onpointerdown={(e) => drag(e, 'effect', effect, 'move')}
              onkeydown={(e) => e.key === 'Enter' && ed.seek(effect.start)}
            >
              <span
                class="absolute inset-y-0 left-0 w-2 cursor-ew-resize"
                onpointerdown={(e) => drag(e, 'effect', effect, 'start')}
                role="presentation"
              ></span>
              <span class="pointer-events-none truncate px-2.5">{effect.label}</span>
              <span
                class="absolute inset-y-0 right-0 w-2 cursor-ew-resize"
                onpointerdown={(e) => drag(e, 'effect', effect, 'end')}
                role="presentation"
              ></span>
            </div>
          {/each}
        </div>
        <div class="w-3 shrink-0"></div>
      </div>
    {/if}

    {#if captionBlocks.length}
      <div class={['flex h-8 items-center', !p.captions.show && 'opacity-50']}>
        <span class="flex w-24 shrink-0 items-center gap-1.5 pl-3 text-footnote text-fg-muted"
          ><Icon icon={Captions} size={13} aria-hidden="true" /> Captions</span
        >
        <div class="relative h-6 flex-1">
          {#each captionBlocks as block (block.id)}
            <button
              type="button"
              class={[
                'absolute inset-y-0 overflow-hidden rounded-[5px] px-1.5 text-left text-caption whitespace-nowrap text-ellipsis outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent',
                captionId === block.id ? 'bg-fg text-bg' : 'bg-fill-strong text-fg-muted hover:text-fg',
              ]}
              style:left="{x(block.start)}px"
              style:width="{Math.max(4, x(block.end - block.start))}px"
              title={block.text}
              aria-label="Caption at {timecode(block.start)}: {block.text}"
              onclick={() => {
                ed.pause();
                ed.seek(block.start);
                ed.tool = 'ai';
              }}>{block.text}</button
            >
          {/each}
        </div>
        <div class="w-3 shrink-0"></div>
      </div>
    {/if}

    <!-- The playhead spans every lane. -->
    <div class="pointer-events-none absolute inset-y-0 left-24 right-3">
      <div
        class="absolute inset-y-0 w-0.5 -translate-x-1/2 bg-record"
        style:transform="translateX({x(ed.time)}px)"
      >
        <span class="absolute -left-[4px] -top-1 size-2.5 rounded-full bg-record"></span>
      </div>
    </div>
  </div>
</div>

<style>
  .clip-film {
    background-image: repeating-linear-gradient(90deg, var(--fill) 0 46px, transparent 46px 48px);
  }
</style>
