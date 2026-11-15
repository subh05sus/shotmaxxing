<script lang="ts" module>
  import type { ExportFormat } from '../../lib/native';

  /** What to export, as chosen in the sheet. */
  export interface ExportChoice {
    width: number;
    height: number;
    fps: number;
    format: ExportFormat;
    bitrate: number;
    /** For a still: the moment of the recording to show, in seconds. */
    at?: number;
  }
</script>

<script lang="ts">
  import { FolderOpen, Info } from '@shotmaxxing/ui/icons';
  import { Advanced, Button, Icon, Keycap, Segmented, Sheet } from '@shotmaxxing/ui';
  import { fileSize, timecode } from '../../lib/format';
  import { chooseExportFolder, isTauri } from '../../lib/native';
  import { settings } from '../../lib/settings.svelte';
  import { ASPECTS, sizeForShort } from './camera';
  import type { Aspect, EditorState } from './store.svelte';

  let {
    open = $bindable(false),
    ed,
    onexport,
  }: { open: boolean; ed: EditorState; onexport: (choice: ExportChoice) => void } = $props();

  type PresetId = 'youtube' | 'social' | 'vertical' | 'gif' | 'uhd' | 'still';
  interface Preset {
    id: PresetId;
    name: string;
    /** The shape the destination expects, when it expects one. */
    aspect?: Aspect;
    /** The shorter side in pixels; GIFs give their longer side instead. */
    short?: number;
    long?: number;
    fps: number;
    format: ExportFormat;
  }
  const PRESETS: Preset[] = [
    { id: 'youtube', name: 'YouTube and the web', short: 1080, fps: 60, format: 'h264' },
    { id: 'social', name: 'X and LinkedIn', aspect: '1:1', short: 1080, fps: 30, format: 'h264' },
    {
      id: 'vertical',
      name: 'Shorts, Reels and TikTok',
      aspect: '9:16',
      short: 1080,
      fps: 30,
      format: 'h264',
    },
    { id: 'gif', name: 'GIF', long: 800, fps: 15, format: 'gif' },
    { id: 'uhd', name: '4K', short: 2160, fps: 60, format: 'hevc' },
    { id: 'still', name: 'This frame as an image', short: 1440, fps: 1, format: 'png' },
  ];
  const SHAPES: Record<string, string> = { '1:1': 'Square', '9:16': 'Vertical (9:16)' };

  let preset = $state<PresetId>('youtube');
  let quality = $state<'small' | 'balanced' | 'high'>('balanced');
  let fps = $state<'30' | '60'>('60');

  const chosen = $derived(PRESETS.find((x) => x.id === preset) ?? PRESETS[0]);
  const p = $derived(ed.project);
  const aspect = $derived(ASPECTS[p.canvas.aspect] ?? p.capture.width / p.capture.height);
  const even = (v: number) => Math.max(16, Math.round(v / 2) * 2);

  /** The video's size: the preset's resolution in the canvas's shape, so the file matches the preview. */
  function sizeFor(item: Preset): { width: number; height: number } {
    if (item.long) {
      return aspect >= 1
        ? { width: even(item.long), height: even(item.long / aspect) }
        : { width: even(item.long * aspect), height: even(item.long) };
    }
    return sizeForShort(aspect, item.short ?? 1080);
  }

  const size = $derived(sizeFor(chosen));
  const still = $derived(chosen.format === 'png');
  const rate = $derived(chosen.format === 'gif' ? 15 : still ? 1 : Number(fps));
  const qualityFactor = $derived(quality === 'small' ? 0.6 : quality === 'high' ? 1.6 : 1);
  // Matches the exporter's own default: about 0.1 bits per pixel per frame for H.264, less for HEVC.
  const bitrate = $derived(
    Math.min(
      120e6,
      Math.max(2e6, size.width * size.height * rate * (chosen.format === 'hevc' ? 0.065 : 0.1)),
    ) * qualityFactor,
  );
  const seconds = $derived(ed.outputDuration);
  // A styled screenshot compresses to roughly a byte a pixel: gradients and text don't squeeze much.
  const bytes = $derived(
    still
      ? size.width * size.height
      : chosen.format === 'gif'
        ? (size.width * size.height * rate * seconds * 0.3) / 8
        : (bitrate / 8) * seconds,
  );
  // Measured on Apple silicon: about 2 ms per megapixel per frame on the GPU, plus a moment to start.
  const eta = $derived(
    0.3 +
      seconds *
        rate *
        Math.max(0.0015, ((size.width * size.height) / 1e6) * 0.0019) *
        (chosen.format === 'gif' ? 3 : 1),
  );
  const etaText = $derived(
    still ? 'a moment' : eta < 60 ? `${Math.max(1, Math.round(eta))} s` : `${Math.round(eta / 60)} min`,
  );
  const formatName = $derived(
    still ? 'PNG' : chosen.format === 'gif' ? 'GIF' : chosen.format === 'hevc' ? 'MP4 (HEVC)' : 'MP4',
  );
  const mismatch = $derived(chosen.aspect && chosen.aspect !== p.canvas.aspect ? chosen.aspect : null);
  const canExport = $derived(!isTauri || !!ed.path);
  const folderLabel = $derived(settings.exportFolder || 'Next to the project');

  function pick(item: Preset) {
    preset = item.id;
    if (item.format !== 'gif' && item.format !== 'png') fps = item.fps === 30 ? '30' : '60';
  }

  async function chooseFolder() {
    const picked = await chooseExportFolder(settings.exportFolder);
    if (picked) settings.exportFolder = picked;
  }

  function submit() {
    if (!canExport) return;
    open = false;
    onexport({
      ...size,
      fps: rate,
      format: chosen.format,
      bitrate: Math.round(bitrate),
      at: still ? ed.time : undefined,
    });
  }
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
      {@const s = sizeFor(item)}
      <button
        type="button"
        role="radio"
        aria-checked={preset === item.id}
        class={[
          'ring-inside press-card flex items-center gap-3 rounded-[10px] border px-3 py-2.5 text-left',
          preset === item.id ? 'border-transparent bg-fill-strong' : 'border-border hover:bg-fill',
        ]}
        onclick={() => pick(item)}
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
          <span class="block text-footnote text-fg-muted tabular"
            >{s.width} × {s.height} · {item.format === 'png'
              ? `PNG · the frame at ${timecode(ed.time)}`
              : item.format === 'gif'
                ? '15 fps · loops'
                : `${preset === item.id ? rate : item.fps} fps · ${item.format === 'hevc' ? 'MP4 (HEVC)' : 'MP4'}`}</span
          >
        </span>
      </button>
    {/each}
  </div>

  {#if mismatch}
    <div
      class="mt-2 flex items-center gap-2.5 rounded-[10px] bg-fill px-3 py-2.5 text-footnote text-fg-muted"
    >
      <Icon icon={Info} size={15} class="shrink-0 text-fg-subtle" aria-hidden="true" />
      <span class="min-w-0 flex-1"
        >{SHAPES[mismatch]} works best for {chosen.name}. This canvas is {p.canvas.aspect === 'original'
          ? 'the recording’s shape'
          : p.canvas.aspect}, and that’s the shape that exports.</span
      >
      <Button size="sm" onclick={() => ed.change('Aspect ratio', () => (ed.project.canvas.aspect = mismatch))}
        >Use {mismatch}</Button
      >
    </div>
  {/if}

  <div class="mt-3">
    <Advanced id="export">
      {#if !still}
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
      {/if}
      {#if chosen.format !== 'gif' && !still}
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
        <span class="flex min-w-0 items-center gap-1">
          {#if settings.exportFolder}
            <Button size="sm" variant="ghost" onclick={() => (settings.exportFolder = '')}>Use default</Button
            >
          {/if}
          <Button size="sm" variant="ghost" icon={FolderOpen} onclick={chooseFolder}>
            <span class="max-w-56 truncate">{folderLabel}</span>
          </Button>
        </span>
      </div>
    </Advanced>
  </div>

  {#snippet footer()}
    {#if canExport}
      <span class="mr-auto text-footnote text-fg-muted tabular"
        >{formatName} · about {fileSize(bytes)} · ready in {still ? '' : 'about '}{etaText}</span
      >
    {:else}
      <span class="mr-auto text-footnote text-fg-muted"
        >Record something to export it. The sample is only a preview.</span
      >
    {/if}
    <Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
    <Button variant="primary" disabled={!canExport} onclick={submit}
      >Export <Keycap chord="return" tone="on-accent" /></Button
    >
  {/snippet}
</Sheet>
