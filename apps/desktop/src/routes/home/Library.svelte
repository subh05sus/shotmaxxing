<script lang="ts">
  import {
    ArrowDownUp,
    Bot,
    Camera,
    Copy,
    Download,
    Film,
    FolderOpen,
    Image,
    Pencil,
    RecordDot,
    SearchX,
    Trash,
  } from '@shotmaxxing/ui/icons';
  import { flip } from 'svelte/animate';
  import {
    Button,
    EmptyState,
    Menu,
    SearchField,
    Segmented,
    Spinner,
    env,
    matchesChord,
    spring,
    toast,
    type MenuItem,
  } from '@shotmaxxing/ui';
  import LibraryCard from './LibraryCard.svelte';
  import { counts } from '../../lib/counts.svelte';
  import { LIBRARY, type LibraryItem } from '../../lib/sample';
  import { onMount } from 'svelte';
  import {
    copyScreenshot,
    duplicateProject,
    editorTarget,
    exportProject,
    isTauri,
    listProjects,
    mediaUrl,
    openWindow,
    revealInFolder,
    trashProject,
    type ProjectSummary,
  } from '../../lib/native';
  import { go, route } from '../../lib/router.svelte';
  import { registerCommands } from '../../lib/commands.svelte';
  import { settings } from '../../lib/settings.svelte';

  type Filter = 'all' | 'recording' | 'screenshot' | 'agent';
  type Sort = 'newest' | 'oldest' | 'longest' | 'name';

  // The app lists real projects; a browser preview shows samples.
  let items = $state<LibraryItem[]>(isTauri || route.query.get('empty') ? [] : [...LIBRARY]);
  let loading = $state(isTauri);
  const pendingTrash = new Map<string, ReturnType<typeof setTimeout>>();

  async function toItem(p: ProjectSummary): Promise<LibraryItem> {
    return {
      id: p.path,
      path: p.path,
      title: p.name,
      app: p.app ?? 'Screen',
      duration: p.duration_us / 1e6,
      created: new Date(p.created_at_ms),
      wallpaper: 'aurora-lime',
      scene: 0,
      agent: p.agent ?? undefined,
      kind: p.kind === 'screenshot' ? 'screenshot' : 'recording',
      sizeMb: Math.max(1, Math.round(p.size_bytes / 1e6)),
      media: p.media ? await mediaUrl(p.media) : undefined,
    };
  }

  async function refresh() {
    try {
      const found = await listProjects(settings.saveFolder);
      items = await Promise.all(found.filter((p) => !pendingTrash.has(p.path)).map(toItem));
      counts.library = items.length;
    } catch (error) {
      toast(`Couldn’t read your projects: ${error instanceof Error ? error.message : String(error)}`, {
        tone: 'danger',
      });
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!isTauri) return;
    void refresh();
    // New recordings land while Home is open; look again whenever the window comes back to the front.
    const onfocus = () => void refresh();
    window.addEventListener('focus', onfocus);
    return () => {
      window.removeEventListener('focus', onfocus);
      // Anything still waiting on Undo goes to the Trash now.
      for (const [path, timer] of pendingTrash) {
        clearTimeout(timer);
        void trashProject(path);
      }
    };
  });
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

  function open(item: LibraryItem, query = '') {
    openWindow(
      'editor',
      (item.path ? editorTarget(item.path) : item.id === 'launch-demo' ? 'sample' : item.id) + query,
    );
  }

  function remove(item: LibraryItem) {
    const at = items.indexOf(item);
    items.splice(at, 1);
    if (selected === item.id) selected = null;
    const trash = env.os === 'windows' ? 'Recycle Bin' : 'Trash';
    // Real projects move to the Trash once the chance to undo has passed.
    const path = item.path;
    if (path) {
      pendingTrash.set(
        path,
        setTimeout(() => {
          pendingTrash.delete(path);
          trashProject(path).catch((error) => {
            items.splice(Math.min(at, items.length), 0, item);
            toast(
              `Couldn’t move “${item.title}” to the ${trash}: ${error instanceof Error ? error.message : error}`,
              {
                tone: 'danger',
              },
            );
          });
        }, 5000),
      );
    }
    toast(`Moved “${item.title}” to the ${trash}`, {
      timeout: 5000,
      action: {
        label: 'Undo',
        run: () => {
          if (path) {
            clearTimeout(pendingTrash.get(path));
            pendingTrash.delete(path);
          }
          items.splice(Math.min(at, items.length), 0, item);
        },
      },
    });
  }

  /** Copies a screenshot, styled, at full resolution. */
  async function copyImage(item: LibraryItem) {
    try {
      await copyScreenshot(item.path ?? '');
      toast('Copied. Paste it anywhere.', { tone: 'success' });
    } catch (error) {
      toast(`Couldn’t copy it: ${error instanceof Error ? error.message : String(error)}`, {
        tone: 'danger',
      });
    }
  }

  /** Saves a screenshot as a PNG, pixel for pixel, in the export folder. */
  async function saveImage(item: LibraryItem) {
    try {
      const outcome = await exportProject({
        id: `library-${Date.now()}`,
        project: item.path ?? '',
        name: item.title,
        width: 0,
        height: 0,
        fps: 1,
        format: 'png',
        bitrate: 0,
        folder: settings.exportFolder || undefined,
      });
      if (outcome.status !== 'done') return;
      toast(`Saved “${outcome.file}”`, {
        tone: 'success',
        action: { label: reveal, run: () => revealInFolder(outcome.path) },
      });
    } catch (error) {
      toast(`Couldn’t save it: ${error instanceof Error ? error.message : String(error)}`, {
        tone: 'danger',
      });
    }
  }

  async function duplicate(item: LibraryItem) {
    if (item.path) {
      try {
        await duplicateProject(item.path);
        await refresh();
        toast(`Duplicated “${item.title}”`);
      } catch (error) {
        toast(`Couldn’t duplicate: ${error instanceof Error ? error.message : String(error)}`, {
          tone: 'danger',
        });
      }
      return;
    }
    const copy = {
      ...item,
      id: `${item.id}-copy-${Date.now()}`,
      title: `${item.title} copy`,
      created: new Date(),
    };
    items.splice(items.indexOf(item) + 1, 0, copy);
    toast(`Duplicated “${item.title}”`);
  }

  /** What ⌘E does to a card: export a video, or save a screenshot as a PNG. */
  function exportItem(item: LibraryItem) {
    if (item.kind === 'screenshot' && item.path) void saveImage(item);
    else open(item, '?export=1');
  }

  function menuFor(item: LibraryItem): MenuItem[] {
    const still = item.kind === 'screenshot' && !!item.path;
    return [
      { label: 'Open', icon: Film, chord: 'return', run: () => open(item) },
      ...(still
        ? [{ label: 'Copy image', icon: Image, chord: 'mod+c', run: () => void copyImage(item) }]
        : []),
      { label: 'Rename', icon: Pencil, run: () => toast('Rename from the editor title for now') },
      { label: 'Duplicate', icon: Copy, chord: 'mod+d', run: () => duplicate(item) },
      { label: reveal, icon: FolderOpen, run: () => revealInFolder(item.path ?? item.id) },
      {
        label: still ? 'Save as PNG' : 'Export…',
        icon: Download,
        chord: 'mod+e',
        run: () => exportItem(item),
      },
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
    } else if (matchesChord(e, 'mod+d', env.os)) {
      e.preventDefault();
      void duplicate(item);
    } else if (matchesChord(e, 'mod+e', env.os)) {
      e.preventDefault();
      exportItem(item);
    } else if (
      matchesChord(e, 'mod+c', env.os) &&
      item.kind === 'screenshot' &&
      item.path &&
      !window.getSelection()?.toString()
    ) {
      e.preventDefault();
      void copyImage(item);
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
    {#if loading}
      <div class="grid h-full place-items-center" role="status" aria-label="Loading your projects">
        <Spinner />
      </div>
    {:else if !items.length}
      <EmptyState
        icon={Film}
        title="Your recordings will live here"
        body="Record your screen, or open the sample project to see what Shotmaxxing does with a recording."
      >
        <Button icon={RecordDot} onclick={() => openWindow('record')}>New recording</Button>
        <Button variant="ghost" onclick={() => openWindow('editor', 'sample')}>Open the sample</Button>
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
          body="Press ⌥⇧4 for a styled screenshot of a screen or a window."
        >
          <Button variant="primary" icon={Camera} onclick={() => openWindow('record', 'screenshot')}
            >Take a screenshot</Button
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
