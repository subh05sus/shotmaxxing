<script lang="ts">
  // At the bottom of the sidebar, only while an update is on offer: what it is and how far along it is. Opens the
  // update dialog.
  import { offered, updates } from '../../lib/updates.svelte';

  const found = $derived(updates.found);
  const percent = $derived(updates.progress === null ? null : Math.round(updates.progress * 100));
  const detail = $derived(
    updates.status === 'ready'
      ? 'Ready: restart to update'
      : updates.status === 'installing'
        ? 'Restarting…'
        : updates.status === 'downloading'
          ? `Downloading${percent === null ? '…' : ` ${percent}%`}`
          : updates.status === 'failed'
            ? 'Download stopped'
            : 'See what’s new',
  );
</script>

{#if found && offered()}
  <button
    type="button"
    class="press rise-in relative mx-2.5 mb-0 mt-2.5 overflow-hidden rounded-[9px] bg-fill px-3 py-2.5 text-left shadow-[var(--elev-1)] ring-1 ring-inset ring-border transition-colors duration-[var(--dur-fast)] hover:bg-fill-strong"
    onclick={() => (updates.dialog = true)}
    aria-label="Update to Shotmaxxing {found.version}: {detail}"
  >
    <span class="flex items-center gap-2 text-callout font-semibold text-fg">
      <span class="size-2 shrink-0 rounded-full bg-accent" aria-hidden="true"></span>
      Update to {found.version}
    </span>
    <span class="mt-0.5 block text-caption text-fg-subtle tabular">{detail}</span>
    {#if updates.status === 'downloading'}
      <span class="absolute inset-x-0 bottom-0 h-[2px] bg-fill-strong" aria-hidden="true">
        <span
          class="block h-full origin-left bg-accent transition-transform duration-200"
          style:transform="scaleX({updates.progress ?? 0.15})"
        ></span>
      </span>
    {/if}
  </button>
{/if}
