<script lang="ts">
  // The update dialog: what changed since this version, and one button that moves the update along (download,
  // then restart). Skip this version stops the sidebar offering it.
  import { Button, Icon, Sheet, Spinner } from '@shotmaxxing/ui';
  import { ExternalLink } from '@shotmaxxing/ui/icons';
  import { openLink } from '../../lib/native';
  import { CHANGELOG_URL, CURRENT, download, install, skip, updates } from '../../lib/updates.svelte';
  import { settings } from '../../lib/settings.svelte';
  import ReleaseNotes from './ReleaseNotes.svelte';

  const found = $derived(updates.found);
  const percent = $derived(updates.progress === null ? null : Math.round(updates.progress * 100));
  const size = $derived(updates.bytes ? ` · ${Math.max(1, Math.round(updates.bytes / 1e6))} MB` : '');
</script>

{#if found}
  <Sheet
    bind:open={updates.dialog}
    title="Shotmaxxing {found.version} is here"
    description="You have {CURRENT}{size}"
    width={540}
  >
    {#if updates.notes === null}
      <p class="flex items-center gap-2 py-6 text-callout text-fg-muted">
        <Spinner size={14} /> Loading what’s new…
      </p>
    {:else if updates.notes.length === 0}
      <p class="py-4 text-callout text-fg-muted">
        The notes for this version couldn’t be loaded. The changelog on GitHub has them.
      </p>
    {:else}
      <ReleaseNotes versions={updates.notes} />
    {/if}
    <button
      type="button"
      class="mt-4 inline-flex items-center gap-1.5 text-footnote font-medium text-fg-muted hover:text-fg"
      onclick={() => void openLink(CHANGELOG_URL)}
    >
      Full changelog <Icon icon={ExternalLink} size={13} aria-hidden="true" />
    </button>
    {#if updates.status === 'failed'}
      <p class="mt-3 text-footnote text-danger" role="alert">The download stopped: {updates.error}</p>
    {/if}

    {#snippet footer()}
      {#if settings.skippedVersion !== found.version}
        <Button variant="ghost" class="mr-auto" onclick={skip}>Skip this version</Button>
      {:else}
        <span class="mr-auto text-footnote text-fg-subtle">You skipped this version.</span>
      {/if}
      <Button variant="secondary" onclick={() => (updates.dialog = false)}>Later</Button>
      {#if updates.status === 'ready'}
        <Button variant="primary" autofocus onclick={() => void install()}>Restart to update</Button>
      {:else if updates.status === 'installing'}
        <Button variant="primary" autofocus loading disabled>Restarting…</Button>
      {:else if updates.status === 'downloading'}
        <Button variant="primary" autofocus loading disabled>
          Downloading{percent === null ? '…' : ` ${percent}%`}
        </Button>
      {:else}
        <Button variant="primary" autofocus onclick={() => void download()}>
          {updates.status === 'failed' ? 'Try again' : 'Download'}
        </Button>
      {/if}
    {/snippet}
  </Sheet>
{/if}
