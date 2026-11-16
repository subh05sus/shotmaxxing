<script lang="ts">
  import { onMount } from 'svelte';
  import {
    BookOpen,
    Bot,
    Camera,
    House,
    Keyboard,
    Monitor,
    Moon,
    RecordDot,
    Route,
    Settings as SettingsIcon,
    Sun,
  } from '@shotmaxxing/ui/icons';
  import {
    CommandPalette,
    ShortcutSheet,
    Toaster,
    applyEnv,
    env,
    matchesChord,
    setThemeSetting,
    toast,
    watchSystem,
    type ShortcutGroup,
  } from '@shotmaxxing/ui';
  import { go, route, startRouter } from './lib/router.svelte';
  import { persist, settings } from './lib/settings.svelte';
  import { allCommands, registerCommands, ui } from './lib/commands.svelte';
  import { focusHomeSearch } from './lib/search.svelte';
  import { playIris } from './lib/iris.svelte';
  import { watchTrust } from './lib/trust.svelte';
  import IrisOverlay from './components/IrisOverlay.svelte';
  import {
    isTauri,
    openWindow,
    permissionStatus,
    platform,
    powerStatus,
    setAppTheme,
    setMenuBarIcon,
  } from './lib/native';
  import WindowChrome from './routes/WindowChrome.svelte';
  import Onboarding from './routes/onboarding/Onboarding.svelte';
  import Home from './routes/home/Home.svelte';
  import Editor from './routes/editor/Editor.svelte';
  import RecordDialog from './routes/record/RecordDialog.svelte';
  import RecordingPill from './routes/record/RecordingPill.svelte';
  import TakeCard from './routes/take/TakeCard.svelte';

  const screen = $derived(route.path[0] ?? '');

  // macOS ties Screen Recording to the exact build of a preview, so after an update it can stop working, which would
  // only show when a recording fails. When it worked at an earlier launch and doesn't now, this says so, once, with
  // the way to fix it (setup's Permissions step).
  async function checkScreenRecording() {
    const state = (await permissionStatus().catch(() => [])).find((p) => p.kind === 'screen')?.state;
    if (state === 'granted') {
      settings.screenAllowedBefore = true;
    } else if (state === 'not_determined' && settings.screenAllowedBefore) {
      settings.screenAllowedBefore = false;
      toast('macOS no longer lets Shotmaxxing record the screen. That can happen after an update.', {
        tone: 'danger',
        timeout: 15_000,
        action: {
          label: 'Fix',
          run: () => {
            settings.onboardingStep = 1;
            go('onboarding');
          },
        },
      });
    }
  }

  onMount(() => {
    const stopRouter = startRouter();
    // ?theme= and ?os= let previews and screenshot checks force a look without touching saved settings.
    const params = new URLSearchParams(location.search);
    const forcedTheme = params.get('theme');
    env.themeSetting = forcedTheme === 'light' || forcedTheme === 'dark' ? forcedTheme : settings.theme;
    const stopSystem = watchSystem();
    const forcedOs = params.get('os');
    if (forcedOs === 'mac' || forcedOs === 'windows') {
      env.os = forcedOs;
      applyEnv();
    } else {
      platform().then((os) => {
        env.os = os;
        applyEnv();
      });
    }
    env.material = isTauri;
    // Allow everything lives in the app; every window shows it as it is.
    watchTrust();
    // The menu bar icon starts shown; hide it if Settings says so.
    if (isTauri && !settings.menuBarIcon) void setMenuBarIcon(false).catch(() => undefined);
    if (params.get('motion') === 'fades') env.lowPower = true;
    applyEnv();

    // The power policy can switch motion to fades; check it now and every 30 s (a cheap call, no idle drawing).
    const checkPower = () =>
      powerStatus().then((p) => {
        env.lowPower =
          settings.lowPower === 'always_on' || (settings.lowPower === 'automatic' && p.low_power);
        applyEnv();
      });
    checkPower();
    const powerTimer = setInterval(checkPower, 30_000);

    const launch = location.hash.match(/^#\/launch(?:\?(.*))?$/);
    if (launch) {
      // The app has just started: the logo builds itself quickly and opens onto the first page. Setup's welcome
      // has the full build instead (routes/onboarding). Reopened by itself from setup (to finish turning on
      // Screen Recording), it goes back to setup, on the step it was on.
      const setup = !settings.onboardingDone || new URLSearchParams(launch[1]).get('to') === 'onboarding';
      if (!setup || settings.onboardingStep > 0) playIris('launch');
      go(setup ? 'onboarding' : 'home/library');
      if (isTauri && !setup) void checkScreenRecording();
    } else if (!location.hash || location.hash === '#/' || location.hash === '#') {
      go(settings.onboardingDone ? 'home/library' : 'onboarding');
    }

    return () => {
      stopRouter();
      stopSystem();
      clearInterval(powerTimer);
    };
  });

  $effect(() => {
    // The recording pill and the take card are transparent floating windows in the app.
    if (isTauri && (screen === 'pill' || screen === 'take'))
      document.documentElement.dataset.transparent = 'true';
    else delete document.documentElement.dataset.transparent;
  });

  $effect(() => {
    // Save every settings change.
    JSON.stringify(settings);
    persist();
  });

  $effect(() => {
    // The native parts (the sidebar's material, menus, dialogs) follow the chosen theme, not only the system's.
    if (isTauri) setAppTheme(settings.theme).catch(() => {});
  });

  $effect(() => {
    return registerCommands('global', [
      {
        id: 'record',
        title: 'New recording',
        group: 'Record',
        icon: RecordDot,
        chord: 'alt+shift+5',
        run: () => openWindow('record'),
      },
      {
        id: 'screenshot',
        title: 'New screenshot',
        group: 'Record',
        icon: Camera,
        chord: 'alt+shift+4',
        keywords: 'capture picture image still',
        run: () => openWindow('record', 'screenshot'),
      },
      { id: 'home', title: 'Open Library', group: 'Go to', icon: House, run: () => openWindow('home') },
      { id: 'learn', title: 'Open guides', group: 'Go to', icon: BookOpen, run: () => go('home/learn') },
      { id: 'agents', title: 'Connect an AI agent', group: 'Go to', icon: Bot, run: () => go('home/agents') },
      {
        id: 'settings',
        title: 'Settings',
        group: 'Go to',
        icon: SettingsIcon,
        chord: 'mod+,',
        run: () => go('home/settings'),
      },
      {
        id: 'shortcuts',
        title: 'Keyboard shortcuts',
        group: 'Help',
        icon: Keyboard,
        chord: 'mod+/',
        run: () => (ui.shortcuts = true),
      },
      { id: 'onboarding', title: 'Run setup again', group: 'Help', icon: Route, run: () => go('onboarding') },
      {
        id: 'theme-system',
        title: 'Theme: match system',
        group: 'Appearance',
        icon: Monitor,
        run: () => setTheme('system'),
      },
      {
        id: 'theme-light',
        title: 'Theme: light',
        group: 'Appearance',
        icon: Sun,
        run: () => setTheme('light'),
      },
      {
        id: 'theme-dark',
        title: 'Theme: dark',
        group: 'Appearance',
        icon: Moon,
        run: () => setTheme('dark'),
      },
    ]);
  });

  function setTheme(theme: 'system' | 'light' | 'dark') {
    settings.theme = theme;
    setThemeSetting(theme);
  }

  const shortcutGroups: ShortcutGroup[] = [
    {
      title: 'Anywhere',
      items: [
        { label: 'Search, or the command palette in the editor', chord: 'mod+k' },
        { label: 'New recording', chord: 'alt+shift+5' },
        { label: 'Styled screenshot', chord: 'alt+shift+4' },
        { label: 'Keyboard shortcuts', chord: 'mod+/' },
        { label: 'Settings', chord: 'mod+,' },
      ],
    },
    {
      title: 'Playback',
      items: [
        { label: 'Play or pause', chord: 'space' },
        { label: 'Back / pause / forward', chord: 'j' },
        { label: 'Previous frame', chord: 'left' },
        { label: 'Next frame', chord: 'right' },
        { label: 'Go to start', chord: 'home' },
      ],
    },
    {
      title: 'Editing',
      items: [
        { label: 'Split at playhead', chord: 's' },
        { label: 'Add zoom at playhead', chord: 'z' },
        { label: 'Delete selection', chord: 'backspace' },
        { label: 'Undo', chord: 'mod+z' },
        { label: 'Redo', chord: 'mod+shift+z' },
      ],
    },
    {
      title: 'Project',
      items: [
        { label: 'Export', chord: 'mod+e' },
        { label: 'Version history', chord: 'mod+alt+h' },
        { label: 'Start the tour', chord: 'mod+shift+t' },
      ],
    },
  ];

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (matchesChord(e, 'mod+k', env.os)) {
      e.preventDefault();
      // Home has its search in the top bar; the other windows keep the palette.
      if (screen === 'home') focusHomeSearch();
      else ui.palette = !ui.palette;
    } else if (matchesChord(e, 'mod+/', env.os)) {
      e.preventDefault();
      ui.shortcuts = !ui.shortcuts;
    } else if (matchesChord(e, 'mod+,', env.os) && !typing) {
      e.preventDefault();
      go('home/settings');
    }
  }
</script>

<svelte:window {onkeydown} />

<WindowChrome {screen}>
  {#if screen === 'onboarding'}
    <Onboarding />
  {:else if screen === 'home'}
    <Home />
  {:else if screen === 'editor'}
    <Editor />
  {:else if screen === 'record'}
    <RecordDialog />
  {:else if screen === 'pill'}
    <RecordingPill />
  {:else if screen === 'take'}
    <TakeCard />
  {/if}
</WindowChrome>

<CommandPalette bind:open={ui.palette} commands={allCommands()} />
<ShortcutSheet bind:open={ui.shortcuts} groups={shortcutGroups} />
<Toaster />
<IrisOverlay />
