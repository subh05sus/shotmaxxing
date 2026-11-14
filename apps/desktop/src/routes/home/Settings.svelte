<script lang="ts">
  import { ExternalLink, FolderOpen, Monitor, Moon, Sun } from '@lucide/svelte';
  import {
    Button,
    Group,
    Keycap,
    Row,
    Segmented,
    Switch,
    applyEnv,
    env,
    setThemeSetting,
    toast,
  } from '@shotmaxxing/ui';
  import { onMount } from 'svelte';
  import { resetSettings, settings, type LowPowerSetting } from '../../lib/settings.svelte';
  import { go } from '../../lib/router.svelte';
  import { ui } from '../../lib/commands.svelte';
  import {
    chooseProjectsFolder,
    defaultProjectsFolder,
    isTauri,
    launchAtLogin,
    openLink,
    setLaunchAtLogin,
    setMenuBarIcon,
  } from '../../lib/native';

  const mac = $derived(env.os === 'mac');
  const version = import.meta.env.VITE_APP_VERSION;

  let defaultFolder = $state(env.os === 'windows' ? '~\\Videos\\Shotmaxxing' : '~/Movies/Shotmaxxing');

  onMount(async () => {
    defaultFolder = await defaultProjectsFolder(defaultFolder).catch(() => defaultFolder);
    // The system decides whether the app opens at login; show what it says.
    if (isTauri) settings.launchAtLogin = await launchAtLogin().catch(() => settings.launchAtLogin);
  });

  function changeMenuBarIcon(visible: boolean) {
    setMenuBarIcon(visible).catch((error) => {
      settings.menuBarIcon = !visible;
      toast(`Couldn’t change the ${mac ? 'menu bar' : 'tray'} icon: ${error}`, { tone: 'danger' });
    });
  }

  function changeLaunchAtLogin(enabled: boolean) {
    setLaunchAtLogin(enabled).catch((error) => {
      settings.launchAtLogin = !enabled;
      toast(`Couldn’t change Open at login: ${error}`, { tone: 'danger' });
    });
  }

  async function chooseFolder() {
    try {
      const folder = await chooseProjectsFolder(settings.saveFolder);
      if (folder) settings.saveFolder = folder === defaultFolder ? '' : folder;
    } catch (error) {
      toast(`Couldn’t choose that folder: ${error}`, { tone: 'danger' });
    }
  }

  const stopHow = $derived(
    settings.menuBarIcon
      ? `Stop from the ${mac ? 'menu bar' : 'tray'} or with ${mac ? '⌥⇧5' : 'Alt+Shift+5'}.`
      : `Stop with ${mac ? '⌥⇧5' : 'Alt+Shift+5'}.`,
  );

  function setLowPower(value: LowPowerSetting) {
    settings.lowPower = value;
    env.lowPower = value === 'always_on';
    applyEnv();
  }

  const shortcuts = [
    { label: 'New recording', chord: 'alt+shift+5' },
    { label: 'Styled screenshot', chord: 'alt+shift+4' },
    { label: 'Stop recording', chord: 'alt+shift+5' },
    { label: 'Stop an agent take (press it three times)', chord: 'escape' },
  ];
</script>

<div class="scroll-room h-full overflow-y-auto px-5 pb-10">
  <div class="mx-auto flex max-w-[680px] flex-col gap-6">
    <Group title="Appearance">
      <Row label="Theme" description="The canvas around your video stays dark in every theme.">
        <div class="w-64">
          <Segmented
            label="Theme"
            size="sm"
            value={settings.theme}
            onchange={(v) => {
              settings.theme = v;
              setThemeSetting(v);
            }}
            options={[
              { value: 'system', label: 'System', icon: Monitor },
              { value: 'light', label: 'Light', icon: Sun },
              { value: 'dark', label: 'Dark', icon: Moon },
            ]}
          />
        </div>
      </Row>
      <Row
        label={mac ? 'Show in the menu bar' : 'Show in the system tray'}
        description={settings.menuBarIcon
          ? 'Start recordings and see agent takes from there.'
          : mac
            ? 'Shotmaxxing keeps its Dock icon instead.'
            : 'Closing the last window quits Shotmaxxing.'}
      >
        <Switch
          bind:checked={settings.menuBarIcon}
          onchange={changeMenuBarIcon}
          label={mac ? 'Show in the menu bar' : 'Show in the system tray'}
        />
      </Row>
      <Row label="Open at login">
        <Switch bind:checked={settings.launchAtLogin} onchange={changeLaunchAtLogin} label="Open at login" />
      </Row>
    </Group>

    <Group title="Recording">
      <Row label="Count down before recording" description="Three seconds to get ready.">
        <Switch bind:checked={settings.countdown} label="Count down before recording" />
      </Row>
      <Row
        label="Show the recording pill"
        description={settings.showPill
          ? 'A small pill with the timer and Stop. It never appears in the video.'
          : stopHow}
      >
        <Switch bind:checked={settings.showPill} label="Show the recording pill" />
      </Row>
      {#if mac}
        <Row
          label="Hide desktop icons while recording"
          description="Screen recordings keep your wallpaper and leave the icons out. Finder windows opened during the recording stay out too."
        >
          <Switch bind:checked={settings.hideDesktopIcons} label="Hide desktop icons while recording" />
        </Row>
      {/if}
      <Row
        label="Frame rate"
        info="Adaptive frame rate records new frames only when something on screen changes, up to the limit you pick. A still screen costs almost nothing."
      >
        <div class="w-64">
          <Segmented
            label="Frame rate"
            size="sm"
            bind:value={settings.frameRate}
            options={[
              { value: 'adaptive30', label: 'Up to 30' },
              { value: 'adaptive60', label: 'Up to 60' },
              { value: 'adaptive120', label: 'Up to 120' },
            ]}
          />
        </div>
      </Row>
      <Row label="Save recordings to" description={settings.saveFolder || defaultFolder}>
        <div class="flex items-center gap-1">
          {#if settings.saveFolder}
            <Button size="sm" variant="ghost" onclick={() => (settings.saveFolder = '')}>Use default</Button>
          {/if}
          <Button size="sm" icon={FolderOpen} disabled={!isTauri} onclick={chooseFolder}>Change…</Button>
        </div>
      </Row>
    </Group>

    <Group
      title="Performance"
      footer="Recording quality never drops: Low power only changes previews and when background work runs."
    >
      <Row
        label="Low power"
        description={settings.lowPower === 'automatic'
          ? `Turns on by itself on battery saver, when your ${mac ? 'Mac' : 'PC'} gets warm, or with Reduce Motion.`
          : settings.lowPower === 'always_on'
            ? 'Previews run at 30 fps, animations become fades, and AI jobs wait for power.'
            : 'Always full speed, even when warm. Recording is protected either way.'}
      >
        <div class="w-64">
          <Segmented
            label="Low power"
            size="sm"
            value={settings.lowPower}
            onchange={setLowPower}
            options={[
              { value: 'automatic', label: 'Automatic' },
              { value: 'always_on', label: 'On' },
              { value: 'always_off', label: 'Off' },
            ]}
          />
        </div>
      </Row>
    </Group>

    <Group title="Shortcuts" footer="Every editor shortcut is in the shortcut sheet.">
      {#each shortcuts as s (s.label)}
        <Row label={s.label}><Keycap chord={s.chord} size="md" /></Row>
      {/each}
      <Row label="All shortcuts">
        <Button size="sm" onclick={() => (ui.shortcuts = true)}>Show <Keycap chord="mod+/" /></Button>
      </Row>
    </Group>

    <Group title="Privacy">
      <Row
        label="Everything stays on this {mac ? 'Mac' : 'PC'}"
        description="Recordings, transcripts and AI helpers run locally. Shotmaxxing sends nothing anywhere, and has no account to sign in to."
      />
      <Row
        label="Check for updates"
        description="Asks GitHub once a day whether a new version exists. Nothing else is sent."
      >
        <Switch bind:checked={settings.checkUpdates} label="Check for updates" />
      </Row>
    </Group>

    <Group title="Help">
      <Row label="Run setup again" description="Permissions, agents and a practice take.">
        <Button size="sm" onclick={() => go('onboarding')}>Run setup</Button>
      </Row>
      <Row label="Replay the editor tour">
        <Button
          size="sm"
          onclick={() => {
            settings.tourDone = false;
            go('editor/sample?tour=1');
          }}>Replay tour</Button
        >
      </Row>
      <Row label="Reset all settings" description="Your recordings are not touched.">
        <Button
          size="sm"
          variant="ghost"
          onclick={() => {
            const before = JSON.parse(JSON.stringify(settings));
            resetSettings();
            setThemeSetting(settings.theme);
            toast('Settings reset', {
              action: { label: 'Undo', run: () => Object.assign(settings, before) },
            });
          }}>Reset</Button
        >
      </Row>
    </Group>

    <Group title="About">
      <Row label="Shotmaxxing {version}" description="Free and open source under the GNU AGPL 3.0.">
        <Button
          size="sm"
          variant="ghost"
          iconEnd={ExternalLink}
          onclick={() => openLink('https://github.com/subh05sus/shotmaxxing')}>Source code</Button
        >
      </Row>
    </Group>
  </div>
</div>
