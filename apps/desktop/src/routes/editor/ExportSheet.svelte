<script lang="ts">
  import { FolderOpen } from '@lucide/svelte';
  import { Advanced, Button, Keycap, Segmented, Sheet, env } from '@shotmaxxing/ui';
  import { fileSize, timecode } from '../../lib/format';
  import type { EditorState } from './store.svelte';

  let {
    open = $bindable(false),
    ed,
    onexport,
  }: { open: boolean; ed: EditorState; onexport: (name: string) => void } = $props();

  type PresetId = 'youtube' | 'social' | 'vertical' | 'gif' | 'uhd';
  const PRESETS: {
    id: PresetId;
    name: string;
    detail: string;
    w: number;
    h: number;
    fps: number;
    mbps: number;
    ext: string;
  }[] = [
    {
      id: 'youtube',
      name: 'YouTube and the web',
      detail: '1920 × 1080 · 60 fps · MP4',
      w: 1920,
      h: 1080,
      fps: 60,
      mbps: 12,
      ext: 'mp4',
    },
    {
      id: 'social',
      name: 'X and LinkedIn',
      detail: '1080 × 1080 · 30 fps · MP4',
      w: 1080,
      h: 1080,
      fps: 30,
      mbps: 6,
      ext: 'mp4',
    },
    {
      id: 'vertical',
      name: 'Shorts, Reels and TikTok',
      detail: '1080 × 1920 · 30 fps · MP4',
      w: 1080,
      h: 1920,
      fps: 30,
      mbps: 8,
      ext: 'mp4',
    },
    {
      id: 'gif',
      name: 'GIF',
      detail: '800 px wide · 15 fps · loops',
      w: 800,
      h: 450,
      fps: 15,
      mbps: 2.2,
      ext: 'gif',
    },
    {
      id: 'uhd',
      name: '4K',
      detail: '3840 × 2160 · 60 fps · MP4 (HEVC)',
      w: 3840,
      h: 2160,
      fps: 60,
      mbps: 40,
      ext: 'mp4',
    },
  ];

  let preset = $state<PresetId>('youtube');
  let quality = $state<'small' | 'balanced' | 'high'>('balanced');
  let fps = $state<'30' | '60'>('60');

  const chosen = $derived(PRESETS.find((x) => x.id === preset) ?? PRESETS[0]);
  const qualityFactor = $derived(quality === 'small' ? 0.6 : quality === 'high' ? 1.6 : 1);
  const seconds = $derived(ed.outputDuration);
  const bytes = $derived(((chosen.mbps * qualityFactor * 1_000_000) / 8) * seconds);
  // Hardware encoders run several times faster than real time; GIFs are slower.
  const eta = $derived(
    Math.max(2, seconds * (chosen.ext === 'gif' ? 0.5 : chosen.id === 'uhd' ? 0.35 : 0.18)),
  );
  const filename = $derived(`${ed.project.title}.${chosen.ext}`);
</script>

<Sheet
  bind:open
  title="Export"
  description="{timecode(seconds)} after cuts. Keep editing while it exports."
  width={560}
  origin="calc(100% - 60px) 0px"
>
  <div class="flex flex-col gap-1.5" role="radiogroup" aria-label="Where is it going?">
    {#each PRESETS as item (item.id)}
      <button
        type="button"
        role="radio"
        aria-checked={preset === item.id}
        class={[
          'ring-inside press-card flex items-center gap-3 rounded-[10px] border px-3 py-2.5 text-left',
          preset === item.id ? 'border-transparent bg-fill-strong' : 'border-border hover:bg-fill',
        ]}
        onclick={() => (preset = item.id)}
      >
        <span
          class={[
            'grid size-4 shrink-0 place-items-center rounded-full border-2',
            preset === item.id ? 'border-accent-text' : 'border-fg-subtle',
          ]}
        >
          {#if preset === item.id}<span class="size-1.5 rounded-full bg-accent-text"></span>{/if}
        </span>
        <span class="min-w-0 flex-1">
          <span class="block text-callout font-semibold">{item.name}</span>
          <span class="block text-footnote text-fg-muted tabular">{item.detail}</span>
        </span>
      </button>
    {/each}
  </div>

  <div class="mt-3">
    <Advanced id="export">
      <div class="flex items-center justify-between gap-3 py-1 text-callout">
        Quality
        <div class="w-56">
          <Segmented
            label="Quality"
            size="sm"
            bind:value={quality}
            options={[
              { value: 'small', label: 'Smaller' },
              { value: 'balanced', label: 'Balanced' },
              { value: 'high', label: 'Higher' },
            ]}
          />
        </div>
      </div>
      {#if chosen.ext !== 'gif'}
        <div class="flex items-center justify-between gap-3 py-1 text-callout">
          Frame rate
          <div class="w-56">
            <Segmented
              label="Frame rate"
              size="sm"
              bind:value={fps}
              options={[
                { value: '30', label: '30 fps' },
                { value: '60', label: '60 fps' },
              ]}
            />
          </div>
        </div>
      {/if}
      <div class="flex items-center justify-between gap-3 py-1 text-callout">
        Save to
        <Button size="sm" variant="ghost" icon={FolderOpen}
          >{env.os === 'mac' ? '~/Movies/Shotmaxxing' : 'Videos\\Shotmaxxing'}</Button
        >
      </div>
    </Advanced>
  </div>

  {#snippet footer()}
    <span class="mr-auto text-footnote text-fg-muted tabular"
      >About {fileSize(bytes)} · ready in about {Math.round(eta)} s</span
    >
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    <Button
      variant="primary"
      onclick={() => {
        open = false;
        onexport(filename);
      }}>Export <Keycap chord="return" tone="on-accent" /></Button
    >
  {/snippet}
</Sheet>
