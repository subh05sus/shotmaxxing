<script lang="ts">
  import { Search, X } from '@lucide/svelte';
  import Keycap from './Keycap.svelte';

  let {
    value = $bindable(''),
    placeholder = 'Search',
    chord,
    label = 'Search',
    input = $bindable(),
  }: {
    value: string;
    placeholder?: string;
    chord?: string;
    label?: string;
    input?: HTMLInputElement;
  } = $props();
</script>

<label
  class="group flex h-[var(--os-control-height)] min-h-7 w-full items-center gap-2 rounded-control border border-border bg-fill px-2.5 text-callout transition-colors duration-[var(--dur-fast)] focus-within:border-ring focus-within:bg-surface"
>
  <Search size={14} class="shrink-0 text-fg-subtle" aria-hidden="true" />
  <input
    bind:this={input}
    bind:value
    {placeholder}
    aria-label={label}
    class="min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle"
    onkeydown={(e) => {
      if (e.key === 'Escape' && value) {
        e.stopPropagation();
        value = '';
      }
    }}
  />
  {#if value}
    <button
      type="button"
      aria-label="Clear search"
      class="press-icon -mr-1 grid size-5 place-items-center rounded-full text-fg-subtle hover:text-fg"
      onclick={() => (value = '')}
    >
      <X size={12} aria-hidden="true" />
    </button>
  {:else if chord}
    <span class="opacity-0 transition-opacity duration-[var(--dur-fast)] group-hover:opacity-100"
      ><Keycap {chord} /></span
    >
  {/if}
</label>
