<script lang="ts">
  import type { Snippet } from 'svelte';
  import { place, type Align, type Side } from '../lib/position';
  import { pop, unpop } from '../lib/motion';
  import { portal } from '../lib/portal';

  interface Props {
    open: boolean;
    /** The element the popover belongs to. */
    anchor: HTMLElement | undefined;
    side?: Side;
    align?: Align;
    label?: string;
    role?: 'dialog' | 'menu' | 'listbox';
    class?: string;
    onclose?: () => void;
    children: Snippet;
  }

  let {
    open = $bindable(false),
    anchor,
    side = 'bottom',
    align = 'center',
    label,
    role = 'dialog',
    class: className = '',
    onclose,
    children,
  }: Props = $props();

  let panel: HTMLDivElement | undefined = $state();
  let pos = $state({ x: -9999, y: -9999, origin: 'top center' });

  function close() {
    open = false;
    onclose?.();
  }

  $effect(() => {
    if (!open || !panel || !anchor) return;
    const p = place(
      anchor.getBoundingClientRect(),
      { width: panel.offsetWidth, height: panel.offsetHeight },
      { side, align },
    );
    pos = { x: p.x, y: p.y, origin: p.origin };
  });

  $effect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      const target = e.target as Node;
      if (panel?.contains(target) || anchor?.contains(target)) return;
      close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
        anchor?.focus();
      }
    };
    window.addEventListener('pointerdown', onPointer, true);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('blur', close);
    return () => {
      window.removeEventListener('pointerdown', onPointer, true);
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('blur', close);
    };
  });
</script>

{#if open}
  <div
    use:portal
    bind:this={panel}
    {role}
    aria-label={label}
    class={[
      'fixed left-0 top-0 z-[900] rounded-[10px] border border-border bg-elevated text-fg shadow-[var(--elev-2)]',
      className,
    ]}
    style:translate="{pos.x}px {pos.y}px"
    in:pop={{ origin: pos.origin }}
    out:unpop={{ origin: pos.origin }}
  >
    {@render children()}
  </div>
{/if}
