<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import type { Menu as NativeMenuType } from '@tauri-apps/api/menu';
  import { Ellipsis, RotateCcw, Stop, Trash } from '@shotmaxxing/ui/icons';
  import { Icon, Menu, Spinner, chordLabel, env, toast, type MenuItem } from '@shotmaxxing/ui';
  import {
    discardRecording,
    editorTarget,
    handOff,
    isTauri,
    restartRecording,
    showError,
    stopRecording,
  } from '../../lib/native';
  import { timecode } from '../../lib/format';

  /**
   * The only thing on screen while recording. It is left out of the recording, has no animation of its own, and
   * updates its timer once a second, so it costs next to nothing.
   */
  let elapsed = $state(0);
  let busy = $state<'saving' | 'restarting' | 'discarding' | null>(null);
  let started = performance.now();

  const timer = setInterval(() => (elapsed = (performance.now() - started) / 1000), 1000);
  onDestroy(() => clearInterval(timer));

  // Stopping from the menu bar or with ⌥⇧5 happens in the app; the pill just says it's saving until it closes.
  onMount(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    void import('@tauri-apps/api/webviewWindow').then(({ getCurrentWebviewWindow }) =>
      getCurrentWebviewWindow()
        .listen('recording-saving', () => {
          busy = 'saving';
          clearInterval(timer);
        })
        .then((stop) => (unlisten = stop)),
    );
    return () => unlisten?.();
  });

  function failed(error: unknown) {
    busy = null;
    const text = error instanceof Error ? error.message : String(error);
    // The pill's window is only as big as the pill, so a toast would be cut off: the app says it in an alert.
    if (isTauri) void showError(text);
    else toast(text, { tone: 'danger', timeout: 8000 });
  }

  async function stop() {
    if (busy) return;
    busy = 'saving';
    clearInterval(timer);
    try {
      // Anything the recording wants to say (dropped frames, a microphone that wasn't allowed) shows in the editor;
      // this window closes straight away.
      const done = await stopRecording();
      await handOff('editor', editorTarget(done.project));
    } catch (error) {
      failed(error);
    }
  }

  const more: MenuItem[] = [
    {
      label: 'Restart recording',
      icon: RotateCcw,
      run: async () => {
        busy = 'restarting';
        try {
          await restartRecording();
          started = performance.now();
          // The timer back at 0:00 says it started over; a toast would have no room in this window.
          elapsed = 0;
          busy = null;
        } catch (error) {
          failed(error);
        }
      },
    },
    {
      label: 'Discard recording',
      icon: Trash,
      danger: true,
      run: async () => {
        busy = 'discarding';
        try {
          await discardRecording();
          await handOff('home', 'home/library');
        } catch (error) {
          failed(error);
        }
      },
    },
  ];

  // The pill's window is only as big as the pill and its shadow, so in the app its options open in a native menu,
  // which can reach past the window's edges. A browser preview has room for the page's own menu.
  let native: Promise<NativeMenuType> | undefined;
  onDestroy(() => void native?.then((menu) => menu.close()));

  async function showMore(button: HTMLElement) {
    if (busy) return;
    // In title case, like the menu-bar icon's menu and other native menus.
    native ??= import('@tauri-apps/api/menu').then(({ Menu: NativeMenu }) =>
      NativeMenu.new({
        items: more.map((item) => ({
          text: item.label.replace(/\b\w/g, (c) => c.toUpperCase()),
          action: () => item.run(),
        })),
      }),
    );
    const { LogicalPosition } = await import('@tauri-apps/api/dpi');
    const r = button.getBoundingClientRect();
    await (await native).popup(new LogicalPosition(r.left, r.bottom + 4));
  }

  const stopTip = `Stop and edit (${chordLabel('alt+shift+5', env.os)})`;
</script>

{#snippet moreButton(props: Record<string, unknown>)}
  <button
    type="button"
    aria-label="More"
    title="More"
    class="press-icon grid size-8 place-items-center rounded-full text-fg-muted hover:bg-fill hover:text-fg"
    {...props}
  >
    <Icon icon={Ellipsis} size={16} aria-hidden="true" />
  </button>
{/snippet}

<!-- 16 pt above the pill and 24 below: what its shadow needs, so the window's edge doesn't cut it. -->
<div class="flex h-full items-start justify-center pt-4">
  <div
    class="flex h-11 items-center gap-1 rounded-full border border-border bg-elevated py-1 pl-3.5 pr-1.5 text-fg shadow-[var(--elev-float)]"
    role="toolbar"
    aria-label="Recording"
  >
    <span class={['size-2.5 rounded-full', busy ? 'bg-fg-subtle' : 'bg-record']} aria-hidden="true"></span>
    <span class="ml-1.5 w-12 text-callout font-semibold tabular" aria-live="off">{timecode(elapsed)}</span>
    <span class="mx-1 h-5 w-px bg-border" aria-hidden="true"></span>
    <!-- A native tooltip: the page's own would have no room in this window. -->
    <button
      type="button"
      aria-label="Stop and edit"
      title={stopTip}
      class="press grid h-8 place-items-center rounded-full bg-accent px-3 text-callout font-semibold text-accent-fg"
      disabled={busy !== null}
      onclick={stop}
    >
      <span class="flex items-center gap-1.5">
        {#if busy}
          <Spinner size={12} />
          {busy === 'saving' ? 'Saving' : busy === 'restarting' ? 'Restarting' : 'Discarding'}
        {:else}
          <Icon icon={Stop} size={11} fill="currentColor" aria-hidden="true" /> Stop
        {/if}
      </span>
    </button>
    {#if isTauri}
      {@render moreButton({
        'aria-haspopup': 'menu',
        disabled: busy !== null,
        onclick: (e: MouseEvent) => void showMore(e.currentTarget as HTMLElement),
      })}
    {:else}
      <Menu items={more} label="Recording options" align="end">
        {#snippet trigger({ props })}{@render moreButton(props)}{/snippet}
      </Menu>
    {/if}
  </div>
</div>
