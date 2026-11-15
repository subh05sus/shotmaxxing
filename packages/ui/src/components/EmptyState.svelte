<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconData } from '../lib/icons';
  import Icon from './Icon.svelte';

  /** Teach and offer one action: a soft tile, a title, one line, one button. */
  let { icon, title, body, children }: { icon: IconData; title: string; body: string; children?: Snippet } =
    $props();
</script>

<div class="rise-in mx-auto flex max-w-sm flex-col items-center px-6 py-12 text-center">
  <div class="mb-4 grid size-14 place-items-center rounded-[16px] bg-fill text-fg-muted">
    <Icon {icon} size={26} aria-hidden="true" />
  </div>
  <h3 class="text-title-3">{title}</h3>
  <p class="mt-1 text-callout text-fg-muted">{body}</p>
  {#if children}<div class="mt-5 flex flex-wrap items-center justify-center gap-2">
      {@render children()}
    </div>{/if}
</div>
