<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Ellipsis, RotateCcw, Square, Trash } from '@lucide/svelte';
  import { Menu, Spinner, Tooltip, toast, type MenuItem } from '@shotmaxxing/ui';
  import {
    discardRecording,
    editorTarget,
    handOff,
    isTauri,
    restartRecording,
    stopRecording,
  } from '../../lib/native';
  import { timecode } from '../../lib/format';

  /**
   * The only thing on screen while recording. It is left out of the recording, has no animation of its own, and
   * updates its timer once a second, so it costs next to nothing.
   */
  let elapsed = $state(0);
  let busy = $state<'saving' | 'restarting' | 'discarding' | null>(null);
  let started = performance.now();

  const timer = setInterval(() => (elapsed = (performance.now() - started) / 1000), 1000);
  onDestroy(() => clearInterval(timer));

  // Stopping from the menu bar or with ⌥⇧5 happens in the app; the pill just says it's saving until it closes.
  onMount(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    void import('@tauri-apps/api/webviewWindow').then(({ getCurrentWebviewWindow }) =>
      getCurrentWebviewWindow()
        .listen('recording-saving', () => {
          busy = 'saving';
          clearInterval(timer);
        })
        .then((stop) => (unlisten = stop)),
    );
    return () => unlisten?.();
  });

  function failed(error: unknown) {
    busy = null;
    toast(error instanceof Error ? error.message : String(error), { tone: 'danger', timeout: 8000 });
  }

  async function stop() {
    if (busy) return;
    busy = 'saving';
    clearInterval(timer);
    try {
      const done = await stopRecording();
      if (done.dropped_frames > 0) {
        toast(`${done.dropped_frames} frames were dropped while the computer was busy.`);
      }
      await handOff('editor', editorTarget(done.project));
    } catch (error) {
      failed(error);
    }
  }

  const more: MenuItem[] = [
    {
      label: 'Restart',
      icon: RotateCcw,
      run: async () => {
        busy = 'restarting';
        try {
          await restartRecording();
          started = performance.now();
          elapsed = 0;
          busy = null;
          toast('Started over. The earlier part was discarded.');
        } catch (error) {
          failed(error);
        }
      },
    },
    {
      label: 'Discard recording',
      icon: Trash,
      danger: true,
      run: async () => {
        busy = 'discarding';
        try {
          await discardRecording();
          await handOff('home', 'home/library');
        } catch (error) {
          failed(error);
        }
      },
    },
  ];
</script>

<div class="flex h-full items-start justify-center pt-4">
  <div
    class="flex h-11 items-center gap-1 rounded-full border border-border bg-elevated py-1 pl-3.5 pr-1.5 text-fg shadow-[var(--elev-3)]"
    role="toolbar"
    aria-label="Recording"
  >
    <span class={['size-2.5 rounded-full', busy ? 'bg-fg-subtle' : 'bg-record']} aria-hidden="true"></span>
    <span class="ml-1.5 w-12 text-callout font-semibold tabular" aria-live="off">{timecode(elapsed)}</span>
    <span class="mx-1 h-5 w-px bg-border" aria-hidden="true"></span>
    <Tooltip label="Stop and edit" chord="alt+shift+5">
      <button
        type="button"
        aria-label="Stop and edit"
        class="press grid h-8 place-items-center rounded-full bg-accent px-3 text-callout font-semibold text-accent-fg"
        disabled={busy !== null}
        onclick={stop}
      >
        <span class="flex items-center gap-1.5">
          {#if busy}
            <Spinner size={12} />
            {busy === 'saving' ? 'Saving' : busy === 'restarting' ? 'Restarting' : 'Discarding'}
          {:else}
            <Square size={11} fill="currentColor" aria-hidden="true" /> Stop
          {/if}
        </span>
      </button>
    </Tooltip>
    <Menu items={more} label="Recording options" align="end">
      {#snippet trigger({ props })}
        <button
          type="button"
          aria-label="More"
          class="press-icon grid size-8 place-items-center rounded-full text-fg-muted hover:bg-fill hover:text-fg"
          {...props}
        >
          <Ellipsis size={16} aria-hidden="true" />
        </button>
      {/snippet}
    </Menu>
  </div>
</div>
