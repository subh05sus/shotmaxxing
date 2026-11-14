<script lang="ts">
  import {
    ArrowDownUp,
    Bot,
    Camera,
    Copy,
    Download,
    Film,
    FolderOpen,
    Pencil,
    SearchX,
    Trash,
  } from '@lucide/svelte';
  import { flip } from 'svelte/animate';
  import {
    Button,
    EmptyState,
    Menu,
    SearchField,
    Segmented,
    env,
    spring,
    toast,
    type MenuItem,
  } from '@shotmaxxing/ui';
  import LibraryCard from './LibraryCard.svelte';
  import { LIBRARY, type LibraryItem } from '../../lib/sample';
  import { openWindow, revealInFolder } from '../../lib/native';
  import { go, route } from '../../lib/router.svelte';
  import { registerCommands } from '../../lib/commands.svelte';

  type Filter = 'all' | 'recording' | 'screenshot' | 'agent';
  type Sort = 'newest' | 'oldest' | 'longest' | 'name';

  let items = $state<LibraryItem[]>(route.query.get('empty') ? [] : [...LIBRARY]);
  let filter = $state<Filter>('all');
  let sort = $state<Sort>('newest');
  let query = $state('');
  let selected = $state<string | null>(null);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = items.filter((item) => {
      if (filter === 'recording' && item.kind !== 'recording') return false;
      if (filter === 'screenshot' && item.kind !== 'screenshot') return false;
      if (filter === 'agent' && !item.agent) return false;
      return !q || `${item.title} ${item.app} ${item.agent ?? ''}`.toLowerCase().includes(q);
    });
    const by: Record<Sort, (a: LibraryItem, b: LibraryItem) => number> = {
      newest: (a, b) => b.created.getTime() - a.created.getTime(),
      oldest: (a, b) => a.created.getTime() - b.created.getTime(),
      longest: (a, b) => b.duration - a.duration,
      name: (a, b) => a.title.localeCompare(b.title),
    };
    return list.sort(by[sort]);
  });

  const reveal = env.os === 'mac' ? 'Show in Finder' : 'Show in File Explorer';

  function open(item: LibraryItem) {
    openWindow('editor', item.id === 'launch-demo' ? 'sample' : item.id);
  }

  function remove(item: LibraryItem) {
    const at = items.indexOf(item);
    items.splice(at, 1);
    if (selected === item.id) selected = null;
    toast(`Moved “${item.title}” to the Trash`, {
      action: { label: 'Undo', run: () => items.splice(at, 0, item) },
    });
  }

  function duplicate(item: LibraryItem) {
    const copy = {
      ...item,
      id: `${item.id}-copy-${Date.now()}`,
      title: `${item.title} copy`,
      created: new Date(),
    };
    items.splice(items.indexOf(item) + 1, 0, copy);
    toast(`Duplicated “${item.title}”`);
  }

  function menuFor(item: LibraryItem): MenuItem[] {
    return [
      { label: 'Open', icon: Film, chord: 'return', run: () => open(item) },
      { label: 'Rename', icon: Pencil, run: () => toast('Rename from the editor title for now') },
      { label: 'Duplicate', icon: Copy, chord: 'mod+d', run: () => duplicate(item) },
      { label: reveal, icon: FolderOpen, run: () => revealInFolder(item.id) },
      { label: 'Export…', icon: Download, chord: 'mod+e', run: () => go(`editor/${item.id}?export=1`) },
      {
        label: 'Move to Trash',
        icon: Trash,
        chord: 'mod+backspace',
        danger: true,
        separated: true,
        run: () => remove(item),
      },
    ];
  }

  $effect(() =>
    registerCommands('library', [
      {
        id: 'lib-recordings',
        title: 'Show recordings only',
        group: 'Library',
        icon: Film,
        run: () => (filter = 'recording'),
      },
      {
        id: 'lib-screens',
        title: 'Show screenshots only',
        group: 'Library',
        icon: Camera,
        run: () => (filter = 'screenshot'),
      },
      {
        id: 'lib-agents',
        title: 'Show agent takes',
        group: 'Library',
        icon: Bot,
        run: () => (filter = 'agent'),
      },
    ]),
  );

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement) return;
    const item = items.find((i) => i.id === selected);
    if (!item) return;
    if ((e.key === 'Backspace' || e.key === 'Delete') && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      remove(item);
    }
  }

  const sortItems: MenuItem[] = [
    { label: 'Newest first', run: () => (sort = 'newest') },
    { label: 'Oldest first', run: () => (sort = 'oldest') },
    { label: 'Longest first', run: () => (sort = 'longest') },
    { label: 'Name', run: () => (sort = 'name') },
  ];
  const sortLabel: Record<Sort, string> = {
    newest: 'Newest',
    oldest: 'Oldest',
    longest: 'Longest',
    name: 'Name',
  };
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col">
  {#if items.length}
    <div class="flex items-center gap-3 px-5 pb-3 pt-1">
      <div class="w-[420px] shrink-0">
        <Segmented
          label="Show"
          bind:value={filter}
          options={[
            { value: 'all', label: 'All' },
            { value: 'recording', label: 'Recordings' },
            { value: 'screenshot', label: 'Screenshots' },
            { value: 'agent', label: 'From agents' },
          ]}
        />
      </div>
      <span class="flex-1"></span>
      <div class="w-52">
        <SearchField bind:value={query} placeholder="Filter by name or app" label="Filter recordings" />
      </div>
      <Menu items={sortItems} label="Sort" align="end">
        {#snippet trigger({ props })}
          <Button size="sm" variant="ghost" icon={ArrowDownUp} {...props}>{sortLabel[sort]}</Button>
        {/snippet}
      </Menu>
    </div>
  {/if}

  <div class="scroll-room min-h-0 flex-1 overflow-y-auto px-5 pb-6">
    {#if !items.length}
      <EmptyState
        icon={Film}
        title="Your recordings will live here"
        body="Record your screen, or open the sample project to see what Shotmaxxing does with a recording."
      >
        <Button variant="record" onclick={() => openWindow('record')}>New recording</Button>
        <Button onclick={() => openWindow('editor', 'sample')}>Open the sample</Button>
      </EmptyState>
    {:else if !visible.length}
      {#if query}
        <EmptyState
          icon={SearchX}
          title="Nothing matches “{query}”"
          body="Try the app name, like Safari or Figma, or clear the filter."
        >
          <Button onclick={() => (query = '')}>Clear filter</Button>
        </EmptyState>
      {:else if filter === 'agent'}
        <EmptyState
          icon={Bot}
          title="No agent takes yet"
          body="Connect Claude Code, Codex or another agent, then ask it to film a demo."
        >
          <Button variant="primary" onclick={() => go('home/agents')}>Connect an agent</Button>
        </EmptyState>
      {:else}
        <EmptyState
          icon={Camera}
          title="No screenshots yet"
          body="Press ⌥⇧4 and click a window for a styled screenshot."
        >
          <Button onclick={() => (filter = 'all')}>Show everything</Button>
        </EmptyState>
      {/if}
    {:else}
      <div class="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-x-4 gap-y-5">
        {#each visible as item, i (item.id)}
          <div animate:flip={{ duration: 260, easing: spring }}>
            <LibraryCard
              {item}
              index={i}
              selected={selected === item.id}
              menu={menuFor(item)}
              onselect={() => (selected = item.id)}
              onopen={() => open(item)}
            />
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
