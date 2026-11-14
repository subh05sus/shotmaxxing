<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    AudioLines,
    ChevronLeft,
    Circle,
    Download,
    History,
    Image,
    MousePointer2,
    Pencil,
    Redo2,
    Scissors,
    Search,
    Sparkles,
    Undo2,
    Video,
    ZoomIn,
  } from '@lucide/svelte';
  import {
    Button,
    CheckDraw,
    CoachMarks,
    IconButton,
    Keycap,
    ProgressRing,
    Tooltip,
    confetti,
    env,
    fade,
    matchesChord,
    toast,
    type CoachStep,
  } from '@shotmaxxing/ui';
  import { go, route } from '../../lib/router.svelte';
  import { registerCommands, ui } from '../../lib/commands.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { openWindow, revealInFolder } from '../../lib/native';
  import { relativeTime } from '../../lib/format';
  import { EditorState, type ToolId } from './store.svelte';
  import Canvas from './Canvas.svelte';
  import Transport from './Transport.svelte';
  import Timeline from './Timeline.svelte';
  import ToolPanel from './ToolPanel.svelte';
  import ExportSheet from './ExportSheet.svelte';
  import HistorySheet from './HistorySheet.svelte';

  const ed = new EditorState();
  onDestroy(() => ed.destroy());

  const TOOLS: { id: ToolId; label: string; icon: typeof Image; chord: string }[] = [
    { id: 'background', label: 'Background', icon: Image, chord: '1' },
    { id: 'cursor', label: 'Cursor', icon: MousePointer2, chord: '2' },
    { id: 'zoom', label: 'Zoom', icon: ZoomIn, chord: '3' },
    { id: 'annotate', label: 'Annotate', icon: Pencil, chord: '4' },
    { id: 'camera', label: 'Camera', icon: Video, chord: '5' },
    { id: 'audio', label: 'Audio', icon: AudioLines, chord: '6' },
    { id: 'ai', label: 'AI', icon: Sparkles, chord: '7' },
  ];

  const requestedTool = route.query.get('tool') as ToolId | null;
  if (requestedTool && TOOLS.some((t) => t.id === requestedTool)) ed.tool = requestedTool;

  let exportOpen = $state(route.query.get('export') === '1');
  let historyOpen = $state(false);
  let editingTitle = $state(false);
  let tourOpen = $state(false);
  let tourSteps = $state<CoachStep[]>([]);

  const TOUR: CoachStep[] = [
    {
      target: '[data-tour="rail"]',
      title: 'Your tools',
      body: 'Each tool changes how the video looks. Nothing is baked in, so you can change anything later.',
    },
    {
      target: '[data-tour="canvas"]',
      title: 'This is your video',
      body: 'What you see here is exactly what exports. It updates as you change things.',
    },
    {
      target: '[data-tour="zoom-lane"]',
      title: 'Zooms explain themselves',
      body: 'Auto zoom added these from your clicks and typing. Hover one to see why. Drag its edges to adjust it.',
      chord: 'z',
    },
    {
      target: '[data-tour="transport"]',
      title: 'Play it back',
      body: 'Space plays and pauses. J, K and L work too.',
      chord: 'space',
    },
    {
      target: '[data-tour="export"]',
      title: 'Export when it looks right',
      body: 'Pick where it’s going and keep editing while it exports.',
      chord: 'mod+e',
    },
    {
      target: '[data-tour="search"]',
      title: 'Find anything',
      body: 'Every tool, setting and guide is one search away.',
      chord: 'mod+k',
    },
  ];

  const TOOL_HELP: Record<ToolId, string> = {
    background:
      'Pick a wallpaper, then set padding, corners and shadow. Aspect ratio changes the shape of the whole video.',
    cursor:
      'Size and smoothing make the cursor easy to follow. Snap to clicks makes near misses land on the button.',
    zoom: 'Auto zoom reads clicks, typing and what changed on screen. Every zoom says why it exists; change any of them on the timeline.',
    annotate:
      'Add a title, an arrow or a blur at the playhead. Blur hides emails, tokens and names before you share.',
    camera: 'Move and shape your camera bubble. It can shrink during zooms so it never covers the action.',
    audio:
      'Set levels for your voice and the computer’s sound. Cleanup and silence cutting run on this computer.',
    ai: 'Captions, voice cleanup and silence cutting run locally with open models. Nothing leaves this computer.',
  };

  function startTour(from = 0) {
    tourSteps = TOUR;
    queueMicrotask(() => {
      ui.tourStart = from;
      tourOpen = true;
    });
  }

  function toolHelp(tool: ToolId) {
    ed.tool = tool;
    tourSteps = [
      {
        target: '[data-tour="panel"]',
        title: TOOLS.find((t) => t.id === tool)?.label ?? 'Tool',
        body: TOOL_HELP[tool],
      },
    ];
    queueMicrotask(() => {
      ui.tourStart = 0;
      tourOpen = true;
    });
  }

  // First visit (or ?tour=1) starts the tour once the layout has settled; ?tour=<tool> explains one tool.
  const tourParam = route.query.get('tour');
  $effect(() => {
    const id = setTimeout(() => {
      if (tourParam && tourParam !== '1' && TOOLS.some((t) => t.id === tourParam))
        toolHelp(tourParam as ToolId);
      else if (tourParam === '1' || !settings.tourDone) startTour();
    }, 450);
    return () => clearTimeout(id);
  });

  function runExport(name: string) {
    const job = ed.startExport(name);
    const started = performance.now();
    const duration = 3600;
    const timer = setInterval(() => {
      job.progress = Math.min(1, (performance.now() - started) / duration);
      if (job.progress >= 1) {
        clearInterval(timer);
        job.done = true;
        toast(`Exported “${name}”`, {
          tone: 'success',
          action: {
            label: env.os === 'mac' ? 'Show in Finder' : 'Show in folder',
            run: () => revealInFolder(name),
          },
        });
        if (!settings.firstExportDone) {
          settings.firstExportDone = true;
          const button = document.querySelector('[data-tour="export"]')?.getBoundingClientRect();
          confetti(button ? { x: button.left + button.width / 2, y: button.bottom } : undefined);
        }
        setTimeout(() => (ed.exports = ed.exports.filter((x) => x.id !== job.id)), 4000);
      }
    }, 100);
  }

  $effect(() =>
    registerCommands('editor', [
      {
        id: 'ed-export',
        title: 'Export…',
        group: 'Project',
        icon: Download,
        chord: 'mod+e',
        run: () => (exportOpen = true),
      },
      {
        id: 'ed-history',
        title: 'Version history',
        group: 'Project',
        icon: History,
        chord: 'mod+alt+h',
        run: () => (historyOpen = true),
      },
      {
        id: 'ed-tour',
        title: 'Start the editor tour',
        group: 'Help',
        icon: Sparkles,
        chord: 'mod+shift+t',
        run: () => startTour(),
      },
      {
        id: 'ed-split',
        title: 'Split at playhead',
        group: 'Edit',
        icon: Scissors,
        chord: 's',
        run: () => ed.split(),
      },
      {
        id: 'ed-zoom',
        title: 'Add zoom at playhead',
        group: 'Edit',
        icon: ZoomIn,
        chord: 'z',
        run: () => ed.addZoom(),
      },
      {
        id: 'ed-redo-zoom',
        title: 'Redo auto zoom',
        group: 'Edit',
        icon: ZoomIn,
        keywords: 'auto zoom again replan',
        run: () => ed.redoAutoZoom(),
      },
      {
        id: 'ed-captions',
        title: 'Generate captions',
        group: 'AI',
        icon: Sparkles,
        keywords: 'subtitles whisper transcribe',
        run: () => (ed.tool = 'ai'),
      },
      ...TOOLS.map((t) => ({
        id: `tool-${t.id}`,
        title: `${t.label} tool`,
        group: 'Tools',
        icon: t.icon,
        chord: t.chord,
        run: () => (ed.tool = t.id),
      })),
      {
        id: 'ed-new',
        title: 'New recording',
        group: 'Record',
        icon: Circle,
        run: () => openWindow('record'),
      },
    ]),
  );

  function onkeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable
    )
      return;
    if (tourOpen || exportOpen || historyOpen || ui.palette || ui.shortcuts) return;
    const os = env.os;
    if (matchesChord(e, 'mod+z', os)) (e.preventDefault(), ed.undo());
    else if (matchesChord(e, 'mod+shift+z', os) || (os === 'windows' && matchesChord(e, 'ctrl+y', os)))
      (e.preventDefault(), ed.redo());
    else if (matchesChord(e, 'mod+e', os)) (e.preventDefault(), (exportOpen = true));
    else if (matchesChord(e, 'mod+alt+h', os)) (e.preventDefault(), (historyOpen = true));
    else if (matchesChord(e, 'mod+shift+t', os)) (e.preventDefault(), startTour());
    else if (e.metaKey || e.ctrlKey || e.altKey) return;
    else if (e.key === ' ') (e.preventDefault(), ed.toggle());
    else if (e.key === 'k' || e.key === 'K') ed.pause();
    else if (e.key === 'l' || e.key === 'L') {
      ed.rate = ed.playing && ed.rate > 0 ? Math.min(4, ed.rate * 2) : 1;
      ed.play();
    } else if (e.key === 'j' || e.key === 'J') (ed.pause(), ed.seek(ed.time - 5));
    else if (e.key === 'ArrowLeft') (e.preventDefault(), ed.seek(ed.time - (e.shiftKey ? 5 : 1 / 60)));
    else if (e.key === 'ArrowRight') (e.preventDefault(), ed.seek(ed.time + (e.shiftKey ? 5 : 1 / 60)));
    else if (e.key === 'Home') ed.seek(0);
    else if (e.key === 'End') ed.seek(ed.project.duration);
    else if (e.key === 's' || e.key === 'S') ed.split();
    else if (e.key === 'z' || e.key === 'Z') ed.addZoom();
    else if (e.key === 'Backspace' || e.key === 'Delete') {
      const label = ed.deleteSelection();
      if (label) toast(label, { action: { label: 'Undo', run: () => ed.undo() } });
    } else if (e.key === 'Escape') ed.selection = null;
    else if (/^[1-7]$/.test(e.key)) ed.tool = TOOLS[Number(e.key) - 1].id;
  }

  const activeExport = $derived(ed.exports.find((x) => !x.done) ?? ed.exports[0]);
</script>

<svelte:window {onkeydown} />

<div class="flex h-full flex-col bg-bg">
  <header
    class="drag-region flex h-[var(--titlebar-height)] shrink-0 items-center gap-2 border-b border-border bg-chrome pl-[var(--titlebar-inset-start)] pr-3"
  >
    <span class="no-drag"
      ><IconButton icon={ChevronLeft} label="Library" size="sm" onclick={() => go('home/library')} /></span
    >
    {#if editingTitle}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="no-drag h-7 w-64 rounded-[6px] border border-ring bg-surface px-2 text-callout font-semibold outline-none"
        value={ed.project.title}
        autofocus
        aria-label="Project name"
        onblur={(e) => {
          const value = e.currentTarget.value.trim();
          if (value && value !== ed.project.title) ed.change('Rename', () => (ed.project.title = value));
          editingTitle = false;
        }}
        onkeydown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') editingTitle = false;
        }}
      />
    {:else}
      <button
        type="button"
        class="no-drag press rounded-[6px] px-1.5 py-0.5 text-callout font-semibold hover:bg-fill"
        title="Rename"
        onclick={() => (editingTitle = true)}
      >
        {ed.project.title}
      </button>
    {/if}
    <span class="flex w-24 items-center gap-1 text-footnote text-fg-subtle" aria-live="polite">
      {#if ed.saving}
        <span in:fade={{ duration: 100 }}>Saving…</span>
      {:else}
        <span
          class="flex items-center gap-1"
          in:fade={{ duration: 100 }}
          title="Saved {relativeTime(new Date(ed.savedAt))}"
        >
          <CheckDraw size={12} /> Saved
        </span>
      {/if}
    </span>
    <span class="flex-1"></span>
    <div class="no-drag flex items-center gap-1">
      <IconButton
        icon={Undo2}
        label="Undo"
        chord="mod+z"
        size="sm"
        disabled={!ed.canUndo}
        onclick={() => ed.undo()}
      />
      <IconButton
        icon={Redo2}
        label="Redo"
        chord="mod+shift+z"
        size="sm"
        disabled={!ed.canRedo}
        onclick={() => ed.redo()}
      />
      <IconButton
        icon={History}
        label="Version history"
        chord="mod+alt+h"
        size="sm"
        onclick={() => (historyOpen = true)}
      />
      <span data-tour="search"
        ><IconButton
          icon={Search}
          label="Search tools and settings"
          chord="mod+k"
          size="sm"
          onclick={() => (ui.palette = true)}
        /></span
      >
      {#if activeExport}
        <span
          class="ml-1 flex h-7 items-center gap-1.5 rounded-full bg-fill px-2.5 text-footnote font-medium tabular"
          in:fade={{ duration: 140 }}
          aria-live="polite"
        >
          {#if activeExport.done}
            <CheckDraw size={13} class="text-success" /> Exported
          {:else}
            <ProgressRing value={activeExport.progress} size={14} label="Export progress" /> Exporting {Math.round(
              activeExport.progress * 100,
            )}%
          {/if}
        </span>
      {/if}
      <span class="ml-1" data-tour="export">
        <Tooltip label="Export" chord="mod+e">
          <Button variant="primary" icon={Download} onclick={() => (exportOpen = true)}>Export</Button>
        </Tooltip>
      </span>
    </div>
  </header>

  <div class="flex min-h-0 flex-1">
    <nav
      class="flex w-[68px] shrink-0 flex-col items-center gap-1 border-r border-border bg-chrome py-2"
      aria-label="Tools"
      data-tour="rail"
    >
      {#each TOOLS as tool (tool.id)}
        {@const Icon = tool.icon}
        <Tooltip label={tool.label} chord={tool.chord} side="right">
          <button
            type="button"
            aria-pressed={ed.tool === tool.id}
            class={[
              'press-icon flex w-14 flex-col items-center gap-1 rounded-[9px] py-2 text-caption font-medium transition-colors duration-[var(--dur-fast)]',
              ed.tool === tool.id
                ? 'bg-accent-tint text-accent-text'
                : 'text-fg-muted hover:bg-fill hover:text-fg',
            ]}
            onclick={() => (ed.tool = tool.id)}
          >
            <Icon size={19} strokeWidth={1.9} aria-hidden="true" />
            {tool.label}
          </button>
        </Tooltip>
      {/each}
      <span class="flex-1"></span>
      <Tooltip label="Keyboard shortcuts" chord="mod+/" side="right">
        <button
          type="button"
          class="press-icon grid size-9 place-items-center rounded-[9px] text-fg-subtle hover:bg-fill hover:text-fg"
          aria-label="Keyboard shortcuts"
          onclick={() => (ui.shortcuts = true)}
        >
          <Keycap chord="mod+/" />
        </button>
      </Tooltip>
    </nav>

    <ToolPanel {ed} onhelp={toolHelp} />

    <div class="flex min-w-0 flex-1 flex-col">
      <Canvas {ed} />
      <Transport {ed} />
    </div>
  </div>

  <Timeline {ed} />
</div>

<ExportSheet bind:open={exportOpen} {ed} onexport={runExport} />
<HistorySheet bind:open={historyOpen} {ed} />
<CoachMarks
  steps={tourSteps}
  bind:open={tourOpen}
  start={ui.tourStart}
  onfinish={() => {
    if (tourSteps.length > 1) settings.tourDone = true;
  }}
/>
