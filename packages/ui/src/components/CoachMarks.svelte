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
  let root: HTMLDivElement | undefined = $state();
  let card: HTMLDivElement | undefined = $state();

  /** Room between the spotlight and the window's edge, which would cut its 2 px ring and the halo around that. */
  const EDGE = 4;
  /** How far the spotlight reaches past its target. */
  const PAD = 6;
  /** Corner rounding: the spotlight's own, and where it hugs a corner of the window, the window's curve. */
  const ROUND = 12;
  const ROUND_AT_CORNER = 22;
  /**
   * When to look for a stop's target again, in ms: it may not be laid out yet (the editor still loading, a panel
   * sliding in), and it settles within about a second.
   */
  const LOOK_AGAIN = [0, 80, 200, 420, 800, 1300];

  /** The spotlight, once its target is found and measured. */
  let hole = $state<{ x: number; y: number; w: number; h: number; radius: string } | null>(null);
  /** The spotlight has been placed once, so it glides from there on instead of flying in from a corner. */
  let placed = $state(false);
  let cardPos = $state<{ x: number; y: number } | null>(null);

  $effect(() => {
    if (open) {
      index = Math.min(start, tour.length - 1);
      placed = false;
    } else {
      hole = null;
      cardPos = null;
    }
  });

  /** Measures the stop's target and places the spotlight and the card. Returns the target, when it's there. */
  function measure(): Element | null {
    const step = tour[index];
    const el = step ? document.querySelector(step.target) : null;
    const r = el?.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    if (!el || !r || r.width < 1 || r.height < 1) {
      // Nothing to point at: the card waits in the middle, over the dimmed window.
      hole = null;
      if (card) cardPos = { x: (vw - card.offsetWidth) / 2, y: (vh - card.offsetHeight) / 2 };
      return null;
    }
    const x = Math.max(EDGE, r.left - PAD);
    const y = Math.max(EDGE, r.top - PAD);
    const right = Math.min(vw - EDGE, r.right + PAD);
    const bottom = Math.min(vh - EDGE, r.bottom + PAD);
    const at = {
      left: x <= EDGE + 1,
      top: y <= EDGE + 1,
      right: right >= vw - EDGE - 1,
      bottom: bottom >= vh - EDGE - 1,
    };
    const corner = (side: boolean, end: boolean) => `${side && end ? ROUND_AT_CORNER : ROUND}px`;
    hole = {
      x,
      y,
      w: Math.max(0, right - x),
      h: Math.max(0, bottom - y),
      radius: [
        corner(at.left, at.top),
        corner(at.right, at.top),
        corner(at.right, at.bottom),
        corner(at.left, at.bottom),
      ].join(' '),
    };
    if (card) {
      const p = place(r, { width: card.offsetWidth, height: card.offsetHeight }, { side: 'right', gap: 18 });
      cardPos = { x: p.x, y: p.y };
    }
    // After this frame paints, moves glide.
    if (!placed) requestAnimationFrame(() => (placed = true));
    return el;
  }

  // Follows the target while the stop shows: as it appears, changes size, scrolls or the window resizes. Nothing
  // runs between those, so the tour costs nothing while it waits for the person.
  $effect(() => {
    if (!open) return;
    void index;
    void tour;
    let frame = 0;
    let watched: Element | null = null;
    const sized = new ResizeObserver(() => soon());
    function soon() {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const el = measure();
        if (el !== watched) {
          if (watched) sized.unobserve(watched);
          if (el) sized.observe(el);
          watched = el;
        }
      });
    }
    // A panel or page finishing its entrance moves what it holds; the tour's own glides don't count.
    const settled = (e: Event) => {
      if (!(e.target instanceof Node) || !root?.contains(e.target)) soon();
    };
    const timers = LOOK_AGAIN.map((ms) => setTimeout(soon, ms));
    window.addEventListener('resize', soon);
    document.addEventListener('scroll', soon, { capture: true, passive: true });
    document.addEventListener('transitionend', settled, true);
    document.addEventListener('animationend', settled, true);
    return () => {
      timers.forEach(clearTimeout);
      cancelAnimationFrame(frame);
      sized.disconnect();
      window.removeEventListener('resize', soon);
      document.removeEventListener('scroll', soon, { capture: true });
      document.removeEventListener('transitionend', settled, true);
      document.removeEventListener('animationend', settled, true);
    };
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
  <div bind:this={root} use:portal class="fixed inset-0 z-[850]" role="presentation">
    {#if hole}
      <!-- Staging: one spotlight, everything else dimmed. The spotlight glides between targets. -->
      <div
        class={[
          'absolute left-0 top-0 shadow-[0_0_0_4px_color-mix(in_oklab,var(--ring)_28%,transparent),0_0_0_9999px_var(--scrim)] ring-2 ring-ring',
          placed && 'transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]',
        ]}
        style:width="{hole.w}px"
        style:height="{hole.h}px"
        style:translate="{hole.x}px {hole.y}px"
        style:border-radius={hole.radius}
        in:fade={{ duration: 180 }}
        out:fade={{ duration: 120 }}
      ></div>
    {:else}
      <div class="absolute inset-0 bg-scrim" in:fade={{ duration: 180 }} out:fade={{ duration: 120 }}></div>
    {/if}
    <div
      bind:this={card}
      role="dialog"
      aria-label="Tour: {step.title}"
      class={[
        'absolute left-0 top-0 w-72 rounded-[12px] border border-border bg-elevated p-4 text-fg shadow-[var(--elev-3)]',
        placed && 'transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]',
      ]}
      style:translate={cardPos ? `${cardPos.x}px ${cardPos.y}px` : undefined}
      style:visibility={cardPos ? undefined : 'hidden'}
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
