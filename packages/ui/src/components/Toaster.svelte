<script lang="ts">
  import { CircleCheck, CircleAlert, X } from '@lucide/svelte';
  import { flip } from 'svelte/animate';
  import { dismiss, holdToasts, toasts } from '../lib/toasts.svelte';
  import { rise, sink, spring } from '../lib/motion';
</script>

<!-- Toasts are solid elevated surfaces, bottom centre, and announced politely to screen readers. -->
<div
  class="pointer-events-none fixed inset-x-0 bottom-5 z-[950] flex flex-col items-center gap-2"
  aria-live="polite"
  role="status"
  onpointerenter={() => holdToasts(true)}
  onpointerleave={() => holdToasts(false)}
>
  {#each toasts as t (t.id)}
    <div
      class="pointer-events-auto flex min-h-10 max-w-[min(480px,calc(100vw-32px))] items-center gap-2.5 rounded-[10px] border border-border bg-elevated py-2 pl-3 pr-1.5 text-callout text-fg shadow-[var(--elev-2)]"
      in:rise={{ y: 12, duration: 240 }}
      out:sink={{ y: 8 }}
      animate:flip={{ duration: 220, easing: spring }}
    >
      {#if t.tone === 'success'}
        <CircleCheck size={16} class="shrink-0 text-success" aria-hidden="true" />
      {:else if t.tone === 'danger'}
        <CircleAlert size={16} class="shrink-0 text-danger" aria-hidden="true" />
      {/if}
      <span class="min-w-0 flex-1">{t.message}</span>
      {#if t.action}
        {@const action = t.action}
        <button
          type="button"
          class="press h-7 rounded-[6px] px-2 font-semibold text-accent-text hover:bg-fill"
          onclick={() => {
            action.run();
            dismiss(t.id);
          }}>{action.label}</button
        >
      {/if}
      <button
        type="button"
        aria-label="Dismiss"
        class="press-icon inline-flex size-7 items-center justify-center rounded-[6px] text-fg-subtle hover:bg-fill hover:text-fg"
        onclick={() => dismiss(t.id)}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  {/each}
</div>
