<script lang="ts" module>
  import type { IconData } from '../lib/icons';

  export interface Command {
    id: string;
    title: string;
    group: string;
    icon?: IconData;
    chord?: string;
    keywords?: string;
    run: () => void;
  }
</script>

<script lang="ts">
  import { fuzzyScore } from '../lib/fuzzy';
  import { Search } from '../lib/icons';
  import Icon from './Icon.svelte';
  import { fade, pop, unpop } from '../lib/motion';
  import { portal } from '../lib/portal';
  import Keycap from './Keycap.svelte';

  let { open = $bindable(false), commands }: { open: boolean; commands: Command[] } = $props();

  let query = $state('');
  let active = $state(0);
  let list: HTMLDivElement | undefined = $state();

  const results = $derived.by(() => {
    const scored = commands
      .map((c) => ({ c, s: Math.max(fuzzyScore(query, c.title), fuzzyScore(query, c.keywords ?? '') - 50) }))
      .filter((r) => r.s >= 0);
    if (query.trim()) scored.sort((a, b) => b.s - a.s);
    return scored.map((r) => r.c).slice(0, 40);
  });

  // Groups keep their first-seen order; with a query, the best match leads.
  const grouped = $derived.by(() => {
    const groups = new Map<string, Command[]>();
    for (const c of results) groups.set(c.group, [...(groups.get(c.group) ?? []), c]);
    return [...groups.entries()];
  });

  const flat = $derived(grouped.flatMap(([, items]) => items));

  $effect(() => {
    if (open) {
      query = '';
      active = 0;
    }
  });

  $effect(() => {
    void query;
    active = 0;
  });

  $effect(() => {
    list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  function run(command: Command | undefined) {
    if (!command) return;
    open = false;
    queueMicrotask(command.run);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') (e.preventDefault(), (active = Math.min(flat.length - 1, active + 1)));
    else if (e.key === 'ArrowUp') (e.preventDefault(), (active = Math.max(0, active - 1)));
    else if (e.key === 'Enter') (e.preventDefault(), run(flat[active]));
    else if (e.key === 'Escape') (e.preventDefault(), (open = false));
  }
</script>

{#if open}
  <div use:portal class="fixed inset-0 z-[880] flex justify-center px-4 pt-[14vh]">
    <div
      class="absolute inset-0 bg-scrim"
      in:fade={{ duration: 150 }}
      out:fade={{ duration: 100 }}
      onclick={() => (open = false)}
      role="presentation"
    ></div>
    <div
      role="dialog"
      aria-label="Command palette"
      class="relative flex h-fit max-h-[60vh] w-full max-w-[560px] flex-col overflow-hidden rounded-[14px] border border-border bg-elevated text-fg shadow-[var(--elev-3)]"
      in:pop={{ from: 0.97, origin: 'top center' }}
      out:unpop={{ origin: 'top center' }}
    >
      <label class="flex items-center gap-2.5 border-b border-border px-4">
        <Icon icon={Search} size={17} class="shrink-0 text-fg-subtle" aria-hidden="true" />
        <!-- svelte-ignore a11y_autofocus -->
        <input
          bind:value={query}
          {onkeydown}
          autofocus
          placeholder="Search tools, settings, guides and projects"
          aria-label="Search commands"
          aria-controls="palette-results"
          aria-activedescendant={flat[active] ? `cmd-${flat[active].id}` : undefined}
          class="h-12 min-w-0 flex-1 bg-transparent text-body outline-none placeholder:text-fg-subtle"
        />
        <Keycap chord="Escape" />
      </label>
      <div bind:this={list} id="palette-results" role="listbox" class="scroll-room min-h-0 overflow-y-auto">
        {#each grouped as [group, items] (group)}
          <p class="eyebrow px-2.5 pb-1 pt-2.5">{group}</p>
          {#each items as command (command.id)}
            {@const index = flat.indexOf(command)}
            <button
              type="button"
              id="cmd-{command.id}"
              role="option"
              aria-selected={index === active}
              data-index={index}
              tabindex="-1"
              onpointermove={() => (active = index)}
              onclick={() => run(command)}
              class={[
                'flex h-9 w-full items-center gap-3 rounded-[8px] px-2.5 text-left text-callout',
                index === active ? 'bg-accent text-accent-fg' : 'text-fg',
              ]}
            >
              {#if command.icon}<Icon
                  icon={command.icon}
                  size={16}
                  class="shrink-0 opacity-80"
                  aria-hidden="true"
                />{/if}
              <span class="flex-1 truncate">{command.title}</span>
              {#if command.chord}<Keycap
                  chord={command.chord}
                  tone={index === active ? 'on-accent' : 'default'}
                />{/if}
            </button>
          {/each}
        {:else}
          <p class="px-3 py-8 text-center text-callout text-fg-muted">
            Nothing matches “{query}”. Try a tool name like zoom, cursor or export.
          </p>
        {/each}
      </div>
    </div>
  </div>
{/if}
