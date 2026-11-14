<script lang="ts" module>
  export interface ShortcutGroup {
    title: string;
    items: { label: string; chord: string }[];
  }
</script>

<script lang="ts">
  import Sheet from './Sheet.svelte';
  import Keycap from './Keycap.svelte';

  let { open = $bindable(false), groups }: { open: boolean; groups: ShortcutGroup[] } = $props();
</script>

<Sheet
  bind:open
  title="Keyboard shortcuts"
  description="Everything here also lives in the command palette."
  width={640}
>
  <div class="grid grid-cols-1 gap-x-8 gap-y-5 sm:grid-cols-2">
    {#each groups as group (group.title)}
      <section>
        <h3 class="eyebrow mb-1.5">{group.title}</h3>
        <ul>
          {#each group.items as item (item.label)}
            <li
              class="flex h-8 items-center justify-between gap-3 border-b border-border text-callout last:border-0"
            >
              <span class="text-fg">{item.label}</span>
              <Keycap chord={item.chord} />
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  </div>
</Sheet>
