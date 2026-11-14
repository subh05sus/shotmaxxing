<script lang="ts">
  import { ChevronDown, Pause, Play, StepBack, StepForward } from '@lucide/svelte';
  import { IconButton, Menu, type MenuItem } from '@shotmaxxing/ui';
  import { timecode } from '../../lib/format';
  import type { Aspect, EditorState } from './store.svelte';

  let { ed }: { ed: EditorState } = $props();

  const rates: MenuItem[] = [0.5, 1, 1.5, 2].map((r) => ({ label: `${r}×`, run: () => (ed.rate = r) }));
  const aspects: { value: Aspect; label: string }[] = [
    { value: '16:9', label: '16:9 Wide' },
    { value: '4:3', label: '4:3 Classic' },
    { value: '1:1', label: '1:1 Square' },
    { value: '9:16', label: '9:16 Vertical' },
    { value: 'original', label: 'Original size' },
  ];
  const aspectItems: MenuItem[] = aspects.map((a) => ({
    label: a.label,
    run: () => ed.change('Aspect ratio', () => (ed.project.canvas.aspect = a.value)),
  }));
</script>

<div class="flex h-12 shrink-0 items-center gap-1 bg-stage px-3 text-fg-on-stage" data-tour="transport">
  <div class="flex w-48 items-center gap-1">
    <Menu items={aspectItems} label="Aspect ratio" side="top">
      {#snippet trigger({ props })}
        <button
          type="button"
          class="press flex h-7 items-center gap-1 rounded-[6px] px-2 text-footnote font-medium text-fg-on-stage/80 hover:bg-on-stage-hover"
          {...props}
        >
          {ed.project.canvas.aspect === 'original' ? 'Original' : ed.project.canvas.aspect}
          <ChevronDown size={12} aria-hidden="true" />
        </button>
      {/snippet}
    </Menu>
  </div>
  <span class="flex-1"></span>
  <IconButton
    icon={StepBack}
    label="Back 5 seconds"
    chord="shift+left"
    side="top"
    tone="on-stage"
    onclick={() => ed.seek(ed.time - 5)}
  />
  <button
    type="button"
    aria-label={ed.playing ? 'Pause' : 'Play'}
    title={ed.playing ? 'Pause (Space)' : 'Play (Space)'}
    class="press-icon mx-1 grid size-9 place-items-center rounded-full bg-on-stage-fill text-fg-on-stage hover:bg-on-stage-hover"
    onclick={() => ed.toggle()}
  >
    {#if ed.playing}<Pause size={18} fill="currentColor" aria-hidden="true" />{:else}<Play
        size={18}
        fill="currentColor"
        class="translate-x-px"
        aria-hidden="true"
      />{/if}
  </button>
  <IconButton
    icon={StepForward}
    label="Forward 5 seconds"
    chord="shift+right"
    side="top"
    tone="on-stage"
    onclick={() => ed.seek(ed.time + 5)}
  />
  <span class="flex-1"></span>
  <div class="flex w-48 items-center justify-end gap-2">
    <span class="text-footnote tabular">
      <span class="text-fg-on-stage">{timecode(ed.time, true)}</span>
      <span class="text-fg-on-stage/50"> / {timecode(ed.project.duration)}</span>
    </span>
    <Menu items={rates} label="Playback speed" side="top" align="end">
      {#snippet trigger({ props })}
        <button
          type="button"
          class="press h-7 rounded-[6px] px-2 text-footnote font-medium text-fg-on-stage/80 tabular hover:bg-on-stage-hover"
          {...props}
        >
          {ed.rate}×
        </button>
      {/snippet}
    </Menu>
  </div>
</div>
