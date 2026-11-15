<script lang="ts">
  import { Bot, Check, Download, MonitorDot, Play, Scissors } from '@shotmaxxing/ui/icons';
  import { Button, CheckDraw, Icon, Sheet, type IconData } from '@shotmaxxing/ui';
  import Scene from '../../components/Scene.svelte';
  import { GUIDES, type Guide } from '../../lib/sample';
  import { settings } from '../../lib/settings.svelte';
  import { openWindow } from '../../lib/native';
  import { go } from '../../lib/router.svelte';
  import { wallpaper } from '../../lib/wallpapers';

  const GROUPS: { id: Guide['group']; icon: IconData; blurb: string }[] = [
    { id: 'Record', icon: MonitorDot, blurb: 'Film a window, a screen or yourself.' },
    { id: 'Edit', icon: Scissors, blurb: 'Zooms, cursor, voice-overs, captions, trims and privacy.' },
    { id: 'Export', icon: Download, blurb: 'The right file for where it’s going.' },
    { id: 'Use with AI', icon: Bot, blurb: 'Let an agent film the demo for you.' },
  ];

  let open = $state<Guide | null>(null);
  let sheetOpen = $state(false);
  let heroWidth = $state(0);

  const done = $derived(settings.completedGuides.length);
  const progress = $derived(done / GUIDES.length);

  function show(guide: Guide) {
    open = guide;
    sheetOpen = true;
  }

  function toggleDone(guide: Guide) {
    const list = settings.completedGuides;
    settings.completedGuides = list.includes(guide.id)
      ? list.filter((id) => id !== guide.id)
      : [...list, guide.id];
  }
</script>

<div class="scroll-room h-full overflow-y-auto px-5 pb-8">
  <section
    class="rise-in flex items-stretch gap-5 rounded-card border border-border bg-surface p-4 shadow-[var(--elev-1)]"
  >
    <div
      bind:clientWidth={heroWidth}
      class="relative aspect-[16/10] w-[300px] shrink-0 overflow-hidden rounded-[8px]"
      style:background={wallpaper('aurora-lime').css}
    >
      <div
        class="absolute left-[7%] top-[8%] aspect-[16/10] w-[86%] overflow-hidden rounded-[5px] shadow-[var(--elev-2)]"
      >
        {#if heroWidth}<Scene variant={0} width={heroWidth * 0.86} t={10.5} />{/if}
      </div>
    </div>
    <div class="flex min-w-0 flex-1 flex-col justify-center">
      <p class="eyebrow">Start here</p>
      <h2 class="mt-1 text-title-2">Explore the sample project</h2>
      <p class="measure mt-1.5 text-callout text-fg-muted">
        A 42-second sign-up demo filmed by an agent. Open it and a short tour shows you every tool, one at a
        time.
      </p>
      <div class="mt-4 flex items-center gap-3">
        <Button variant="primary" icon={Play} onclick={() => go('editor/sample?tour=1')}
          >Open with tour</Button
        >
        <Button variant="ghost" onclick={() => openWindow('record')}>Or record your own</Button>
      </div>
    </div>
  </section>

  <div class="mt-6 flex items-center gap-3">
    <h2 class="text-title-3">Guides</h2>
    <span class="text-footnote text-fg-muted tabular">{done} of {GUIDES.length} done</span>
    <span class="h-1.5 w-32 overflow-hidden rounded-full bg-fill" aria-hidden="true">
      <span
        class="block h-full w-full origin-left rounded-full bg-accent transition-transform duration-[var(--dur-slow)] ease-[var(--ease-spring)]"
        style:transform="scaleX({progress})"
      ></span>
    </span>
  </div>

  {#each GROUPS as group, g (group.id)}
    <section class="mt-5">
      <h3 class="flex items-center gap-2 text-callout font-semibold">
        <Icon icon={group.icon} size={15} class="text-fg-muted" aria-hidden="true" />
        {group.id}
        <span class="font-normal text-fg-muted">· {group.blurb}</span>
      </h3>
      <div class="mt-2 grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-2.5">
        {#each GUIDES.filter((guide) => guide.group === group.id) as guide, i (guide.id)}
          {@const isDone = settings.completedGuides.includes(guide.id)}
          <button
            type="button"
            class="press-card rise-in flex flex-col rounded-card border border-border bg-surface p-3.5 text-left shadow-[var(--elev-1)] hover:border-border-strong"
            style="--i: {g * 2 + i}"
            onclick={() => show(guide)}
          >
            <span class="flex items-start gap-2">
              <span class="flex-1 text-callout font-semibold">{guide.title}</span>
              {#if isDone}
                <span
                  class="grid size-5 place-items-center rounded-full bg-accent text-accent-fg"
                  aria-label="Done"><Icon icon={Check} size={12} /></span
                >
              {/if}
            </span>
            <span class="mt-1 line-clamp-2 text-footnote text-fg-muted">{guide.summary}</span>
            <span class="mt-2.5 text-caption font-medium text-fg-subtle tabular">{guide.minutes} min</span>
          </button>
        {/each}
      </div>
    </section>
  {/each}
</div>

{#if open}
  {@const guide = open}
  {@const isDone = settings.completedGuides.includes(guide.id)}
  <Sheet bind:open={sheetOpen} title={guide.title} description={guide.summary} width={520}>
    <ol class="flex flex-col gap-3">
      {#each guide.steps as text, i (i)}
        <li class="flex gap-3">
          <span
            class="grid size-6 shrink-0 place-items-center rounded-full bg-fill text-footnote font-semibold tabular"
            >{i + 1}</span
          >
          <span class="pt-0.5 text-callout leading-[1.45]">{text}</span>
        </li>
      {/each}
    </ol>
    {#snippet footer()}
      <Button variant="ghost" onclick={() => toggleDone(guide)}>
        {#if isDone}<CheckDraw size={14} class="text-success" /> Done{:else}Mark as done{/if}
      </Button>
      {#if guide.tool}
        <Button
          variant="primary"
          onclick={() => {
            sheetOpen = false;
            go(`editor/sample?tool=${guide.tool}&tour=${guide.tool}`);
          }}>Show me in the editor</Button
        >
      {:else if guide.id === 'screenshot'}
        <Button
          variant="primary"
          onclick={() => {
            sheetOpen = false;
            void openWindow('record', 'screenshot');
          }}>Take a screenshot</Button
        >
      {:else}
        <Button variant="primary" onclick={() => (sheetOpen = false)}>Got it</Button>
      {/if}
    {/snippet}
  </Sheet>
{/if}
