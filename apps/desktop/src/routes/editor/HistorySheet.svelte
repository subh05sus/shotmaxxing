<script lang="ts">
  import { Button, Sheet, toast } from '@shotmaxxing/ui';
  import { relativeTime } from '../../lib/format';
  import type { EditorState } from './store.svelte';

  let { open = $bindable(false), ed }: { open: boolean; ed: EditorState } = $props();
</script>

<Sheet
  bind:open
  title="Version history"
  description="Saved automatically after each quiet minute of editing. Restoring keeps the current version too."
  width={480}
>
  <ol class="flex flex-col">
    <li class="flex items-center gap-3 border-b border-border py-2.5">
      <span class="size-2 rounded-full bg-accent" aria-hidden="true"></span>
      <span class="flex-1">
        <span class="block text-callout font-semibold">Current version</span>
        <span class="block text-footnote text-fg-muted"
          >{ed.saving ? 'Saving…' : `Saved ${relativeTime(new Date(ed.savedAt))}`}</span
        >
      </span>
    </li>
    {#each ed.versions as version (version.id)}
      <li class="flex items-center gap-3 border-b border-border py-2.5 last:border-0">
        <span class="size-2 rounded-full bg-fg-subtle" aria-hidden="true"></span>
        <span class="min-w-0 flex-1">
          <span class="block truncate text-callout">{version.label}</span>
          <span class="block text-footnote text-fg-muted">{relativeTime(new Date(version.at))}</span>
        </span>
        <Button
          size="sm"
          onclick={() => {
            ed.restore(version);
            open = false;
            toast(`Restored “${version.label}”`, { action: { label: 'Undo', run: () => ed.undo() } });
          }}>Restore</Button
        >
      </li>
    {/each}
  </ol>
</Sheet>
