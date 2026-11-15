<script lang="ts">
  /**
   * The Home window's one search, in the top bar: ⌘K and the Library's filter in one field. Typing filters the
   * Library while it shows, and the list under the field offers the first matching recordings and actions. Enter runs
   * the highlighted one, Esc clears the search, and ⌘K jumps here from anywhere in the window.
   */
  import { Icon, Keycap, fuzzyScore, pop, unpop } from '@shotmaxxing/ui';
  import { Camera, Film, Search, X, type IconData } from '@shotmaxxing/ui/icons';
  import { allCommands } from '../../lib/commands.svelte';
  import { homeSearch } from '../../lib/search.svelte';
  import { go, route } from '../../lib/router.svelte';

  interface Row {
    id: string;
    group: string;
    title: string;
    detail?: string;
    icon?: IconData;
    chord?: string;
    run: () => void;
    /** Keeps the search as it is afterwards: opening a match, or showing the Library for it. */
    keep?: boolean;
  }

  let input: HTMLInputElement | undefined = $state();
  let listOpen = $state(false);
  let active = $state(0);

  const onLibrary = $derived((route.path[1] ?? 'library') === 'library');
  const query = $derived(homeSearch.query.trim());

  const rows = $derived.by(() => {
    const out: Row[] = [];
    if (query && onLibrary) {
      for (const match of homeSearch.matches) {
        out.push({
          id: `open-${match.id}`,
          group: match.kind === 'screenshot' ? 'Screenshots' : 'Recordings',
          title: match.title,
          detail: match.detail,
          icon: match.kind === 'screenshot' ? Camera : Film,
          run: match.open,
          keep: true,
        });
      }
    }
    const scored = allCommands()
      .map((c) => ({ c, s: Math.max(fuzzyScore(query, c.title), fuzzyScore(query, c.keywords ?? '') - 50) }))
      .filter((r) => r.s >= 0);
    if (query) scored.sort((a, b) => b.s - a.s);
    for (const { c } of scored.slice(0, query ? 6 : 9)) {
      out.push({ id: c.id, group: c.group, title: c.title, icon: c.icon, chord: c.chord, run: c.run });
    }
    if (query && !onLibrary) {
      out.push({
        id: 'find-in-library',
        group: 'Library',
        title: `Find “${query}” in the Library`,
        icon: Search,
        run: () => go('home/library'),
        keep: true,
      });
    }
    return out;
  });

  // Groups keep their first-seen order, so the best match leads; the keys move through them in the order shown.
  const grouped = $derived.by(() => {
    const groups = new Map<string, Row[]>();
    for (const row of rows) groups.set(row.group, [...(groups.get(row.group) ?? []), row]);
    return [...groups.entries()];
  });
  const flat = $derived(grouped.flatMap(([, items]) => items));
  const showing = $derived(listOpen && flat.length > 0);

  $effect(() => {
    void query;
    active = 0;
  });

  // ⌘K: into the search, with what's there selected so typing replaces it.
  $effect(() => {
    if (!homeSearch.focus) return;
    input?.focus();
    input?.select();
  });

  function run(row: Row | undefined) {
    if (!row) return;
    listOpen = false;
    if (!row.keep) homeSearch.query = '';
    input?.blur();
    queueMicrotask(row.run);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (!listOpen) listOpen = true;
      else active = Math.min(flat.length - 1, active + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = Math.max(0, active - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (showing) run(flat[active]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (homeSearch.query) homeSearch.query = '';
      else {
        listOpen = false;
        input?.blur();
      }
    }
  }
</script>

<div class="no-drag relative w-72">
  <label
    class="flex h-7 items-center gap-2 rounded-control border border-border bg-fill px-2.5 text-callout transition-colors duration-[var(--dur-fast)] focus-within:border-ring focus-within:bg-surface hover:bg-fill-hover focus-within:hover:bg-surface"
  >
    <Icon icon={Search} size={14} class="shrink-0 text-fg-subtle" aria-hidden="true" />
    <input
      bind:this={input}
      bind:value={homeSearch.query}
      {onkeydown}
      onfocus={() => (listOpen = true)}
      onblur={() => (listOpen = false)}
      oninput={() => (listOpen = true)}
      placeholder="Search recordings and actions"
      aria-label="Search recordings and actions"
      role="combobox"
      aria-expanded={showing}
      aria-controls="home-search-results"
      aria-autocomplete="list"
      aria-activedescendant={showing && flat[active] ? `home-search-${flat[active].id}` : undefined}
      data-tour="palette"
      class="h-full min-w-0 flex-1 bg-transparent text-fg outline-none placeholder:text-fg-subtle"
    />
    {#if homeSearch.query}
      <button
        type="button"
        aria-label="Clear the search"
        class="press grid size-5 shrink-0 place-items-center rounded-full text-fg-subtle hover:bg-fill-hover hover:text-fg"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => {
          homeSearch.query = '';
          input?.focus();
        }}
      >
        <Icon icon={X} size={12} aria-hidden="true" />
      </button>
    {:else}
      <Keycap chord="mod+k" />
    {/if}
  </label>

  {#if showing}
    <div
      id="home-search-results"
      role="listbox"
      aria-label="Matching recordings and actions"
      class="scroll-room absolute right-0 top-[calc(100%+6px)] z-[700] max-h-[min(60vh,420px)] w-[340px] overflow-y-auto rounded-[12px] border border-border bg-elevated text-fg shadow-[var(--elev-3)]"
      in:pop={{ from: 0.97, origin: 'top right' }}
      out:unpop={{ origin: 'top right' }}
    >
      {#each grouped as [group, items] (group)}
        <p class="eyebrow px-2.5 pb-1 pt-2">{group}</p>
        {#each items as row (row.id)}
          {@const index = flat.indexOf(row)}
          <button
            type="button"
            id="home-search-{row.id}"
            role="option"
            aria-selected={index === active}
            tabindex="-1"
            onmousedown={(e) => e.preventDefault()}
            onpointermove={() => (active = index)}
            onclick={() => run(row)}
            class={[
              'flex h-9 w-full items-center gap-3 rounded-[8px] px-2.5 text-left text-callout',
              index === active ? 'bg-accent text-accent-fg' : 'text-fg',
            ]}
          >
            {#if row.icon}<Icon
                icon={row.icon}
                size={16}
                class="shrink-0 opacity-80"
                aria-hidden="true"
              />{/if}
            <span class="min-w-0 flex-1 truncate"
              >{row.title}{#if row.detail}<span class={index === active ? 'opacity-75' : 'text-fg-subtle'}
                  >{' · '}{row.detail}</span
                >{/if}</span
            >
            {#if row.chord}<Keycap chord={row.chord} tone={index === active ? 'on-accent' : 'default'} />{/if}
          </button>
        {/each}
      {/each}
    </div>
  {/if}
</div>
