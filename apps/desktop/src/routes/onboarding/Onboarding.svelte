<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    Bot,
    Check,
    Gauge,
    Hand,
    Mic,
    Monitor,
    MousePointer2,
    Video,
    ZoomIn,
    Circle,
    Square,
  } from '@lucide/svelte';
  import {
    Button,
    CheckDraw,
    Keycap,
    Logo,
    ProgressRing,
    Spinner,
    Stepper,
    env,
    fade,
    pop,
    slide,
    toast,
  } from '@shotmaxxing/ui';
  import { settings } from '../../lib/settings.svelte';
  import { go } from '../../lib/router.svelte';
  import { agents, refreshAgents } from '../../lib/agents.svelte';
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
  import Scene from '../../components/Scene.svelte';
  import { wallpaper } from '../../lib/wallpapers';

  const STEPS = ['Welcome', 'Permissions', 'Connect AI', 'Practice', 'Done'];

  let step = $state(Math.min(settings.onboardingStep, STEPS.length - 1));
  let direction = $state(1);

  function goTo(next: number) {
    direction = next > step ? 1 : -1;
    step = next;
    settings.onboardingStep = next;
    if (next === STEPS.length - 1) settings.onboardingDone = true;
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

  // Permissions, checked live while the step is open (one cheap call a second, stopped when it isn't needed).
  const PERMISSIONS: {
    kind: PermissionKind;
    icon: typeof Monitor;
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
  let states = $state<Record<string, PermissionState>>({});
  let waiting = $state<PermissionKind | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;

  async function refresh() {
    const list = await permissionStatus().catch(() => []);
    for (const p of list) states[p.kind] = p.state;
    if (waiting && (states[waiting] === 'granted' || states[waiting] === 'not_needed')) waiting = null;
  }

  $effect(() => {
    if (step !== 1) return;
    refresh();
    poll = setInterval(refresh, 1000);
    return () => clearInterval(poll);
  });

  async function allow(kind: PermissionKind) {
    waiting = kind;
    await openPermissionSettings(kind);
  }

  const screenReady = $derived(states.screen === 'granted' || states.screen === 'not_needed');

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
      if (!result.ok) toast(result.message, { tone: 'danger' });
    } catch (error) {
      connected[id] = 'idle';
      toast(`Couldn’t connect: ${error}`, { tone: 'danger' });
    }
  }
  const connectedCount = $derived(Object.values(connected).filter((s) => s === 'done').length);

  // Practice take: countdown, ten seconds of recording, then the result.
  let practice = $state<'ready' | 'countdown' | 'recording' | 'saving' | 'done'>('ready');
  // In the app the practice take is a real recording of the main screen; a browser preview acts it out.
  let practiceProject = $state<string | null>(null);
  let practiceZooms = $state(3);
  let count = $state(3);
  let elapsed = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  function practiceFailed(error: unknown) {
    practice = 'ready';
    const message = error instanceof Error ? error.message : String(error);
    toast(message, {
      tone: 'danger',
      timeout: 8000,
      action: /Screen Recording/i.test(message)
        ? { label: 'Open settings', run: () => openPermissionSettings('screen') }
        : undefined,
    });
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
            await startRecording('display', 'Practice take');
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
  });

  const machine = $derived(env.os === 'mac' ? 'Mac' : 'PC');
</script>

<div class="flex h-full flex-col bg-bg">
  <header
    data-tauri-drag-region
    class="drag-region flex h-[var(--titlebar-height)] shrink-0 items-center justify-center pl-[var(--titlebar-inset-start)] pr-3"
  >
    <div class="no-drag"><Stepper steps={STEPS} current={step} onselect={goTo} /></div>
    {#if step < STEPS.length - 1}
      <button
        type="button"
        class="no-drag press absolute right-3 rounded-[6px] px-2 py-1 text-footnote text-fg-muted hover:bg-fill hover:text-fg"
        onclick={() => finish('home/library')}>Skip setup</button
      >
    {/if}
  </header>

  <main class="grid min-h-0 flex-1 overflow-hidden">
    {#key step}
      <section
        class="scroll-room col-start-1 row-start-1 overflow-y-auto"
        in:slide={{ x: direction * 28, duration: 280 }}
        out:fade={{ duration: 120 }}
      >
        <div class="mx-auto flex min-h-full w-full max-w-[600px] flex-col justify-center px-8 py-8">
          {#if step === 0}
            <div class="flex flex-col items-start">
              <Logo size={64} tile animate />
              <h1 class="mt-6 text-display">Demos that edit themselves</h1>
              <p class="measure mt-3 text-body text-fg-muted">
                Record your screen by hand, or let an AI agent film it for you. Shotmaxxing adds the zooms,
                smooths the cursor and keeps your {machine} cool while it works.
              </p>
              <ul class="mt-7 flex w-full flex-col gap-3.5">
                {#each [{ icon: ZoomIn, title: 'Auto zoom that explains itself', body: 'Every zoom says why it is there, and you can change any of them.' }, { icon: MousePointer2, title: 'A cursor people can follow', body: 'Bigger, smoother, and landing exactly on every click.' }, { icon: Bot, title: 'Agents can film for you', body: 'Claude, Codex and others record a whole demo from one sentence.' }, { icon: Gauge, title: 'Light on your battery', body: 'Hardware encoding and a native renderer. No fans spinning up.' }] as item, i (item.title)}
                  {@const Icon = item.icon}
                  <li class="rise-in flex items-start gap-3.5" style="--i: {i + 1}">
                    <span
                      class="grid size-9 shrink-0 place-items-center rounded-[10px] bg-accent-tint text-accent-text"
                    >
                      <Icon size={18} strokeWidth={2} aria-hidden="true" />
                    </span>
                    <span>
                      <span class="block text-callout font-semibold">{item.title}</span>
                      <span class="block text-callout text-fg-muted">{item.body}</span>
                    </span>
                  </li>
                {/each}
              </ul>
            </div>
          {:else if step === 1}
            <h1 class="text-title-1">Allow what Shotmaxxing needs</h1>
            <p class="measure mt-2 text-body text-fg-muted">
              Only Screen Recording is required. This page checks live, so it updates the moment you allow
              something.
            </p>
            <ul
              class="mt-6 divide-y divide-border rounded-card border border-border bg-surface px-4 shadow-[var(--elev-1)]"
            >
              {#each PERMISSIONS as p (p.kind)}
                {@const Icon = p.icon}
                {@const state = states[p.kind]}
                <li class="flex items-center gap-3.5 py-3.5">
                  <span class="grid size-9 shrink-0 place-items-center rounded-[10px] bg-fill text-fg-muted">
                    <Icon size={18} strokeWidth={1.9} aria-hidden="true" />
                  </span>
                  <div class="min-w-0 flex-1">
                    <p class="flex items-center gap-2 text-callout font-semibold">
                      {p.name}
                      <span class="rounded-full bg-fill px-1.5 py-px text-caption font-medium text-fg-muted">
                        {p.required ? 'Required' : 'Optional'}
                      </span>
                    </p>
                    <p class="mt-0.5 text-footnote text-fg-muted">{p.why}</p>
                  </div>
                  <div class="flex w-36 shrink-0 justify-end">
                    {#if state === 'granted' || state === 'not_needed'}
                      <span class="flex items-center gap-1.5 text-callout font-medium text-success" in:pop>
                        <CheckDraw size={16} />
                        {state === 'granted' ? 'Allowed' : 'Not needed'}
                      </span>
                    {:else if waiting === p.kind}
                      <span class="flex items-center gap-2 text-footnote text-fg-muted" in:fade>
                        <Spinner size={14} label="Waiting" /> Waiting for you…
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
            <p class="mt-3 text-footnote text-fg-muted">
              {env.os === 'mac'
                ? 'Allow opens System Settings › Privacy & Security. Turn on Shotmaxxing, then come back here.'
                : 'Allow opens Settings › Privacy & security. Turn on access for Shotmaxxing, then come back here.'}
            </p>
          {:else if step === 2}
            <h1 class="flex items-center gap-2.5 text-title-1">
              Let AI agents film for you
              <span class="rounded-full bg-fill px-2 py-0.5 text-footnote font-medium text-fg-muted"
                >Optional</span
              >
            </h1>
            <p class="measure mt-2 text-body text-fg-muted">
              Connect the agents you already use. Each one gets a single new tool, Shotmaxxing, and asks you
              before it touches your mouse or keyboard. Everything stays on this {machine}.
            </p>
            <ul class="mt-6 grid grid-cols-2 gap-2">
              {#each agents as agent, i (agent.id)}
                <li
                  class="rise-in flex items-center gap-3 rounded-card border border-border bg-surface px-3 py-2.5 shadow-[var(--elev-1)]"
                  style="--i: {i}"
                >
                  <span class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-fill text-fg-muted">
                    <Bot size={16} aria-hidden="true" />
                  </span>
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-callout font-semibold">{agent.name}</span>
                    <span class="block truncate text-footnote text-fg-muted">
                      {agent.state === 'not_installed' ? 'Not found on this computer' : agent.kind}
                    </span>
                  </span>
                  {#if connected[agent.id] === 'done'}
                    <span class="flex items-center gap-1 text-footnote font-semibold text-success"
                      ><CheckDraw size={14} />Connected</span
                    >
                  {:else if agent.state === 'not_installed'}
                    <span class="text-footnote text-fg-subtle">—</span>
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
          {:else if step === 3}
            <h1 class="text-title-1">Try a 10-second practice take</h1>
            <p class="measure mt-2 text-body text-fg-muted">
              Press Record, then move around and click a few things. You'll see the result with zooms already
              added.
            </p>
            <div
              class="relative mt-6 aspect-[16/10] w-full overflow-hidden rounded-card bg-stage shadow-[var(--elev-2)]"
              style:background={wallpaper('graphite').css}
            >
              <div
                class="absolute inset-[8%] overflow-hidden rounded-[8px] opacity-60 shadow-[var(--elev-2)]"
              >
                <Scene variant={4} width={600 * 0.84} />
              </div>
              <div class="absolute inset-0 grid place-items-center">
                {#if practice === 'ready'}
                  <button
                    type="button"
                    class="press group flex flex-col items-center gap-3 rounded-[20px] p-4 text-fg-on-stage"
                    onclick={startPractice}
                    in:pop
                  >
                    <span
                      class="grid size-20 place-items-center rounded-full bg-accent text-accent-fg shadow-[var(--elev-3)]"
                    >
                      <Circle size={30} fill="currentColor" aria-hidden="true" />
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
                      <Square size={12} fill="currentColor" aria-hidden="true" /> Stop
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
                        ? 'Nothing was clicked, so no zooms yet'
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
          {:else}
            <div class="flex flex-col items-start">
              <span
                class="grid size-14 place-items-center rounded-full bg-accent text-accent-fg shadow-[var(--top-light)]"
                in:pop
              >
                <CheckDraw size={28} />
              </span>
              <h1 class="mt-5 text-title-1">You're all set</h1>
              <ul class="mt-4 flex flex-col gap-2 text-callout">
                <li class="flex items-center gap-2">
                  <Check size={16} class="text-success" aria-hidden="true" /> Screen Recording allowed
                </li>
                <li class="flex items-center gap-2">
                  <Check
                    size={16}
                    class={connectedCount ? 'text-success' : 'text-fg-subtle'}
                    aria-hidden="true"
                  />
                  {connectedCount
                    ? `${connectedCount} AI agent${connectedCount === 1 ? '' : 's'} connected`
                    : 'No agents yet. Connect one any time from Agents.'}
                </li>
                <li class="flex items-center gap-2">
                  <Check
                    size={16}
                    class={practice === 'done' ? 'text-success' : 'text-fg-subtle'}
                    aria-hidden="true"
                  />
                  {practice === 'done'
                    ? 'Practice take recorded'
                    : 'Practice skipped. The sample project is ready instead.'}
                </li>
              </ul>
              <div
                class="mt-6 w-full rounded-card border border-border bg-surface p-4 shadow-[var(--elev-1)]"
              >
                <p class="eyebrow mb-2">Worth remembering</p>
                <div class="grid grid-cols-3 gap-3 text-footnote text-fg-muted">
                  <span class="flex flex-col gap-1.5"
                    ><Keycap chord="alt+shift+5" size="md" />Record from anywhere</span
                  >
                  <span class="flex flex-col gap-1.5"><Keycap chord="mod+k" size="md" />Find any tool</span>
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

  <footer class="flex h-16 shrink-0 items-center gap-2 border-t border-border px-6">
    {#if step > 0 && step < STEPS.length - 1}
      <Button variant="ghost" onclick={() => goTo(step - 1)}>Back</Button>
    {/if}
    <span class="flex-1"></span>
    {#if step === 0}
      <Button variant="primary" size="lg" onclick={() => goTo(1)}>Get started</Button>
    {:else if step === 1}
      {#if !screenReady}<span class="mr-2 text-footnote text-fg-muted"
          >Allow Screen Recording to continue</span
        >{/if}
      <Button variant="primary" disabled={!screenReady} onclick={() => goTo(2)}>Continue</Button>
    {:else if step === 2}
      <Button variant="ghost" onclick={() => goTo(3)}>Skip for now</Button>
      <Button variant="primary" onclick={() => goTo(3)}>Continue</Button>
    {:else if step === 3}
      {#if practice !== 'done'}<Button variant="ghost" onclick={() => goTo(4)}>Skip practice</Button>{/if}
      <Button variant="primary" disabled={practice !== 'done'} onclick={() => goTo(4)}
        >See it in the editor</Button
      >
    {:else}
      <Button variant="ghost" onclick={() => finish('home/library')}>Go to Library</Button>
      <Button
        variant="primary"
        onclick={() =>
          finishInEditor(practiceProject ? `${editorTarget(practiceProject)}?tour=1` : 'sample?tour=1')}
      >
        {practice === 'done' ? 'Open my practice take' : 'Open the sample project'}
      </Button>
    {/if}
  </footer>
</div>
