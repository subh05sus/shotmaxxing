<script lang="ts" module>
  // Once one tooltip has shown, neighbours show instantly for a moment, so scanning a toolbar isn't slow.
  let warmUntil = 0;
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { place, type Side } from '../lib/position';
  import { pop, unpop } from '../lib/motion';
  import { portal } from '../lib/portal';
  import Keycap from './Keycap.svelte';

  interface Props {
    label: string;
    chord?: string;
    side?: Side;
    /** Extra line under the label. */
    hint?: string;
    children: Snippet;
  }

  let { label, chord, side = 'bottom', hint, children }: Props = $props();

  let anchor: HTMLSpanElement | undefined = $state();
  let tip: HTMLDivElement | undefined = $state();
  let open = $state(false);
  let pos = $state({ x: -9999, y: -9999, origin: 'center' });
  let timer: ReturnType<typeof setTimeout> | undefined;

  function show() {
    clearTimeout(timer);
    const delay = performance.now() < warmUntil ? 0 : 550;
    timer = setTimeout(() => (open = true), delay);
  }

  function hide() {
    clearTimeout(timer);
    if (open) warmUntil = performance.now() + 700;
    open = false;
  }

  $effect(() => {
    if (!open || !tip || !anchor) return;
    const trigger = (anchor.firstElementChild ?? anchor).getBoundingClientRect();
    const p = place(trigger, { width: tip.offsetWidth, height: tip.offsetHeight }, { side, gap: 8 });
    pos = { x: p.x, y: p.y, origin: p.origin };
  });
</script>

<span
  bind:this={anchor}
  class="contents"
  role="presentation"
  onpointerenter={show}
  onpointerleave={hide}
  onfocusin={show}
  onfocusout={hide}
  onpointerdown={hide}
>
  {@render children()}
</span>

{#if open}
  <div
    use:portal
    bind:this={tip}
    role="tooltip"
    class="pointer-events-none fixed left-0 top-0 z-[1000] max-w-64 rounded-[7px] bg-elevated px-2 py-1.5 text-footnote text-fg shadow-[var(--elev-2)]"
    style:translate="{pos.x}px {pos.y}px"
    in:pop={{ from: 0.94, duration: 160, origin: pos.origin }}
    out:unpop={{ duration: 90, origin: pos.origin }}
  >
    <div class="flex items-center gap-2">
      <span class="font-medium">{label}</span>
      {#if chord}<Keycap {chord} />{/if}
    </div>
    {#if hint}<p class="mt-0.5 text-fg-muted">{hint}</p>{/if}
  </div>
{/if}
