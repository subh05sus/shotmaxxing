<script lang="ts">
  /**
   * Setup, as full-window scenes: Welcome, Permissions, Connect AI, Practice and Done. Each scene is big type and one
   * action on the left and what it's about on the right. Moving between them pans the view while a soft lime glow
   * drifts to a new spot, then everything is still. The welcome waits behind the logo building itself
   * (components/IrisOverlay.svelte), and finishing closes setup with a shutter that opens on the Library.
   */
  import { onDestroy, onMount } from 'svelte';
  import type { TransitionConfig } from 'svelte/transition';
  import {
    ArrowRight,
    Bot,
    Check,
    Gauge,
    Hand,
    Mic,
    Monitor,
    MousePointer2,
    RecordDot,
    RefreshCw,
    Stop,
    Video,
    ZoomIn,
  } from '@shotmaxxing/ui/icons';
  import {
    Button,
    CheckDraw,
    Icon,
    Keycap,
    ProgressRing,
    Spinner,
    bezier,
    env,
    fade,
    motionLevel,
    pop,
    toast,
    type IconData,
  } from '@shotmaxxing/ui';
  import { persist, recordingOptions, settings } from '../../lib/settings.svelte';
  import { isScreenRecordingProblem, reopenApp, screenRecordingFix } from '../../lib/screen-access';
  import { go } from '../../lib/router.svelte';
  import { agents, agentsLoaded, connectProblem, refreshAgents } from '../../lib/agents.svelte';
  import {
    connectAgent,
    editorTarget,
    isTauri,
    openPermissionSettings,
    openWindow,
    permissionStatus,
    startRecording,
    stopRecording,
    type PermissionKind,
    type PermissionState,
  } from '../../lib/native';
  import { iris, playIris } from '../../lib/iris.svelte';
  import Scene from '../../components/Scene.svelte';
  import { wallpaper } from '../../lib/wallpapers';
  import MagicShow from './MagicShow.svelte';
  import PermissionArt from './PermissionArt.svelte';

  const STEPS = ['Welcome', 'Permissions', 'Connect AI', 'Practice', 'Done'];
  const LAST = STEPS.length - 1;

  let step = $state(Math.min(settings.onboardingStep, LAST));
  let direction = $state(1);

  function goTo(next: number) {
    direction = next > step ? 1 : -1;
    step = next;
    settings.onboardingStep = next;
    if (next === LAST) settings.onboardingDone = true;
  }

  function finish(target: string) {
    settings.onboardingDone = true;
    settings.onboardingStep = 0;
    go(target);
  }

  /** The app opens projects in their own editor window and leaves this one on the Library. */
  function finishInEditor(target: string) {
    if (!isTauri) return finish(`editor/${target}`);
    finish('home/library');
    void openWindow('editor', target);
  }

  // The last step closes with the shutter: it shuts, the page behind it changes, and it opens on the new one.
  let closing = $state(false);
  function closeWith(then: () => void) {
    if (closing) return;
    closing = true;
    playIris('finale', { closed: then });
  }

  // Starting on the welcome, the logo builds itself first (any key or click skips it).
  onMount(() => {
    if (step === 0) playIris('intro');
  });

  /**
   * The move between scenes: both glide the same way on one curve, like a camera panning from one to the next, the
   * old one fading early and the new one late so their words never overlap. Reduce Motion gets a short fade.
   */
  const glide = bezier(0.65, 0, 0.35, 1);
  function pan(
    node: Element,
    { dir, leaving = false }: { dir: number; leaving?: boolean },
  ): TransitionConfig {
    if (motionLevel() === 'fades') return { duration: 160, css: (t) => `opacity: ${t}` };
    const shift = Math.min(520, (node as HTMLElement).offsetWidth * 0.4);
    return {
      duration: 540,
      easing: glide,
      css: leaving
        ? (_t, u) => `transform: translateX(${-dir * u * shift}px); opacity: ${Math.max(0, 1 - u * 2.1)}`
        : (t, u) => `transform: translateX(${dir * u * shift}px); opacity: ${Math.max(0, (t - 0.44) / 0.56)}`,
    };
  }

  /** Where the two soft lights sit in each scene (a fraction of the window), so the glow shifts as you go. */
  const GLOW: [number, number, number, number][] = [
    [0.7, 0.5, 0.56, 0.3],
    [0.74, 0.34, 0.86, 0.7],
    [0.3, 0.72, 0.72, 0.42],
    [0.66, 0.44, 0.36, 0.78],
    [0.28, 0.42, 0.62, 0.64],
  ];
  const glow = $derived(GLOW[step]);

  // Permissions, checked live while the step is open (one cheap call a second, stopped when it isn't needed).
  const PERMISSIONS: {
    kind: PermissionKind;
    icon: IconData;
    name: string;
    why: string;
    required?: boolean;
  }[] = [
    {
      kind: 'screen',
      icon: Monitor,
      name: 'Screen Recording',
      why: 'To film your screen. Without it, recordings come out blank.',
      required: true,
    },
    {
      kind: 'microphone',
      icon: Mic,
      name: 'Microphone',
      why: 'To record your voice while you show something.',
    },
    { kind: 'camera', icon: Video, name: 'Camera', why: 'To put you in a bubble next to the action.' },
    {
      kind: 'accessibility',
      icon: Hand,
      name: 'Accessibility',
      why: 'To notice clicks and typing for auto zoom, and to let agents press buttons for you.',
    },
  ];
  // Windows asks nothing before recording the screen or watching for clicks; only its privacy switches for the
  // microphone and camera can stand in the way.
  const shownPermissions = $derived(
    env.os === 'windows'
      ? PERMISSIONS.filter((p) => p.kind === 'microphone' || p.kind === 'camera')
      : PERMISSIONS,
  );
  let states = $state<Record<string, PermissionState>>({});
  let waiting = $state<PermissionKind | null>(null);
  // A permission still off after its trip to System Settings: the note under the list says what else to try.
  let missed = $state<PermissionKind | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;
  let comeBack: ReturnType<typeof setTimeout> | undefined;

  /** Nothing left to switch on: allowed, not asked for here, or allowed and waiting for Shotmaxxing to reopen. */
  const settled = (state: PermissionState | undefined) =>
    state === 'granted' || state === 'not_needed' || state === 'needs_restart';

  // One check at a time: a check asked for while one runs shares its answer.
  let checking: Promise<void> | null = null;
  function refresh(): Promise<void> {
    checking ??= permissionStatus()
      .catch(() => [])
      .then((list) => {
        for (const p of list) states[p.kind] = p.state;
        if (waiting && settled(states[waiting])) waiting = null;
        if (missed && settled(states[missed])) missed = null;
      })
      .finally(() => (checking = null));
    return checking;
  }

  // Back from System Settings: check at once, and if the permission is still off a moment later, offer Allow
  // again with what else to try.
  function cameBack() {
    void refresh();
    clearTimeout(comeBack);
    comeBack = setTimeout(async () => {
      await refresh();
      if (waiting && !settled(states[waiting])) {
        missed = waiting;
        waiting = null;
      }
    }, 2000);
  }

  const screenReady = $derived(
    env.os === 'windows' || states.screen === 'granted' || states.screen === 'not_needed',
  );
  const screenReopen = $derived(states.screen === 'needs_restart');
  // Before the first check there's nothing to say about Screen Recording yet.
  const screenKnown = $derived(env.os === 'windows' || states.screen !== undefined);

  // Every step starts from a fresh check (setup can open on any step), and it checks live while the Permissions
  // step is open, or the practice take while Screen Recording isn't ready. Not while the window is hidden.
  const watching = $derived(step === 1 || (step === 3 && !screenReady));
  $effect(() => {
    void step;
    void refresh();
  });
  $effect(() => {
    if (!watching) return;
    poll = setInterval(() => {
      if (!document.hidden) void refresh();
    }, 1000);
    window.addEventListener('focus', cameBack);
    return () => {
      clearInterval(poll);
      clearTimeout(comeBack);
      window.removeEventListener('focus', cameBack);
    };
  });

  let waitLimit: ReturnType<typeof setTimeout> | undefined;
  async function allow(kind: PermissionKind) {
    waiting = kind;
    if (missed === kind) missed = null;
    // Should nothing come up (this window keeps its focus), Allow comes back instead of waiting for ever.
    clearTimeout(waitLimit);
    waitLimit = setTimeout(() => {
      if (waiting === kind && document.hasFocus()) cameBack();
    }, 8000);
    await openPermissionSettings(kind).catch((error) =>
      toast(`Couldn’t open System Settings: ${error}`, { tone: 'danger' }),
    );
  }

  /** What to try when a permission stays off after a trip to System Settings. */
  function missedHint(kind: PermissionKind): string {
    const name = PERMISSIONS.find((p) => p.kind === kind)?.name ?? 'It';
    if (env.os === 'windows') {
      return `${name} is still off. In Settings › Privacy & security › ${name}, turn on access, including for desktop apps, then come back here.`;
    }
    if (kind === 'microphone' || kind === 'camera') {
      return `${name} is still off. Choose Allow to open its page in System Settings, and turn on Shotmaxxing there.`;
    }
    return `${name} still looks off. If its switch is already on, macOS is remembering an older copy of Shotmaxxing: select Shotmaxxing in that list, remove it with the − button, then choose Allow here again.`;
  }

  // macOS gives Screen Recording to a running app only once it reopens. Setup opens again on this step, the one
  // saved.
  let reopening = $state(false);
  async function reopen() {
    if (reopening) return;
    reopening = true;
    settings.onboardingStep = step;
    persist();
    if (!(await reopenApp('onboarding'))) reopening = false;
    // Should the app still be here a while later, the button works again.
    else setTimeout(() => (reopening = false), 10_000);
  }

  // Each permission's picture plays as the scene arrives, and again when you point at its row (not more than
  // once every two seconds, so passing over the list doesn't set them all off).
  let replays = $state<Record<string, number>>({});
  const lastReplay: Record<string, number> = {};
  function replay(kind: PermissionKind) {
    const now = performance.now();
    if (now - (lastReplay[kind] ?? 0) < 2000) return;
    lastReplay[kind] = now;
    replays[kind] = (replays[kind] ?? 0) + 1;
  }

  // Agents
  let connected = $state<Record<string, 'idle' | 'busy' | 'done'>>({});
  $effect(() => {
    for (const a of agents) if (a.state === 'connected' && !connected[a.id]) connected[a.id] = 'done';
  });
  onMount(() => void refreshAgents());
  async function connect(id: string) {
    connected[id] = 'busy';
    try {
      const result = await connectAgent(id);
      connected[id] = result.ok ? 'done' : 'idle';
      if (!result.ok) connectProblem(result);
    } catch (error) {
      connected[id] = 'idle';
      toast(`Couldn’t connect: ${error}`, { tone: 'danger' });
    }
  }
  const connectedCount = $derived(Object.values(connected).filter((s) => s === 'done').length);
  // Agents that aren't installed get one line under the list instead of a row each.
  const installedAgents = $derived(agents.filter((a) => a.state !== 'not_installed'));
  const missingAgents = $derived(agents.filter((a) => a.state === 'not_installed'));
  const listNames = (names: string[]) =>
    names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;

  // Practice take: countdown, ten seconds of recording, then the result.
  let practice = $state<'ready' | 'countdown' | 'recording' | 'saving' | 'done'>('ready');
  // In the app the practice take is a real recording of the main screen; a browser preview acts it out.
  let practiceProject = $state<string | null>(null);
  let practiceZooms = $state(3);
  let count = $state(3);
  let elapsed = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;
  let stageWidth = $state(560);

  async function practiceFailed(error: unknown) {
    practice = 'ready';
    const message = error instanceof Error ? error.message : String(error);
    const fix = isScreenRecordingProblem(message) ? await screenRecordingFix() : null;
    // The stage then shows the way out too, and watches until Screen Recording works.
    if (fix) void refresh();
    toast(message, { tone: 'danger', timeout: 8000, action: fix ?? undefined });
  }

  function startPractice() {
    practice = 'countdown';
    count = 3;
    timer = setInterval(async () => {
      count -= 1;
      if (count === 0) {
        clearInterval(timer);
        if (isTauri) {
          try {
            await startRecording('display', 'Practice take', recordingOptions());
          } catch (error) {
            return practiceFailed(error);
          }
        }
        practice = 'recording';
        elapsed = 0;
        const started = performance.now();
        timer = setInterval(() => {
          elapsed = Math.min(10, (performance.now() - started) / 1000);
          if (elapsed >= 10) stopPractice();
        }, 250);
      }
    }, 1000);
  }

  async function stopPractice() {
    clearInterval(timer);
    if (isTauri) {
      practice = 'saving';
      try {
        const done = await stopRecording();
        practiceProject = done.project;
        practiceZooms = done.zooms;
      } catch (error) {
        return practiceFailed(error);
      }
    }
    practice = 'done';
  }

  onDestroy(() => {
    clearInterval(timer);
    clearInterval(poll);
    clearTimeout(comeBack);
    clearTimeout(waitLimit);
  });

  const machine = $derived(env.os === 'mac' ? 'Mac' : 'PC');

  const FEATURES: { icon: IconData; title: string }[] = [
    { icon: ZoomIn, title: 'Zooms that say why' },
    { icon: MousePointer2, title: 'A cursor that glides' },
    { icon: Bot, title: 'Agents film for you' },
    { icon: Gauge, title: 'Light on your battery' },
  ];
</script>

<div class="relative flex h-full flex-col overflow-hidden bg-bg">
  <!-- Two soft lights, uneven on purpose, that drift to new spots as the scenes change. -->
  <div class="pointer-events-none absolute inset-0" aria-hidden="true">
    <span
      class="glow glow-wide"
      style:transform="translate(calc({glow[0]} * 100vw - 50%), calc({glow[1]} * 100vh - 50%))"
    ></span>
    <span
      class="glow glow-near"
      style:transform="translate(calc({glow[2]} * 100vw - 50%), calc({glow[3]} * 100vh - 50%))"
    ></span>
  </div>

  <header
    data-tauri-drag-region
    class="drag-region relative z-10 flex h-[var(--titlebar-height)] shrink-0 items-center justify-end pl-[var(--titlebar-inset-start)] pr-3"
  >
    {#if step < LAST}
      <button
        type="button"
        class="no-drag press rounded-[6px] px-2 py-1 text-footnote text-fg-muted hover:bg-fill hover:text-fg"
        onclick={() => finish('home/library')}>Skip setup</button
      >
    {/if}
  </header>

  <main class="relative grid min-h-0 flex-1 overflow-hidden">
    {#key step}
      <section
        class="scroll-room col-start-1 row-start-1 overflow-y-auto"
        in:pan={{ dir: direction }}
        out:pan={{ dir: direction, leaving: true }}
      >
        <div
          class="scene mx-auto grid min-h-full w-full max-w-[1100px] grid-cols-[minmax(0,4.5fr)_minmax(0,7fr)] items-center gap-x-[clamp(32px,5vw,72px)] px-[clamp(32px,5.5vw,72px)] py-6"
          class:behind={step === 0 && !iris.revealed}
        >
          {#if step === 0}
            {#if iris.revealed}
              <div class="flex flex-col items-start">
                <p class="eyebrow rise-in" style="--i: 0">Welcome to Shotmaxxing</p>
                <h1 class="scene-title rise-in mt-3" style="--i: 1">Demos that edit themselves</h1>
                <p class="measure rise-in mt-4 text-body text-fg-muted" style="--i: 2">
                  Record your screen by hand, or let an AI agent film it for you. Shotmaxxing adds the zooms,
                  smooths the cursor and keeps your {machine} cool while it works.
                </p>
                <ul class="mt-6 grid w-full grid-cols-2 gap-x-4 gap-y-3">
                  {#each FEATURES as item, i (item.title)}
                    <li class="rise-in flex items-center gap-2.5" style="--i: {i + 3}">
                      <span
                        class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-accent-tint text-accent-text"
                      >
                        <Icon icon={item.icon} size={16} aria-hidden="true" />
                      </span>
                      <span class="text-callout font-medium">{item.title}</span>
                    </li>
                  {/each}
                </ul>
                <div class="rise-in mt-8" style="--i: 7">
                  <Button variant="primary" size="lg" iconEnd={ArrowRight} onclick={() => goTo(1)}>
                    Get started
                  </Button>
                </div>
              </div>
              <div class="rise-in" style="--i: 2">
                <MagicShow />
              </div>
            {/if}
          {:else if step === 1}
            <div class="flex flex-col items-start">
              <p class="eyebrow">Step 2 of 5</p>
              <h1 class="scene-title mt-3">
                {env.os === 'windows' ? 'Allow the microphone and camera' : 'Allow what Shotmaxxing needs'}
              </h1>
              <p class="measure mt-4 text-body text-fg-muted">
                {env.os === 'windows'
                  ? 'Windows records the screen without asking. The microphone and camera are up to you; this page checks live, so it updates the moment you turn one on.'
                  : 'Only Screen Recording is required. This page checks live, so it updates the moment you allow something.'}
              </p>
              <div class="mt-8 flex flex-wrap items-center gap-3">
                {#if screenReopen}
                  <Button variant="primary" size="lg" icon={RefreshCw} loading={reopening} onclick={reopen}>
                    Reopen Shotmaxxing
                  </Button>
                {:else}
                  <Button
                    variant="primary"
                    size="lg"
                    iconEnd={ArrowRight}
                    disabled={!screenReady}
                    onclick={() => goTo(2)}>Continue</Button
                  >
                {/if}
              </div>
              <!-- Two lines kept free, so the column doesn't jump as the note changes. -->
              <div class="measure mt-3 min-h-[2lh] text-footnote text-fg-muted" aria-live="polite">
                {#if screenReopen}
                  <p in:fade>
                    Screen Recording is on. macOS lets Shotmaxxing use it once it reopens, and setup comes
                    back to this step.
                  </p>
                {:else if screenKnown && !screenReady}
                  <p>
                    Allow Screen Recording to continue, or
                    <button
                      type="button"
                      class="press rounded-[4px] font-medium text-accent-text underline-offset-2 hover:underline"
                      onclick={() => goTo(2)}>continue without it</button
                    >.
                  </p>
                {/if}
              </div>
            </div>
            <div>
              <ul
                class="divide-y divide-border rounded-card border border-border bg-surface px-4 shadow-[var(--elev-2)]"
              >
                {#each shownPermissions as p, i (p.kind)}
                  {@const state = states[p.kind]}
                  <li class="flex items-center gap-3.5 py-3" onpointerenter={() => replay(p.kind)}>
                    <PermissionArt
                      kind={p.kind}
                      play={replays[p.kind] ?? 0}
                      delay={replays[p.kind] ? 0 : 380 + i * 140}
                    />
                    <div class="min-w-0 flex-1">
                      <p class="flex items-center gap-2 text-callout font-semibold">
                        {p.name}
                        <span
                          class="rounded-full bg-fill px-1.5 py-px text-caption font-medium text-fg-muted"
                        >
                          {p.required ? 'Required' : 'Optional'}
                        </span>
                      </p>
                      <p class="mt-0.5 text-footnote text-fg-muted">{p.why}</p>
                    </div>
                    <div class="flex w-32 shrink-0 justify-end">
                      {#if state === 'granted' || state === 'not_needed'}
                        <span class="flex items-center gap-1.5 text-callout font-medium text-success" in:pop>
                          <CheckDraw size={16} />
                          {state === 'granted' ? 'Allowed' : 'Not needed'}
                        </span>
                      {:else if state === 'needs_restart'}
                        <span
                          class="flex items-center gap-1.5 text-footnote font-medium text-accent-text"
                          in:fade
                        >
                          <Icon icon={RefreshCw} size={14} aria-hidden="true" /> Reopen to use
                        </span>
                      {:else if waiting === p.kind}
                        <span class="flex items-center gap-2 text-footnote text-fg-muted" in:fade>
                          <Spinner size={14} label="Waiting" /> Waiting…
                        </span>
                      {:else}
                        <Button
                          size="sm"
                          variant={p.required ? 'primary' : 'secondary'}
                          onclick={() => allow(p.kind)}>Allow</Button
                        >
                      {/if}
                    </div>
                  </li>
                {/each}
              </ul>
              <p class={['mt-3 text-footnote', missed ? 'text-fg' : 'text-fg-muted']} aria-live="polite">
                {#if missed}
                  {missedHint(missed)}
                {:else if env.os === 'mac'}
                  Allow opens System Settings › Privacy & Security. Turn on Shotmaxxing, then come back here.
                {:else}
                  Allow opens Settings › Privacy & security. Turn on access, including for desktop apps, then
                  come back here.
                {/if}
              </p>
            </div>
          {:else if step === 2}
            <div class="flex flex-col items-start">
              <p class="eyebrow">Step 3 of 5 · Optional</p>
              <h1 class="scene-title mt-3">Let AI agents film for you</h1>
              <p class="measure mt-4 text-body text-fg-muted">
                Connect the agents you already use. Each one gets a single new tool, Shotmaxxing, and asks you
                before it touches your mouse or keyboard. Everything stays on this {machine}.
              </p>
              <div class="mt-8 flex flex-wrap items-center gap-3">
                <Button variant="primary" size="lg" iconEnd={ArrowRight} onclick={() => goTo(3)}>
                  Continue
                </Button>
                {#if !connectedCount}
                  <span class="text-footnote text-fg-muted">Or later, from Agents</span>
                {/if}
              </div>
            </div>
            <div>
              {#if installedAgents.length}
                <ul class="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-2">
                  {#each installedAgents as agent, i (agent.id)}
                    <li
                      class="rise-in flex items-center gap-3 rounded-card border border-border bg-surface px-3 py-2 shadow-[var(--elev-1)]"
                      style="--i: {i + 2}"
                    >
                      <span
                        class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-fill text-fg-muted"
                      >
                        <Icon icon={Bot} size={16} aria-hidden="true" />
                      </span>
                      <span class="min-w-0 flex-1">
                        <span class="block truncate text-callout font-semibold">{agent.name}</span>
                        <span class="block truncate text-footnote text-fg-muted">{agent.kind}</span>
                      </span>
                      {#if connected[agent.id] === 'done'}
                        <span class="flex items-center gap-1 text-footnote font-semibold text-success"
                          ><CheckDraw size={14} />Connected</span
                        >
                      {:else}
                        <Button
                          size="sm"
                          loading={connected[agent.id] === 'busy'}
                          onclick={() => connect(agent.id)}>Connect</Button
                        >
                      {/if}
                    </li>
                  {/each}
                </ul>
              {:else if agentsLoaded.value}
                <p
                  class="rounded-card border border-border bg-surface p-5 text-callout text-fg-muted shadow-[var(--elev-1)]"
                >
                  No agents on this {machine} yet. Install Claude Code, Codex or another one, then connect it from
                  Agents.
                </p>
              {/if}
              {#if agentsLoaded.value && missingAgents.length && installedAgents.length}
                <p class="mt-3 text-footnote text-fg-muted">
                  Not found on this {machine}: {listNames(missingAgents.map((a) => a.name))}.
                </p>
              {/if}
            </div>
          {:else if step === 3}
            <div class="flex flex-col items-start">
              <p class="eyebrow">Step 4 of 5</p>
              <h1 class="scene-title mt-3">Try a 10-second practice take</h1>
              <p class="measure mt-4 text-body text-fg-muted">
                Press Record, then move around and click a few things. You'll see the result with zooms
                already added.
              </p>
              <div class="mt-8 flex flex-wrap items-center gap-3">
                {#if practice === 'done'}
                  <Button variant="primary" size="lg" iconEnd={ArrowRight} onclick={() => goTo(4)}>
                    Continue
                  </Button>
                {:else}
                  <Button variant="ghost" size="lg" class="-ml-4" onclick={() => goTo(4)}
                    >Skip practice</Button
                  >
                {/if}
              </div>
            </div>
            <div>
              <div
                class="relative aspect-[16/10] w-full overflow-hidden rounded-[16px] bg-stage shadow-[var(--elev-3)]"
                style:background={wallpaper('graphite').css}
                bind:clientWidth={stageWidth}
              >
                <div
                  class="absolute inset-[8%] overflow-hidden rounded-[8px] opacity-60 shadow-[var(--elev-2)]"
                >
                  <Scene variant={4} width={stageWidth * 0.84} />
                </div>
                <div class="absolute inset-0 grid place-items-center">
                  {#if practice === 'ready' && screenKnown && !screenReady}
                    <!-- Continued without Screen Recording: the way to turn it on, live, instead of a take that
                         can't start. -->
                    <div
                      class="mx-6 flex max-w-[340px] flex-col items-center gap-2 rounded-[16px] border border-border bg-surface px-5 py-4 text-center text-fg shadow-[var(--elev-3)]"
                      in:pop
                    >
                      <Icon icon={Monitor} size={26} class="text-fg-muted" aria-hidden="true" />
                      <span class="text-callout font-semibold">
                        {screenReopen
                          ? 'Reopen Shotmaxxing to record'
                          : 'The practice take needs Screen Recording'}
                      </span>
                      <span class="text-footnote text-fg-muted" aria-live="polite">
                        {screenReopen
                          ? 'Screen Recording is on. macOS lets Shotmaxxing use it once it reopens, and setup comes back here.'
                          : missed === 'screen'
                            ? missedHint('screen')
                            : 'Allow opens System Settings › Privacy & Security. Turn on Shotmaxxing, then come back here.'}
                      </span>
                      <div class="mt-1">
                        {#if screenReopen}
                          <Button
                            size="sm"
                            variant="primary"
                            icon={RefreshCw}
                            loading={reopening}
                            onclick={reopen}>Reopen Shotmaxxing</Button
                          >
                        {:else if waiting === 'screen'}
                          <span class="flex items-center gap-2 text-footnote text-fg-muted" in:fade>
                            <Spinner size={14} label="Waiting" /> Waiting…
                          </span>
                        {:else}
                          <Button size="sm" variant="primary" onclick={() => allow('screen')}
                            >Allow Screen Recording</Button
                          >
                        {/if}
                      </div>
                    </div>
                  {:else if practice === 'ready'}
                    <button
                      type="button"
                      class="press group flex flex-col items-center gap-3 rounded-[20px] p-4 text-fg-on-stage"
                      onclick={startPractice}
                      in:pop
                    >
                      <span
                        class="grid size-20 place-items-center rounded-full bg-accent text-accent-fg shadow-[var(--elev-3)]"
                      >
                        <Icon icon={RecordDot} size={44} aria-hidden="true" />
                      </span>
                      <span class="rounded-full bg-scrim px-3 py-1 text-callout font-semibold"
                        >Record practice take</span
                      >
                    </button>
                  {:else if practice === 'countdown'}
                    {#key count}
                      <span
                        class="text-[96px] font-bold leading-none text-fg-on-stage tabular"
                        in:pop={{ from: 0.6, duration: 260 }}>{count}</span
                      >
                    {/key}
                  {:else if practice === 'saving'}
                    <div class="flex flex-col items-center gap-3 text-fg-on-stage" in:fade role="status">
                      <Spinner size={22} />
                      <span class="rounded-full bg-scrim px-3 py-1 text-callout font-semibold"
                        >Adding zooms…</span
                      >
                    </div>
                  {:else if practice === 'recording'}
                    <div class="flex flex-col items-center gap-4 text-fg-on-stage" in:fade>
                      <span
                        class="flex items-center gap-2 rounded-full bg-scrim px-3 py-1.5 text-callout font-semibold tabular"
                      >
                        <span class="size-2 rounded-full bg-record"></span>
                        0:{String(Math.floor(elapsed)).padStart(2, '0')}
                      </span>
                      <button
                        type="button"
                        class="press flex items-center gap-2 rounded-full bg-accent px-4 py-2 text-callout font-semibold text-accent-fg"
                        onclick={stopPractice}
                      >
                        <ProgressRing value={elapsed / 10} size={16} label="Practice take progress" />
                        <Icon icon={Stop} size={12} fill="currentColor" aria-hidden="true" /> Stop
                      </button>
                    </div>
                  {:else}
                    <div class="flex flex-col items-center gap-2 text-fg-on-stage" in:pop>
                      <span class="grid size-14 place-items-center rounded-full bg-accent text-accent-fg"
                        ><CheckDraw size={26} /></span
                      >
                      <span class="text-title-3">Nice take</span>
                      <span class="text-callout opacity-80"
                        >{practiceZooms === 0
                          ? 'No zooms yet: clicks on buttons, links and fields make them'
                          : `${practiceZooms} ${practiceZooms === 1 ? 'zoom' : 'zooms'} added from your clicks`}</span
                      >
                    </div>
                  {/if}
                </div>
              </div>
              <p class="mt-3 text-footnote text-fg-muted">
                It records your main screen. Practice takes go to your Library like any recording, so you can
                delete them there.
              </p>
            </div>
          {:else}
            <div class="flex flex-col items-start">
              <span
                class="grid size-14 place-items-center rounded-full bg-accent text-accent-fg shadow-[var(--top-light)]"
                in:pop
              >
                <CheckDraw size={28} />
              </span>
              <h1 class="scene-title mt-6">You're all set</h1>
              <p class="measure mt-4 text-body text-fg-muted">
                Record from anywhere, or ask an agent to film a demo for you. The editor opens with a short
                tour.
              </p>
              <div class="mt-8 flex flex-wrap items-center gap-3">
                <Button
                  variant="primary"
                  size="lg"
                  disabled={closing}
                  onclick={() =>
                    closeWith(() =>
                      finishInEditor(
                        practiceProject ? `${editorTarget(practiceProject)}?tour=1` : 'sample?tour=1',
                      ),
                    )}
                >
                  {practice === 'done' ? 'Open my practice take' : 'Open the sample project'}
                </Button>
                <Button
                  variant="ghost"
                  size="lg"
                  disabled={closing}
                  onclick={() => closeWith(() => finish('home/library'))}
                >
                  Go to Library
                </Button>
              </div>
            </div>
            <div class="flex flex-col gap-3">
              <ul
                class="flex flex-col gap-2.5 rounded-card border border-border bg-surface p-4 text-callout shadow-[var(--elev-2)]"
              >
                <li class="flex items-center gap-2.5">
                  <Icon
                    icon={Check}
                    size={16}
                    class={screenReady ? 'text-success' : 'text-fg-subtle'}
                    aria-hidden="true"
                  />
                  {env.os === 'windows'
                    ? 'Ready to record your screen'
                    : screenReady
                      ? 'Screen Recording allowed'
                      : screenReopen
                        ? 'Screen Recording works once Shotmaxxing reopens'
                        : 'Screen Recording is still off. Record shows how to turn it on.'}
                </li>
                <li class="flex items-center gap-2.5">
                  <Icon
                    icon={Check}
                    size={16}
                    class={connectedCount ? 'text-success' : 'text-fg-subtle'}
                    aria-hidden="true"
                  />
                  {connectedCount
                    ? `${connectedCount} AI agent${connectedCount === 1 ? '' : 's'} connected`
                    : 'No agents yet. Connect one any time from Agents.'}
                </li>
                <li class="flex items-center gap-2.5">
                  <Icon
                    icon={Check}
                    size={16}
                    class={practice === 'done' ? 'text-success' : 'text-fg-subtle'}
                    aria-hidden="true"
                  />
                  {practice === 'done'
                    ? 'Practice take recorded'
                    : 'Practice skipped. The sample project is ready instead.'}
                </li>
              </ul>
              <div class="rounded-card border border-border bg-surface p-4 shadow-[var(--elev-2)]">
                <p class="eyebrow mb-3">Worth remembering</p>
                <div class="grid grid-cols-3 gap-3 text-footnote text-fg-muted">
                  <span class="flex flex-col gap-1.5"
                    ><Keycap chord="alt+shift+5" size="md" />Record from anywhere</span
                  >
                  <span class="flex flex-col gap-1.5"
                    ><Keycap chord="mod+k" size="md" />Search and run anything</span
                  >
                  <span class="flex flex-col gap-1.5"
                    ><Keycap chord="mod+/" size="md" />See every shortcut</span
                  >
                </div>
              </div>
            </div>
          {/if}
        </div>
      </section>
    {/key}
  </main>

  <footer class="relative z-10 grid h-14 shrink-0 grid-cols-[1fr_auto_1fr] items-center px-6">
    <div>
      {#if step > 0 && step < LAST}
        <Button variant="ghost" onclick={() => goTo(step - 1)}>Back</Button>
      {/if}
    </div>
    <nav aria-label="Setup steps" class="flex items-center">
      {#each STEPS as name, i (name)}
        <button
          type="button"
          class="press grid h-7 place-items-center rounded-full px-1 enabled:hover:bg-fill disabled:cursor-default"
          disabled={i >= step || closing}
          aria-label="{name}, step {i + 1} of {STEPS.length}"
          aria-current={i === step ? 'step' : undefined}
          title={name}
          onclick={() => goTo(i)}
        >
          <span
            class="dot block h-1.5 rounded-full {i === step
              ? 'w-5 bg-accent'
              : i < step
                ? 'w-1.5 bg-fg-subtle'
                : 'w-1.5 bg-fill-strong'}"
          ></span>
        </button>
      {/each}
    </nav>
    <div></div>
  </footer>
</div>

<style>
  .scene-title {
    font-size: clamp(30px, 3.3vw, 42px);
    line-height: 1.1;
    letter-spacing: -0.03em;
    font-weight: 650;
    text-wrap: balance;
  }
  .scene {
    transform-origin: 50% 45%;
    transition: transform 1000ms var(--ease-out); /* design-guard-allow long: the welcome settling as the lens opens */
  }
  /* Behind the closed iris the welcome sits a little closer, then settles back as the lens opens onto it. */
  .scene.behind {
    transform: scale(1.06);
    transition: none;
  }
  .glow {
    position: absolute;
    left: 0;
    top: 0;
    border-radius: 50%;
    will-change: transform;
    transition: transform 900ms var(--ease-out); /* design-guard-allow long: the light drifting between scenes */
  }
  .glow-wide {
    width: 980px;
    height: 760px;
    background: radial-gradient(closest-side, var(--scene-glow), transparent);
  }
  .glow-near {
    width: 460px;
    height: 400px;
    opacity: 0.8;
    background: radial-gradient(closest-side, var(--scene-glow), transparent);
    transition-duration: 1250ms; /* design-guard-allow long: the second light trails the first */
  }
  :global(:root[data-motion='fades']) .glow,
  :global(:root[data-motion='fades']) .scene {
    transition: none;
  }
</style>
