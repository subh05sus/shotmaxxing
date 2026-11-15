<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    Bot,
    Check,
    CircleAlert,
    Hand,
    Pause,
    Play,
    RotateCcw,
    ShieldCheck,
    Stop,
    X,
  } from '@shotmaxxing/ui/icons';
  import {
    Button,
    CheckDraw,
    Icon,
    IconButton,
    Keycap,
    Spinner,
    confetti,
    fade,
    rise,
  } from '@shotmaxxing/ui';
  import { route } from '../../lib/router.svelte';
  import {
    answerTake,
    closeThisWindow,
    dismissTake,
    editorTarget,
    handBackTake,
    handOff,
    isTauri,
    onTakeCard,
    pauseTake,
    resumeTake,
    stopTake,
    takeOverTake,
    takeCard,
    type TakeCaller,
    type TakeCardState,
  } from '../../lib/native';

  /**
   * The take card. An agent asks here before Shotmaxxing sees the screen for it (once per session) and before every
   * take, and the card names who asked as the system reports it. Then the take plays out step by step, with Stop and
   * Hand back always in reach. In the app it follows the real take through the control API; in the browser preview
   * it plays a sample take (`?state=access` shows the access question).
   */
  type Phase = 'asking' | 'running' | 'paused' | 'handoff' | 'saving' | 'done' | 'stopped';

  // The sample take, for the browser preview.
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
  const live = isTauri && !initial;
  const startPhase: Phase =
    initial === 'asking' || initial === 'handoff' || initial === 'done'
      ? initial
      : initial === 'access'
        ? 'asking'
        : 'running';
  let sampleKind = $state<'access' | 'take'>(initial === 'access' ? 'access' : 'take');
  const SAMPLE_CALLER: TakeCaller = {
    pid: 0,
    program: 'shotmaxxing',
    path: '/Applications/Shotmaxxing.app/Contents/MacOS/shotmaxxing',
    agent: 'claude',
    app: 'Terminal',
    signer: 'Apple',
    checked: true,
  };
  let samplePhase = $state<Phase>(startPhase);
  let sampleCurrent = $state(initial === 'handoff' ? HANDOFF : initial === 'done' ? BEATS.length : 0);
  let timer: ReturnType<typeof setInterval> | undefined;

  function run() {
    clearInterval(timer);
    timer = setInterval(() => {
      if (samplePhase !== 'running') return;
      if (sampleCurrent === HANDOFF) {
        samplePhase = 'handoff';
        return;
      }
      sampleCurrent += 1;
      if (sampleCurrent >= BEATS.length) {
        samplePhase = 'done';
        clearInterval(timer);
      }
    }, 1400);
  }
  if (!live && startPhase === 'running') run();

  function restart() {
    sampleKind = 'take';
    sampleCurrent = 0;
    samplePhase = 'running';
    run();
  }

  // The real take.
  let card = $state<TakeCardState | null>(null);
  let loaded = $state(false);
  let pending = $state(false);
  let problem = $state('');
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    if (!live) return;
    try {
      card = await takeCard();
      unlisten = await onTakeCard((next) => {
        card = next;
        pending = false;
      });
    } finally {
      loaded = true;
    }
  });
  onDestroy(() => {
    clearInterval(timer);
    unlisten?.();
  });
  // Nothing to show (the take was cleared before this window opened): close quietly.
  $effect(() => {
    if (live && loaded && !card) void closeThisWindow();
  });

  /** Runs a button's action once, and says what went wrong if it fails. */
  async function act(action: () => Promise<void>) {
    if (pending) return;
    pending = true;
    problem = '';
    try {
      await action();
    } catch (error) {
      problem = error instanceof Error ? error.message : String(error);
    } finally {
      pending = false;
    }
  }

  const phase = $derived<Phase>(
    !live
      ? samplePhase
      : !card
        ? 'running'
        : card.phase === 'waiting'
          ? 'handoff'
          : card.phase === 'running' && card.beat >= card.beats.length
            ? 'saving'
            : card.phase,
  );
  const beats = $derived(live ? (card?.beats ?? []) : BEATS);
  const current = $derived(live ? (card?.beat ?? 0) : sampleCurrent);
  const client = $derived(live ? card?.client || 'An agent' : 'Claude Code');
  const app = $derived(live ? card?.app || 'the app' : 'Safari');
  const kind = $derived(live ? (card?.kind ?? 'take') : sampleKind);
  const purpose = $derived(live ? card?.purpose || 'see your screen' : 'look at Safari');
  const caller = $derived(live ? (card?.caller ?? null) : SAMPLE_CALLER);

  /** Who asked, from the system rather than the request: the agent's program, its app, and who signed that app. */
  const asker = $derived.by((): { tone: 'safe' | 'plain' | 'warn'; text: string } => {
    if (!caller) return { tone: 'warn', text: 'Shotmaxxing couldn’t tell which program asked' };
    const command = /^shotmaxxing(\.exe)?$/i.test(caller.program);
    const who = command ? (caller.agent ?? 'a program') : caller.program;
    const where = caller.app ? ` in ${caller.app}` : '';
    if (!command) return { tone: 'warn', text: `From ${who}${where}, not the shotmaxxing command` };
    if (!caller.app) return { tone: 'warn', text: `From ${who}, outside any app` };
    if (!caller.checked) return { tone: 'plain', text: `From ${who}${where}` };
    if (!caller.signer) return { tone: 'warn', text: `From ${who}${where}, which isn’t signed` };
    return { tone: 'safe', text: `From ${who}${where} · signed by ${caller.signer}` };
  });
  const result = $derived(live ? (card?.result ?? null) : null);
  const progress = $derived(
    phase === 'done' || phase === 'saving' ? 1 : Math.min(1, current / Math.max(1, beats.length)),
  );

  function duration(seconds: number): string {
    const s = Math.max(1, Math.round(seconds));
    if (s < 60) return `${s} s`;
    return s % 60 ? `${Math.floor(s / 60)} min ${s % 60} s` : `${s / 60} min`;
  }
  const steps = (n: number) => `${n} ${n === 1 ? 'step' : 'steps'}`;

  const title = $derived(
    {
      asking: kind === 'access' ? `${client} wants to see your screen` : `${client} wants to film a take`,
      running: `${client} is filming`,
      paused: 'Paused',
      handoff: 'Your turn',
      saving: 'Saving the take',
      done: 'Take ready',
      stopped: 'Take stopped',
    }[phase],
  );
  const subtitle = $derived.by(() => {
    const n = beats.length;
    if (kind === 'access') return 'For as long as it stays connected';
    switch (phase) {
      case 'asking':
        return `${app} · ${steps(n)} · about ${duration(live ? (card?.seconds ?? 0) : 42)}`;
      case 'saving':
        return `${app} · all ${steps(n)} done`;
      case 'done':
        return live ? `${app} · ${steps(n)}, ${duration(result?.seconds ?? 0)}` : `Launch demo · ${app}`;
      case 'stopped':
        return `${app} · ${result?.beats_done ?? current} of ${steps(n)}`;
      default:
        return `${app} · step ${Math.min(current + 1, n)} of ${n}`;
    }
  });

  async function openInEditor(target: HTMLElement, project: string) {
    const r = target.getBoundingClientRect();
    confetti({ x: r.left + r.width / 2, y: r.top });
    if (live && card) await dismissTake(card.id);
    setTimeout(() => void handOff('editor', editorTarget(project)), 500);
  }

  async function close() {
    if (card) await dismissTake(card.id);
    await closeThisWindow();
  }
</script>

<div class="flex h-full items-end justify-end p-6">
  <section
    class="w-[360px] overflow-hidden rounded-[14px] border border-border bg-elevated text-fg shadow-[var(--elev-3)]"
    aria-label="Agent take"
    in:rise={{ y: 16 }}
  >
    <header class="flex items-center gap-3 px-4 pb-3 pt-3.5">
      <span class="grid size-9 shrink-0 place-items-center rounded-[10px] bg-accent text-accent-fg"
        ><Icon icon={Bot} size={18} aria-hidden="true" /></span
      >
      <div class="min-w-0 flex-1">
        <p class="text-callout font-semibold" aria-live="polite">{title}</p>
        <p class="truncate text-footnote text-fg-muted">{subtitle}</p>
      </div>
      {#if phase === 'running'}
        <span class="size-2.5 shrink-0 rounded-full bg-record" aria-label="Recording"></span>
      {:else if live && (phase === 'done' || phase === 'stopped')}
        <IconButton icon={X} label="Close" size="sm" onclick={() => act(close)} />
      {/if}
    </header>

    <div class="h-1 bg-fill" aria-hidden="true">
      <div
        class="h-full w-full origin-left bg-accent transition-transform duration-[var(--dur-slow)] ease-[var(--ease-out)]"
        style:transform="scaleX({phase === 'asking' ? 0 : progress})"
      ></div>
    </div>

    {#if phase === 'asking'}
      <div class="px-4 pt-3" in:fade>
        <p
          class={[
            'flex items-center gap-1.5 text-caption font-medium',
            asker.tone === 'safe' && 'text-success',
            asker.tone === 'warn' && 'text-warning',
            asker.tone === 'plain' && 'text-fg-muted',
          ]}
          title={caller?.path || undefined}
        >
          {#if asker.tone === 'safe'}
            <Icon icon={ShieldCheck} size={14} class="shrink-0" aria-hidden="true" />
          {:else if asker.tone === 'warn'}
            <Icon icon={CircleAlert} size={14} class="shrink-0" aria-hidden="true" />
          {/if}
          <span class="min-w-0 truncate">{asker.text}</span>
        </p>
        <p class="mt-1.5 text-footnote leading-[1.45] text-fg-muted">
          {#if kind === 'access'}
            Shotmaxxing will {purpose} for it. Until it disconnects, it can also see which windows are open and
            record the screen, with the recording pill showing. Every take still asks you first.
          {:else}
            It will move the pointer, click and type in {app} while Shotmaxxing records. Stop it whenever you like.
          {/if}
        </p>
      </div>
    {:else if phase === 'handoff'}
      <div class="px-4 py-4" in:fade>
        <p class="flex items-center gap-2 text-callout font-semibold">
          <Icon icon={Hand} size={16} class="shrink-0 text-accent-text" aria-hidden="true" />
          <span class="min-w-0">{live ? card?.waiting || 'Over to you' : 'Enter the password yourself'}</span>
        </p>
        <p class="mt-1 text-footnote leading-[1.45] text-fg-muted">
          The agent is waiting and won’t touch anything. This part is cut from the video. Hand back when
          you’re done.
        </p>
      </div>
    {:else if phase === 'paused'}
      <div class="px-4 pt-3" in:fade>
        <p class="text-footnote leading-[1.45] text-fg-muted">
          Nothing moves while the take is paused, and the pause is cut from the video.
        </p>
      </div>
    {:else if phase === 'stopped' && result?.stopped}
      <div class="px-4 pt-3" in:fade>
        <p class="text-footnote leading-[1.45] text-fg-muted">{result.stopped}</p>
      </div>
    {/if}

    {#if kind === 'take'}
      <ol class="scroll-room max-h-60 overflow-y-auto" aria-label="Steps">
        {#each beats as beat, i (i)}
          {@const active = i === current && phase !== 'asking' && phase !== 'done' && phase !== 'saving'}
          <li
            class={[
              'flex items-center gap-2.5 rounded-[7px] px-2 py-1.5 text-footnote',
              active && phase !== 'stopped' && 'bg-fill font-semibold',
            ]}
          >
            <span class="grid size-4 shrink-0 place-items-center">
              {#if phase !== 'asking' && (i < current || phase === 'done' || phase === 'saving')}
                <Icon icon={Check} size={13} class="text-success" aria-label="Done" />
              {:else if active && phase === 'running'}
                <Spinner size={13} label="In progress" />
              {:else if active && phase === 'handoff'}
                <Icon icon={Hand} size={13} class="text-accent-text" aria-label="Waiting for you" />
              {:else if active && phase === 'paused'}
                <Icon icon={Pause} size={13} fill="currentColor" class="text-fg-muted" aria-label="Paused" />
              {:else}
                <span class="size-1.5 rounded-full bg-fg-subtle" aria-hidden="true"></span>
              {/if}
            </span>
            <span class={['min-w-0', phase !== 'asking' && i > current && 'text-fg-muted']}>{beat}</span>
          </li>
        {/each}
      </ol>
    {:else}
      <div class="h-3" aria-hidden="true"></div>
    {/if}

    <footer class="flex items-center gap-2 border-t border-border px-3 py-2.5">
      {#if phase === 'asking'}
        <Button
          size="sm"
          variant="ghost"
          disabled={pending}
          onclick={() =>
            live && card
              ? act(() => answerTake(card!.id, false))
              : kind === 'access'
                ? restart()
                : (samplePhase = 'stopped')}>Don’t allow</Button
        >
        <span class="flex-1"></span>
        <Button
          size="sm"
          variant="primary"
          loading={pending}
          onclick={() => (live && card ? act(() => answerTake(card!.id, true)) : restart())}>Allow</Button
        >
      {:else if phase === 'saving'}
        <Spinner size={14} label="Saving" />
        <span class="flex-1 text-footnote text-fg-muted">Saving the project…</span>
      {:else if phase === 'done'}
        <span class="flex items-center gap-1.5 text-footnote font-semibold text-success"
          ><CheckDraw size={14} />
          {live ? steps(result?.beats_done ?? beats.length) : '7 steps, 42 s'}</span
        >
        <span class="flex-1"></span>
        {#if !live}
          <Button size="sm" variant="ghost" icon={RotateCcw} onclick={restart}>Re-film</Button>
        {/if}
        {#if !live || result?.project}
          <Button
            size="sm"
            variant="primary"
            onclick={(e) => openInEditor(e.currentTarget as HTMLElement, live ? result!.project! : 'sample')}
            >Open in editor</Button
          >
        {/if}
      {:else if phase === 'handoff'}
        <span class="flex-1 text-caption text-fg-subtle">
          {#if live}Or press <Keycap chord="alt+mod+enter" />{:else}Step 4 of 7{/if}
        </span>
        <Button
          size="sm"
          variant="primary"
          loading={pending}
          onclick={() =>
            live && card
              ? act(() => handBackTake(card!.id))
              : ((sampleCurrent = HANDOFF + 1), (samplePhase = 'running'), run())}>Hand back</Button
        >
      {:else if phase === 'stopped'}
        <span class="flex-1 text-footnote text-fg-muted">
          {#if !live}Kept the first {sampleCurrent} steps as a project.{:else if result?.project}The steps so
            far are saved as a project.{/if}
        </span>
        {#if live && result?.project}
          <Button size="sm" onclick={(e) => openInEditor(e.currentTarget as HTMLElement, result!.project!)}
            >Open in editor</Button
          >
        {:else if live}
          <Button size="sm" onclick={() => act(close)}>Close</Button>
        {:else}
          <Button size="sm" onclick={restart}>Start over</Button>
        {/if}
      {:else}
        {#if live && card?.hold && phase === 'running'}
          <span class="flex min-w-0 items-center gap-2 text-footnote text-fg-muted">
            <Spinner size={13} label="Waiting for the step to finish" />
            {card.hold === 'pause' ? 'Pausing after this step…' : 'Handing over after this step…'}
          </span>
        {:else}
          <Button
            size="sm"
            icon={phase === 'paused' ? Play : Pause}
            disabled={pending}
            onclick={() =>
              live && card
                ? act(() => (phase === 'paused' ? resumeTake(card!.id) : pauseTake(card!.id)))
                : (samplePhase = samplePhase === 'paused' ? 'running' : 'paused')}
          >
            {phase === 'paused' ? 'Resume' : 'Pause'}
          </Button>
          <Button
            size="sm"
            variant="ghost"
            icon={Hand}
            disabled={pending}
            onclick={() => (live && card ? act(() => takeOverTake(card!.id)) : (samplePhase = 'handoff'))}
            >Take over</Button
          >
        {/if}
        <span class="flex-1"></span>
        <Button
          size="sm"
          variant="ghost"
          icon={Stop}
          loading={pending}
          onclick={() => {
            if (live && card) act(() => stopTake(card!.id));
            else {
              samplePhase = 'stopped';
              clearInterval(timer);
            }
          }}>Stop</Button
        >
      {/if}
    </footer>
    {#if problem}
      <p class="border-t border-border px-4 py-2 text-caption text-danger" role="alert">{problem}</p>
    {/if}
    {#if phase === 'running' || phase === 'paused'}
      <p class="flex items-center gap-1.5 border-t border-border px-4 py-2 text-caption text-fg-subtle">
        Press <Keycap chord="escape" /> three times to stop from anywhere.
      </p>
    {/if}
  </section>
</div>
