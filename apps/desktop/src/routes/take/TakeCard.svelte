<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Bot, Check, Hand, Pause, Play, RotateCcw, Square } from '@lucide/svelte';
  import { Button, CheckDraw, Keycap, Spinner, confetti, fade, rise } from '@shotmaxxing/ui';
  import { route } from '../../lib/router.svelte';
  import { handOff } from '../../lib/native';

  /**
   * The live take card: what the agent is doing, step by step, with Pause, Take over and Stop always in reach.
   * In the app it is fed by the control API; in the preview it plays the sample take.
   */
  type Phase = 'running' | 'paused' | 'handoff' | 'done' | 'stopped';

  const BEATS = [
    'Open northwind.app/signup in Safari',
    'Click Get started',
    'Type a work email',
    'Your turn: enter a password',
    'Click Create account',
    'Wait for the confirmation',
    'Point at Pricing',
  ];
  const HANDOFF = 3;

  const initial = route.query.get('state');
  let phase = $state<Phase>(initial === 'handoff' ? 'handoff' : initial === 'done' ? 'done' : 'running');
  let current = $state(initial === 'handoff' ? HANDOFF : initial === 'done' ? BEATS.length : 0);
  let timer: ReturnType<typeof setInterval> | undefined;

  function run() {
    clearInterval(timer);
    timer = setInterval(() => {
      if (phase !== 'running') return;
      if (current === HANDOFF) {
        phase = 'handoff';
        return;
      }
      current += 1;
      if (current >= BEATS.length) {
        phase = 'done';
        clearInterval(timer);
      }
    }, 1400);
  }
  if (initial !== 'handoff' && initial !== 'done') run();
  onDestroy(() => clearInterval(timer));

  function handBack() {
    current = HANDOFF + 1;
    phase = 'running';
    run();
  }

  function restart() {
    current = 0;
    phase = 'running';
    run();
  }

  const progress = $derived(Math.min(1, current / BEATS.length));
</script>

<div class="flex h-full items-end justify-end p-6">
  <section
    class="w-[360px] overflow-hidden rounded-[14px] border border-border bg-elevated text-fg shadow-[var(--elev-3)]"
    aria-label="Agent take"
    in:rise={{ y: 16 }}
  >
    <header class="flex items-center gap-3 px-4 pb-3 pt-3.5">
      <span class="grid size-9 place-items-center rounded-[10px] bg-accent text-accent-fg"
        ><Bot size={18} aria-hidden="true" /></span
      >
      <div class="min-w-0 flex-1">
        <p class="text-callout font-semibold">
          {#if phase === 'done'}Take ready{:else if phase === 'handoff'}Your turn{:else if phase === 'paused'}Paused{:else if phase === 'stopped'}Take
            stopped{:else}Claude Code is filming{/if}
        </p>
        <p class="truncate text-footnote text-fg-muted">
          Launch demo · Safari · step {Math.min(current + 1, BEATS.length)} of {BEATS.length}
        </p>
      </div>
      {#if phase === 'running'}<span class="size-2.5 rounded-full bg-record" aria-label="Recording"
        ></span>{/if}
    </header>

    <div class="h-1 bg-fill" aria-hidden="true">
      <div
        class="h-full w-full origin-left bg-accent transition-transform duration-[var(--dur-slow)] ease-[var(--ease-out)]"
        style:transform="scaleX({progress})"
      ></div>
    </div>

    {#if phase === 'handoff'}
      <div class="px-4 py-4" in:fade>
        <p class="flex items-center gap-2 text-callout font-semibold">
          <Hand size={16} class="text-accent-text" aria-hidden="true" /> Enter the password yourself
        </p>
        <p class="mt-1 text-footnote leading-[1.45] text-fg-muted">
          Agents never type passwords. Click the Password field, type it, then hand back. The agent is waiting
          and won’t touch anything.
        </p>
      </div>
    {/if}

    <ol class="scroll-room max-h-60 overflow-y-auto" aria-label="Steps">
      {#each BEATS as beat, i (beat)}
        <li
          class={[
            'flex items-center gap-2.5 rounded-[7px] px-2 py-1.5 text-footnote',
            i === current && phase !== 'done' && 'bg-fill font-semibold',
          ]}
        >
          <span class="grid size-4 shrink-0 place-items-center">
            {#if i < current}
              <Check size={13} strokeWidth={3} class="text-success" aria-label="Done" />
            {:else if i === current && phase === 'running'}
              <Spinner size={13} label="In progress" />
            {:else if i === current && phase === 'handoff'}
              <Hand size={13} class="text-accent-text" aria-label="Waiting for you" />
            {:else}
              <span class="size-1.5 rounded-full bg-fg-subtle" aria-hidden="true"></span>
            {/if}
          </span>
          <span class={i > current ? 'text-fg-muted' : ''}>{beat}</span>
        </li>
      {/each}
    </ol>

    <footer class="flex items-center gap-2 border-t border-border px-3 py-2.5">
      {#if phase === 'done'}
        <span class="flex items-center gap-1.5 text-footnote font-semibold text-success"
          ><CheckDraw size={14} /> 7 steps, 42 s</span
        >
        <span class="flex-1"></span>
        <Button size="sm" variant="ghost" icon={RotateCcw} onclick={restart}>Re-film</Button>
        <Button
          size="sm"
          variant="primary"
          onclick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            confetti({ x: r.left + r.width / 2, y: r.top });
            setTimeout(() => handOff('editor', 'sample'), 500);
          }}>Open in editor</Button
        >
      {:else if phase === 'handoff'}
        <span class="flex-1 text-caption text-fg-subtle">Step 4 of 7</span>
        <Button size="sm" variant="primary" onclick={handBack}>Hand back</Button>
      {:else if phase === 'stopped'}
        <span class="flex-1 text-footnote text-fg-muted">Kept the first {current} steps as a project.</span>
        <Button size="sm" onclick={restart}>Start over</Button>
      {:else}
        <Button
          size="sm"
          icon={phase === 'paused' ? Play : Pause}
          onclick={() => (phase = phase === 'paused' ? 'running' : 'paused')}
        >
          {phase === 'paused' ? 'Resume' : 'Pause'}
        </Button>
        <Button size="sm" variant="ghost" icon={Hand} onclick={() => (phase = 'handoff')}>Take over</Button>
        <span class="flex-1"></span>
        <Button
          size="sm"
          variant="ghost"
          icon={Square}
          onclick={() => {
            phase = 'stopped';
            clearInterval(timer);
          }}>Stop</Button
        >
      {/if}
    </footer>
    {#if phase === 'running' || phase === 'paused'}
      <p class="flex items-center gap-1.5 border-t border-border px-4 py-2 text-caption text-fg-subtle">
        Press <Keycap chord="escape" /> three times to stop from anywhere.
      </p>
    {/if}
  </section>
</div>
