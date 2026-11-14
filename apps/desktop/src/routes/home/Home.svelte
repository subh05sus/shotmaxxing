<script lang="ts">
  import { BookOpen, Bot, Circle, Film, Search, Settings as SettingsIcon } from '@lucide/svelte';
  import { Button, Keycap, StatusDot, Tooltip, env, fade } from '@shotmaxxing/ui';
  import { go, route } from '../../lib/router.svelte';
  import { openWindow } from '../../lib/native';
  import { ui } from '../../lib/commands.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { AGENTS, LIBRARY } from '../../lib/sample';
  import Library from './Library.svelte';
  import Learn from './Learn.svelte';
  import Agents from './Agents.svelte';
  import Settings from './Settings.svelte';

  const page = $derived(route.path[1] ?? 'library');
  const connectedAgents = $derived(AGENTS.filter((a) => a.state === 'connected').length);

  const NAV = $derived([
    { id: 'library', label: 'Library', icon: Film, count: LIBRARY.length },
    { id: 'learn', label: 'Learn', icon: BookOpen, count: undefined },
    { id: 'agents', label: 'Agents', icon: Bot, count: undefined },
    { id: 'settings', label: 'Settings', icon: SettingsIcon, count: undefined },
  ]);

  const titles: Record<string, string> = {
    library: 'Library',
    learn: 'Learn',
    agents: 'Agents',
    settings: 'Settings',
  };
  const lowPower = $derived(env.lowPower);
</script>

<div class="flex h-full">
  <aside class="flex w-[212px] shrink-0 flex-col border-r border-border bg-chrome">
    <div class="drag-region h-[var(--titlebar-height)] shrink-0"></div>
    <nav class="flex flex-col gap-0.5 px-2.5" aria-label="Sections">
      {#each NAV as item (item.id)}
        {@const Icon = item.icon}
        <button
          type="button"
          aria-current={page === item.id ? 'page' : undefined}
          onclick={() => go(`home/${item.id}`)}
          class={[
            'ring-inside flex h-8 items-center gap-2.5 rounded-[7px] px-2.5 text-callout transition-colors duration-[var(--dur-fast)]',
            page === item.id
              ? 'bg-accent-tint font-semibold text-fg'
              : 'text-fg-muted hover:bg-fill hover:text-fg',
          ]}
        >
          <Icon
            size={16}
            strokeWidth={2}
            class={page === item.id ? 'text-accent-text' : ''}
            aria-hidden="true"
          />
          <span class="flex-1 text-left">{item.label}</span>
          {#if item.id === 'agents' && connectedAgents}
            <StatusDot tone="success" />
          {:else if item.count}
            <span class="text-footnote text-fg-subtle tabular">{item.count}</span>
          {/if}
        </button>
      {/each}
    </nav>
    <span class="flex-1"></span>
    <div class="m-2.5 rounded-[9px] bg-fill px-3 py-2.5">
      <StatusDot tone={lowPower ? 'warning' : 'success'} label={lowPower ? 'Low power on' : 'Running cool'} />
      <p class="mt-1 text-caption text-fg-subtle">
        {lowPower
          ? 'Previews run at 30 fps with simple fades.'
          : 'Nothing is drawing while you aren’t looking.'}
      </p>
    </div>
  </aside>

  <main class="flex min-w-0 flex-1 flex-col bg-bg">
    <header class="drag-region flex h-[var(--titlebar-height)] shrink-0 items-center gap-3 px-5">
      <h1 class="text-title-3">{titles[page] ?? 'Library'}</h1>
      <span class="flex-1"></span>
      <Tooltip label="Search everything" chord="mod+k">
        <button
          type="button"
          class="no-drag press flex h-7 w-56 items-center gap-2 rounded-control border border-border bg-fill px-2.5 text-callout text-fg-subtle hover:bg-fill-hover"
          onclick={() => (ui.palette = true)}
          data-tour="palette"
        >
          <Search size={14} aria-hidden="true" />
          <span class="flex-1 text-left">Search</span>
          <Keycap chord="mod+k" />
        </button>
      </Tooltip>
      <span class="no-drag">
        <Tooltip label="New recording" chord="alt+shift+5">
          <Button variant="record" icon={Circle} onclick={() => openWindow('record')}>Record</Button>
        </Tooltip>
      </span>
    </header>

    <div class="grid min-h-0 flex-1">
      {#key page}
        <div class="col-start-1 row-start-1 min-h-0" in:fade={{ duration: 160 }}>
          {#if page === 'learn'}
            <Learn />
          {:else if page === 'agents'}
            <Agents />
          {:else if page === 'settings'}
            <Settings />
          {:else}
            <Library />
          {/if}
        </div>
      {/key}
    </div>
  </main>
</div>

{#if !settings.onboardingDone && page === 'library'}
  <!-- Resumable setup: a quiet reminder, not a blocker. -->
  <div
    class="fixed bottom-5 left-[236px] z-[600] flex items-center gap-3 rounded-[10px] border border-border bg-elevated py-2 pl-3 pr-2 text-callout shadow-[var(--elev-2)]"
  >
    Setup isn’t finished yet.
    <Button size="sm" variant="primary" onclick={() => go('onboarding')}>Continue setup</Button>
  </div>
{/if}
