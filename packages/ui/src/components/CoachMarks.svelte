<script lang="ts" module>
  export interface CoachStep {
    /** CSS selector of the thing to point at. */
    target: string;
    title: string;
    body: string;
    /** Optional shortcut to teach. */
    chord?: string;
  }
</script>

<script lang="ts">
  import { place } from '../lib/position';
  import { fade, pop } from '../lib/motion';
  import { portal } from '../lib/portal';
  import Button from './Button.svelte';
  import Keycap from './Keycap.svelte';

  interface Props {
    steps: CoachStep[];
    open: boolean;
    /** Start at a given step, e.g. when a tool's ? button replays its stop. */
    start?: number;
    onfinish?: (completed: boolean) => void;
  }

  let { steps, open = $bindable(false), start = 0, onfinish }: Props = $props();

  // Miller's law: a tour longer than seven stops gets skipped, so it is capped here.
  const tour = $derived(steps.slice(0, 7));
  let index = $state(0);
  let card: HTMLDivElement | undefined = $state();
  let hole = $state({ x: 0, y: 0, w: 0, h: 0 });
  let cardPos = $state({ x: -9999, y: -9999 });

  $effect(() => {
    if (open) index = Math.min(start, tour.length - 1);
  });

  function measure() {
    const step = tour[index];
    const el = step && document.querySelector(step.target);
    if (!el) return;
    const r = el.getBoundingClientRect();
    hole = { x: r.left - 6, y: r.top - 6, w: r.width + 12, h: r.height + 12 };
    if (card) {
      const p = place(r, { width: card.offsetWidth, height: card.offsetHeight }, { side: 'right', gap: 18 });
      cardPos = { x: p.x, y: p.y };
    }
  }

  $effect(() => {
    if (!open) return;
    void index;
    measure();
    const onResize = () => measure();
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  });

  function finish(completed: boolean) {
    open = false;
    onfinish?.(completed);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') finish(false);
    if (e.key === 'ArrowRight' || e.key === 'Enter') next();
    if (e.key === 'ArrowLeft' && index > 0) index--;
  }

  function next() {
    if (index < tour.length - 1) index++;
    else finish(true);
  }
</script>

<svelte:window {onkeydown} />

{#if open && tour.length}
  {@const step = tour[index]}
  <div use:portal class="fixed inset-0 z-[850]" role="presentation">
    <!-- Staging: one spotlight, everything else dimmed. The hole glides between targets. -->
    <div
      class="absolute left-0 top-0 rounded-[12px] shadow-[0_0_0_9999px_var(--scrim)] ring-2 ring-accent transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]"
      style:width="{hole.w}px"
      style:height="{hole.h}px"
      style:translate="{hole.x}px {hole.y}px"
      in:fade={{ duration: 180 }}
      out:fade={{ duration: 120 }}
    ></div>
    <div
      bind:this={card}
      role="dialog"
      aria-label="Tour: {step.title}"
      class="absolute left-0 top-0 w-72 rounded-[12px] border border-border bg-elevated p-4 text-fg shadow-[var(--elev-3)] transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]"
      style:translate="{cardPos.x}px {cardPos.y}px"
      in:pop={{ duration: 220 }}
    >
      <p class="eyebrow tabular">{index + 1} of {tour.length}</p>
      {#key index}
        <div in:fade={{ duration: 160 }}>
          <h3 class="mt-1 text-title-3">{step.title}</h3>
          <p class="mt-1 text-callout leading-[1.45] text-fg-muted">{step.body}</p>
          {#if step.chord}
            <p class="mt-2 flex items-center gap-1.5 text-footnote text-fg-muted">
              Shortcut <Keycap chord={step.chord} />
            </p>
          {/if}
        </div>
      {/key}
      <div class="mt-4 flex items-center gap-2">
        <button
          type="button"
          class="press rounded-[6px] px-1.5 py-1 text-footnote text-fg-muted hover:text-fg"
          onclick={() => finish(false)}
        >
          Skip tour
        </button>
        <span class="flex-1"></span>
        {#if index > 0}<Button size="sm" variant="ghost" onclick={() => index--}>Back</Button>{/if}
        <Button size="sm" variant="primary" onclick={next}
          >{index === tour.length - 1 ? 'Got it' : 'Next'}</Button
        >
      </div>
    </div>
  </div>
{/if}
