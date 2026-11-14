<script lang="ts">
  import type { Snippet } from 'svelte';

  /** An inset group of rows with a small heading, like the settings on macOS and Windows. */
  let { title, footer, children }: { title?: string; footer?: string; children: Snippet } = $props();
</script>

<section class="flex flex-col gap-1.5">
  {#if title}<h3 class="eyebrow px-1">{title}</h3>{/if}
  <div
    class="divide-y divide-border rounded-card border border-border bg-surface px-4 shadow-[var(--elev-1)]"
  >
    {@render children()}
  </div>
  {#if footer}<p class="px-1 text-footnote text-fg-muted">{footer}</p>{/if}
</section>
