<script lang="ts">
  import { Info } from '../lib/icons';
  import Icon from './Icon.svelte';
  import Popover from './Popover.svelte';

  /** A small ⓘ that explains a setting in a sentence or two. */
  let { text, title }: { text: string; title?: string } = $props();

  let button: HTMLButtonElement | undefined = $state();
  let open = $state(false);
</script>

<button
  bind:this={button}
  type="button"
  aria-label={title ? `About ${title}` : 'More info'}
  aria-expanded={open}
  onclick={() => (open = !open)}
  class="press-icon -m-1.5 inline-flex size-6 items-center justify-center rounded-full text-fg-subtle hover:text-fg-muted"
>
  <Icon icon={Info} size={13} aria-hidden="true" />
</button>

<Popover bind:open anchor={button} side="top" label={title ?? 'More info'} class="w-64 p-3">
  {#if title}<p class="mb-1 text-callout font-semibold">{title}</p>{/if}
  <p class="text-footnote leading-[1.45] text-fg-muted">{text}</p>
</Popover>
