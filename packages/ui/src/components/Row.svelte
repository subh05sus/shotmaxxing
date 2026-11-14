<script lang="ts">
  import type { Snippet } from 'svelte';
  import InfoTip from './InfoTip.svelte';

  /** A settings row: a label (and an optional line under it) with its control on the right. */
  let {
    label,
    description,
    info,
    children,
  }: { label: string; description?: string; info?: string; children?: Snippet } = $props();
</script>

<div class="flex min-h-11 items-center gap-4 py-2">
  <div class="min-w-0 flex-1">
    <div class="flex items-center gap-1 text-callout text-fg">
      {label}
      {#if info}<InfoTip text={info} title={label} />{/if}
    </div>
    {#if description}<p class="mt-0.5 text-footnote text-fg-muted">{description}</p>{/if}
  </div>
  {#if children}<div class="flex shrink-0 items-center gap-2">{@render children()}</div>{/if}
</div>
