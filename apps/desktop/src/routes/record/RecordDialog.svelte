<script lang="ts">
  import { onMount } from 'svelte';
  import { AppWindow, Bot, ChevronDown, Circle, Crop, Mic, Monitor, Video, Volume2 } from '@lucide/svelte';
  import {
    Button,
    Keycap,
    Menu,
    Segmented,
    Switch,
    env,
    fade,
    pop,
    toast,
    type MenuItem,
  } from '@shotmaxxing/ui';
  import Scene from '../../components/Scene.svelte';
  import RecordDot from '../../components/RecordDot.svelte';
  import {
    closeThisWindow,
    handOff,
    isTauri,
    listDevices,
    listSources,
    openPermissionSettings,
    sourceThumbnails,
    type Device,
    startRecording,
    type CaptureSource,
  } from '../../lib/native';
  import { recordingOptions, settings } from '../../lib/settings.svelte';
  import { wallpaper } from '../../lib/wallpapers';

  type Mode = 'display' | 'window' | 'area';

  let mode = $state<Mode>('window');
  let sources = $state<CaptureSource[]>([]);
  let selected = $state<string | null>(null);
  let microphones = $state<Device[]>([]);
  let cameras = $state<Device[]>([]);
  /** The chosen device when it's still plugged in, otherwise the system's default. */
  const pickDevice = (list: Device[], id: string) =>
    list.find((d) => d.id === id) ?? list.find((d) => d.is_default) ?? list[0];
  const micDevice = $derived(pickDevice(microphones, settings.microphoneId));
  const cameraDevice = $derived(pickDevice(cameras, settings.cameraId));
  const micName = $derived(micDevice?.name ?? 'No microphone');
  const cameraName = $derived(cameraDevice?.name ?? 'No camera');
  // Switches show off while no such device is plugged in, without forgetting the choice for next time.
  const micOn = $derived(settings.microphone && !!micDevice);
  const cameraOn = $derived(settings.camera && !!cameraDevice);
  let countdown = $state<number | null>(null);
  let starting = $state(false);
  let loadError = $state('');
  let tileWidth = $state(0);
  let tick: ReturnType<typeof setInterval> | undefined;

  async function load() {
    loadError = '';
    try {
      sources = await listSources();
      selected = sources.find((s) => s.kind === 'window')?.id ?? sources[0]?.id ?? null;
    } catch (error) {
      loadError = error instanceof Error ? error.message : String(error);
      return;
    }
    // Pictures arrive a moment later; tiles show a placeholder until then.
    const pictures = await sourceThumbnails(sources.map((s) => s.id)).catch(() => ({}));
    sources = sources.map((s) => ({
      ...s,
      thumbnail: pictures[s.id as keyof typeof pictures] ?? s.thumbnail,
    }));
  }

  async function loadDevices() {
    const found = await listDevices().catch(() => ({ microphones: [], cameras: [] }));
    microphones = found.microphones;
    cameras = found.cameras;
  }

  onMount(() => {
    void load();
    void loadDevices();
    return () => clearInterval(tick);
  });

  const shown = $derived(sources.filter((s) => (mode === 'area' ? s.kind === 'display' : s.kind === mode)));

  $effect(() => {
    if (shown.length && !shown.some((s) => s.id === selected)) selected = shown[0].id;
  });

  const sceneFor: Record<string, number> = { Safari: 4, Figma: 1, Notes: 5, Terminal: 2, Xcode: 1 };

  function record() {
    if (!selected || starting || countdown !== null) return;
    if (!settings.countdown) return void begin();
    countdown = 3;
    tick = setInterval(() => {
      countdown = (countdown ?? 1) - 1;
      if (countdown <= 0) {
        clearInterval(tick);
        countdown = null;
        void begin();
      }
    }, 1000);
  }

  function cancelCountdown() {
    clearInterval(tick);
    countdown = null;
  }

  async function begin() {
    if (!selected) return;
    starting = true;
    try {
      // Area recording records the whole screen for now; the crop is applied in the editor.
      await startRecording(selected, undefined, {
        ...recordingOptions(),
        microphone: micOn ? micDevice?.id : undefined,
        camera: cameraOn ? cameraDevice?.id : undefined,
      });
      // Without the pill, Stop lives in the menu bar and on the shortcut.
      if (settings.showPill) await handOff('pill');
      else await closeThisWindow();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      const permission = /Screen Recording/i.test(message);
      toast(message, {
        tone: 'danger',
        timeout: 8000,
        action: permission
          ? { label: 'Open settings', run: () => openPermissionSettings('screen') }
          : undefined,
      });
    } finally {
      starting = false;
    }
  }

  const micItems: MenuItem[] = $derived(
    microphones.map((d) => ({ label: d.name, run: () => (settings.microphoneId = d.id) })),
  );
  const cameraItems: MenuItem[] = $derived(
    cameras.map((d) => ({ label: d.name, run: () => (settings.cameraId = d.id) })),
  );

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !(e.target instanceof HTMLInputElement) && countdown === null) record();
    if (e.key === 'Escape' && countdown !== null) cancelCountdown();
  }
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col bg-bg">
  <header
    data-tauri-drag-region
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
      {#if loadError}
        <p class="col-span-full px-1 py-6 text-center text-callout text-fg-muted" role="alert">
          {loadError}
          <button type="button" class="ml-1 font-medium text-accent-text underline" onclick={load}
            >Try again</button
          >
        </p>
      {/if}
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
            {#if source.thumbnail && source.kind === 'display'}
              <img src={source.thumbnail} alt="" class="absolute inset-0 size-full object-cover" in:fade />
            {:else if source.thumbnail}
              <!-- A window floats on the tile, its own shape, so its rounded corners stay rounded. -->
              <span class="absolute inset-0 grid place-items-center">
                <img
                  src={source.thumbnail}
                  alt=""
                  class="max-h-[84%] max-w-[88%] rounded-[5px] shadow-[var(--elev-2)]"
                  in:fade
                />
              </span>
            {:else if isTauri}
              <span class="absolute inset-0 grid place-items-center text-fg-subtle">
                {#if source.kind === 'display'}<Monitor size={28} strokeWidth={1.5} aria-hidden="true" />
                {:else}<AppWindow size={28} strokeWidth={1.5} aria-hidden="true" />{/if}
              </span>
            {:else if tileWidth}
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
      <Switch
        checked={micOn}
        onchange={(on: boolean) => (settings.microphone = on)}
        label="Microphone"
        disabled={!microphones.length}
      />
      {#if microphones.length > 1}
        <Menu items={micItems} label="Microphone">
          {#snippet trigger({ props })}
            <button
              type="button"
              disabled={!micOn}
              class="press flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill disabled:opacity-40"
              {...props}
            >
              {micName}<ChevronDown size={12} aria-hidden="true" />
            </button>
          {/snippet}
        </Menu>
      {:else}
        <span class={['px-1.5 text-footnote', micOn ? 'text-fg-muted' : 'text-fg-subtle']}>{micName}</span>
      {/if}
    </span>
    <span class="flex items-center gap-2">
      <Video size={15} class="text-fg-muted" aria-hidden="true" />
      <Switch
        checked={cameraOn}
        onchange={(on: boolean) => (settings.camera = on)}
        label="Camera"
        disabled={!cameras.length}
      />
      {#if cameras.length > 1}
        <Menu items={cameraItems} label="Camera">
          {#snippet trigger({ props })}
            <button
              type="button"
              disabled={!cameraOn}
              class="press flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill disabled:opacity-40"
              {...props}
            >
              {cameraName}<ChevronDown size={12} aria-hidden="true" />
            </button>
          {/snippet}
        </Menu>
      {:else}
        <span class={['px-1.5 text-footnote', cameraOn ? 'text-fg-muted' : 'text-fg-subtle']}
          >{cameraName}</span
        >
      {/if}
    </span>
    <span class="flex items-center gap-2">
      <Volume2 size={15} class="text-fg-muted" aria-hidden="true" />
      <Switch bind:checked={settings.systemAudio} label="System audio" hideLabel={false} />
    </span>
  </div>

  <footer class="flex h-16 shrink-0 items-center gap-3 border-t border-border px-5">
    <button
      type="button"
      class="press flex items-center gap-1.5 rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:bg-fill hover:text-fg"
      onclick={() => handOff('home', 'home/agents')}
    >
      <Bot size={14} aria-hidden="true" /> Let an agent film it instead
    </button>
    <span class="flex-1"></span>
    <span class="text-footnote text-fg-subtle"
      >{settings.countdown ? '3-second countdown' : 'Starts right away'}</span
    >
    <Button variant="primary" size="lg" icon={RecordDot} disabled={!selected || starting} onclick={record}>
      {starting ? 'Starting…' : 'Record'}
      <Keycap chord="return" tone="on-accent" />
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
