<script lang="ts">
  // Shown once, on the first launch after an update: what's new in this version and any skipped on the way.
  import { Button, Sheet } from '@shotmaxxing/ui';
  import { openLink } from '../../lib/native';
  import { CHANGELOG_URL, CURRENT, updates } from '../../lib/updates.svelte';
  import ReleaseNotes from './ReleaseNotes.svelte';

  // Home works out what's new as it mounts, after this does.
  let open = $state(false);
  $effect(() => {
    if (updates.whatsNew.length) open = true;
  });
  const close = () => {
    open = false;
    updates.whatsNew = [];
  };
</script>

{#if updates.whatsNew.length}
  <Sheet
    bind:open
    title="What’s new in {CURRENT}"
    description="Shotmaxxing is up to date."
    width={540}
    onclose={close}
  >
    <ReleaseNotes versions={updates.whatsNew} />
    {#snippet footer()}
      <Button variant="ghost" class="mr-auto" onclick={() => void openLink(CHANGELOG_URL)}
        >Full changelog</Button
      >
      <Button variant="primary" autofocus onclick={close}>Got it</Button>
    {/snippet}
  </Sheet>
{/if}
