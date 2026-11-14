<script lang="ts">
  import { env } from '../lib/env.svelte';
  import { chordCaps, chordLabel } from '../lib/keys';

  /** A chord such as `mod+shift+z`. `mod` is ⌘ on macOS and Ctrl on Windows. */
  let {
    chord,
    size = 'sm',
    tone = 'default',
  }: { chord: string; size?: 'sm' | 'md'; tone?: 'default' | 'on-accent' } = $props();

  const caps = $derived(chordCaps(chord, env.os));
</script>

<span class="inline-flex items-center gap-[3px]" aria-label={chordLabel(chord, env.os)}>
  {#each caps as cap, i (i)}
    <kbd
      aria-hidden="true"
      class={[
        'inline-flex items-center justify-center rounded-[5px] font-sans font-medium leading-none tabular',
        size === 'sm'
          ? 'h-[18px] min-w-[18px] px-1 text-caption'
          : 'h-[22px] min-w-[22px] px-1.5 text-footnote',
        tone === 'on-accent'
          ? 'bg-accent-fg/15 text-accent-fg'
          : 'border border-border bg-fill text-fg-muted shadow-[0_1px_0_var(--border)]',
      ]}>{cap}</kbd
    >
  {/each}
</span>
