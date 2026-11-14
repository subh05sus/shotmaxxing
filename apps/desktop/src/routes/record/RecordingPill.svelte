<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Ellipsis, Pause, Play, RotateCcw, Square, Trash } from '@lucide/svelte';
  import { Menu, Tooltip, toast, type MenuItem } from '@shotmaxxing/ui';
  import { go } from '../../lib/router.svelte';
  import { timecode } from '../../lib/format';

  /**
   * The only thing on screen while recording. It is excluded from capture, has no animation of its own, and updates
   * its timer once a second, so it costs next to nothing.
   */
  let elapsed = $state(0);
  let paused = $state(false);
  let last = performance.now();

  const timer = setInterval(() => {
    const now = performance.now();
    if (!paused) elapsed += (now - last) / 1000;
    last = now;
  }, 1000);
  onDestroy(() => clearInterval(timer));

  function stop() {
    clearInterval(timer);
    go('editor/sample');
  }

  const more: MenuItem[] = [
    {
      label: 'Restart',
      icon: RotateCcw,
      run: () => {
        elapsed = 0;
        toast('Started over. The earlier part was discarded.');
      },
    },
    { label: 'Discard recording', icon: Trash, danger: true, run: () => go('home/library') },
  ];
</script>

<div class="flex h-full items-start justify-center pt-4">
  <div
    class="flex h-11 items-center gap-1 rounded-full border border-border bg-elevated py-1 pl-3.5 pr-1.5 text-fg shadow-[var(--elev-3)]"
    role="toolbar"
    aria-label="Recording"
  >
    <span class={['size-2.5 rounded-full', paused ? 'bg-fg-subtle' : 'bg-record']} aria-hidden="true"></span>
    <span class="ml-1.5 w-12 text-callout font-semibold tabular" aria-live="off">{timecode(elapsed)}</span>
    <span class="mx-1 h-5 w-px bg-border" aria-hidden="true"></span>
    <Tooltip label={paused ? 'Resume' : 'Pause'} chord="alt+shift+p">
      <button
        type="button"
        aria-label={paused ? 'Resume' : 'Pause'}
        class="press-icon grid size-8 place-items-center rounded-full text-fg-muted hover:bg-fill hover:text-fg"
        onclick={() => (paused = !paused)}
      >
        {#if paused}<Play size={16} aria-hidden="true" />{:else}<Pause size={16} aria-hidden="true" />{/if}
      </button>
    </Tooltip>
    <Tooltip label="Stop and edit" chord="alt+shift+5">
      <button
        type="button"
        aria-label="Stop and edit"
        class="press grid h-8 place-items-center rounded-full bg-record px-3 text-callout font-semibold text-record-fg"
        onclick={stop}
      >
        <span class="flex items-center gap-1.5"
          ><Square size={11} fill="currentColor" aria-hidden="true" /> Stop</span
        >
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
