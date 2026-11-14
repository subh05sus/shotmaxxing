<script lang="ts">
  import { onMount } from 'svelte';
  import { AppWindow, Bot, ChevronDown, Circle, Crop, Mic, Monitor, Video, Volume2 } from '@lucide/svelte';
  import { Button, Keycap, Menu, Segmented, Switch, env, fade, pop, type MenuItem } from '@shotmaxxing/ui';
  import Scene from '../../components/Scene.svelte';
  import { listSources, type CaptureSource } from '../../lib/native';
  import { settings } from '../../lib/settings.svelte';
  import { go } from '../../lib/router.svelte';
  import { wallpaper } from '../../lib/wallpapers';

  type Mode = 'display' | 'window' | 'area';

  let mode = $state<Mode>('window');
  let sources = $state<CaptureSource[]>([]);
  let selected = $state<string | null>(null);
  let camera = $state(false);
  let mic = $state(true);
  let systemAudio = $state(true);
  let micName = $state(env.os === 'mac' ? 'MacBook Pro Microphone' : 'Microphone Array');
  let cameraName = $state(env.os === 'mac' ? 'FaceTime HD Camera' : 'Integrated Camera');
  let countdown = $state<number | null>(null);
  let tileWidth = $state(0);

  onMount(async () => {
    sources = await listSources();
    selected = sources.find((s) => s.kind === 'window')?.id ?? null;
  });

  const shown = $derived(sources.filter((s) => (mode === 'area' ? s.kind === 'display' : s.kind === mode)));

  $effect(() => {
    if (shown.length && !shown.some((s) => s.id === selected)) selected = shown[0].id;
  });

  const sceneFor: Record<string, number> = { Safari: 4, Figma: 1, Notes: 5, Terminal: 2, Xcode: 1 };

  function record() {
    if (!settings.countdown) return go('pill');
    countdown = 3;
    const tick = setInterval(() => {
      countdown = (countdown ?? 1) - 1;
      if (countdown <= 0) {
        clearInterval(tick);
        go('pill');
      }
    }, 1000);
  }

  const micItems: MenuItem[] = [
    {
      label: env.os === 'mac' ? 'MacBook Pro Microphone' : 'Microphone Array',
      run: () => (micName = env.os === 'mac' ? 'MacBook Pro Microphone' : 'Microphone Array'),
    },
    { label: 'Studio Display Microphone', run: () => (micName = 'Studio Display Microphone') },
    { label: 'USB Audio Interface', run: () => (micName = 'USB Audio Interface') },
  ];
  const cameraItems: MenuItem[] = [
    {
      label: env.os === 'mac' ? 'FaceTime HD Camera' : 'Integrated Camera',
      run: () => (cameraName = env.os === 'mac' ? 'FaceTime HD Camera' : 'Integrated Camera'),
    },
    { label: 'Studio Display Camera', run: () => (cameraName = 'Studio Display Camera') },
  ];

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement) && countdown === null) record();
    if (e.key === 'Escape' && countdown !== null) countdown = null;
  }
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col bg-bg">
  <header
    class="drag-region flex h-[var(--titlebar-height)] shrink-0 items-center gap-3 pl-[var(--titlebar-inset-start)] pr-4"
  >
    <h1 class="text-title-3">New recording</h1>
    <span class="flex-1"></span>
    <div class="no-drag w-[300px]">
      <Segmented
        label="What to record"
        bind:value={mode}
        options={[
          { value: 'display', label: 'Screen', icon: Monitor },
          { value: 'window', label: 'Window', icon: AppWindow },
          { value: 'area', label: 'Area', icon: Crop },
        ]}
      />
    </div>
  </header>

  <div class="scroll-room min-h-0 flex-1 overflow-y-auto px-4 pb-2">
    {#if mode === 'area'}
      <p class="px-1 pb-2 text-footnote text-fg-muted">
        Pick a screen, then drag over the part you want to record.
      </p>
    {/if}
    <div
      class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3"
      role="radiogroup"
      aria-label="Source"
    >
      {#each shown as source, i (source.id)}
        <button
          type="button"
          role="radio"
          aria-checked={selected === source.id}
          class="press-card rise-in group flex flex-col gap-1.5 rounded-card p-1.5 text-left hover:bg-fill"
          style="--i: {i}"
          onclick={() => (selected = source.id)}
          ondblclick={record}
        >
          <span
            bind:clientWidth={tileWidth}
            class={[
              'relative block aspect-[16/10] w-full overflow-hidden rounded-[8px] shadow-[var(--elev-1)]',
              selected === source.id && 'ring-2 ring-accent ring-offset-2 ring-offset-bg',
            ]}
            style:background={source.kind === 'display'
              ? wallpaper(i % 2 ? 'ocean' : 'aurora-lime').css
              : 'var(--fill-strong)'}
          >
            {#if tileWidth}
              {#if source.kind === 'display'}
                <span
                  class="absolute left-[10%] top-[12%] block aspect-[16/10] w-[62%] overflow-hidden rounded-[3px] shadow-[var(--elev-2)]"
                >
                  <Scene variant={i ? 1 : 0} width={tileWidth * 0.62} t={6} />
                </span>
              {:else}
                <Scene variant={sceneFor[source.app ?? ''] ?? 0} width={tileWidth} t={6} />
              {/if}
            {/if}
          </span>
          <span class="flex items-baseline gap-1.5 px-0.5">
            <span class="truncate text-callout font-medium">{source.app ?? source.title}</span>
            <span class="truncate text-footnote text-fg-muted"
              >{source.app ? source.title : `${source.width} × ${source.height}`}</span
            >
          </span>
        </button>
      {/each}
    </div>
  </div>

  <div class="flex shrink-0 flex-wrap items-center gap-x-5 gap-y-2 border-t border-border px-5 py-3">
    <span class="flex items-center gap-2">
      <Mic size={15} class="text-fg-muted" aria-hidden="true" />
      <Switch bind:checked={mic} label="Microphone" />
      <Menu items={micItems} label="Microphone">
        {#snippet trigger({ props })}
          <button
            type="button"
            disabled={!mic}
            class="press flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill disabled:opacity-40"
            {...props}
          >
            {micName}<ChevronDown size={12} aria-hidden="true" />
          </button>
        {/snippet}
      </Menu>
    </span>
    <span class="flex items-center gap-2">
      <Video size={15} class="text-fg-muted" aria-hidden="true" />
      <Switch bind:checked={camera} label="Camera" />
      <Menu items={cameraItems} label="Camera">
        {#snippet trigger({ props })}
          <button
            type="button"
            disabled={!camera}
            class="press flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill disabled:opacity-40"
            {...props}
          >
            {cameraName}<ChevronDown size={12} aria-hidden="true" />
          </button>
        {/snippet}
      </Menu>
    </span>
    <span class="flex items-center gap-2">
      <Volume2 size={15} class="text-fg-muted" aria-hidden="true" />
      <Switch bind:checked={systemAudio} label="System audio" hideLabel={false} />
    </span>
  </div>

  <footer class="flex h-16 shrink-0 items-center gap-3 border-t border-border px-5">
    <button
      type="button"
      class="press flex items-center gap-1.5 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill hover:text-fg"
      onclick={() => go('home/agents')}
    >
      <Bot size={14} aria-hidden="true" /> Let an agent film it instead
    </button>
    <span class="flex-1"></span>
    <span class="text-footnote text-fg-subtle"
      >{settings.countdown ? '3-second countdown' : 'Starts right away'}</span
    >
    <Button variant="record" size="lg" icon={Circle} disabled={!selected} onclick={record}>
      Record <Keycap chord="return" tone="on-accent" />
    </Button>
  </footer>
</div>

{#if countdown !== null}
  <div class="fixed inset-0 z-[900] grid place-items-center bg-scrim" in:fade={{ duration: 120 }}>
    <div class="flex flex-col items-center gap-3">
      {#key countdown}
        <span
          class="text-[120px] font-bold leading-none text-fg-on-stage tabular"
          in:pop={{ from: 0.6, duration: 260 }}>{countdown}</span
        >
      {/key}
      <span class="text-callout text-fg-on-stage/80">Press Esc to cancel</span>
    </div>
  </div>
{/if}
