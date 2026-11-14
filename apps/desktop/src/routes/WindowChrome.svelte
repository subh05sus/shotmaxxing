<script lang="ts">
  import type { Snippet } from 'svelte';
  import { env } from '@shotmaxxing/ui';
  import { isTauri } from '../lib/native';

  /**
   * In the real app the OS draws the window frame. In a browser preview this draws a stand-in (traffic lights on
   * macOS, a caption bar on Windows), so screenshots look like the real window. Floating windows (the recording pill,
   * the take card) get a desktop-like backdrop instead.
   */
  let { screen, children }: { screen: string; children: Snippet } = $props();

  const floating = $derived(screen === 'pill' || screen === 'take');
</script>

{#if isTauri}
  {@render children()}
{:else if floating}
  <div class="preview-desktop relative h-full w-full overflow-hidden">
    {@render children()}
  </div>
{:else}
  <div class="relative flex h-full flex-col">
    {#if env.os === 'windows'}
      <div
        class="flex h-8 shrink-0 items-center justify-between bg-chrome pl-3 text-footnote text-fg-muted"
        aria-hidden="true"
      >
        <span>Shotmaxxing</span>
        <span class="flex h-full">
          <span class="grid w-11 place-items-center">—</span>
          <span class="grid w-11 place-items-center">▢</span>
          <span class="grid w-11 place-items-center">✕</span>
        </span>
      </div>
    {:else}
      <!-- macOS 26 metrics: 14 pt lights, 23 pt apart, centred in the 52 pt title bar. -->
      <div
        class="pointer-events-none absolute left-[20px] top-[19px] z-[700] flex gap-[9px]"
        aria-hidden="true"
      >
        <span class="size-3.5 rounded-full bg-traffic-close"></span>
        <span class="size-3.5 rounded-full bg-traffic-min"></span>
        <span class="size-3.5 rounded-full bg-traffic-max"></span>
      </div>
    {/if}
    <div class="min-h-0 flex-1">{@render children()}</div>
  </div>
{/if}

<style>
  .preview-desktop {
    background: var(--preview-desktop);
  }
</style>
