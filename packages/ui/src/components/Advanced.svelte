<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronRight } from '../lib/icons';
  import Icon from './Icon.svelte';
  import { rise } from '../lib/motion';

  /**
   * Progressive disclosure: presets first, the fine controls one click away. Once opened it stays open (remembered
   * per key), because someone who wants the details once usually wants them again.
   */
  let { id, label = 'Advanced', children }: { id: string; label?: string; children: Snippet } = $props();

  const key = $derived(`smx.advanced.${id}`);
  let open = $state(false);

  $effect(() => {
    try {
      open = localStorage.getItem(key) === '1';
    } catch {
      open = false;
    }
  });

  function toggle() {
    open = !open;
    try {
      localStorage.setItem(key, open ? '1' : '0');
    } catch {
      /* storage can be unavailable; the toggle still works for this session */
    }
  }
</script>

<div class="border-t border-border pt-2">
  <button
    type="button"
    aria-expanded={open}
    onclick={toggle}
    class="press flex h-8 w-full items-center gap-1.5 rounded-[6px] px-1 text-callout font-medium text-fg-muted hover:text-fg"
  >
    <Icon
      icon={ChevronRight}
      size={14}
      aria-hidden="true"
      class={['transition-[rotate] duration-[var(--dur-fast)] ease-[var(--ease-out)]', open && 'rotate-90']}
    />
    {label}
  </button>
  {#if open}
    <div class="flex flex-col gap-1 pb-1 pt-1" in:rise={{ y: 4, duration: 200 }}>
      {@render children()}
    </div>
  {/if}
</div>
