<script lang="ts">
  import { Bot, Camera, Ellipsis } from '@shotmaxxing/ui/icons';
  import { Icon, Menu, type MenuItem } from '@shotmaxxing/ui';
  import Scene from '../../components/Scene.svelte';
  import { wallpaper } from '../../lib/wallpapers';
  import { relativeTime, timecode } from '../../lib/format';
  import type { LibraryItem } from '../../lib/sample';

  interface Props {
    item: LibraryItem;
    selected: boolean;
    index: number;
    menu: MenuItem[];
    onselect: () => void;
    onopen: () => void;
  }

  let { item, selected, index, menu, onselect, onopen }: Props = $props();

  let width = $state(0);
  let scrub = $state<number | null>(null);
  let video = $state<HTMLVideoElement | null>(null);

  // Real recordings show a frame of the video; hover scrubbing seeks it. Nothing plays.
  $effect(() => {
    if (video && video.readyState >= 1) video.currentTime = t;
  });
  let menuButton: HTMLButtonElement | undefined = $state();

  // Hover scrub: the pointer is the playhead. Only transforms change, and only while the pointer moves.
  const t = $derived(scrub === null ? item.duration * 0.62 : scrub * item.duration);
  const push = $derived(scrub === null ? 1 : 1 + 0.14 * Math.sin(Math.PI * scrub));

  function onpointermove(e: PointerEvent) {
    if (item.kind !== 'recording') return;
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    scrub = Math.min(1, Math.max(0, (e.clientX - box.left) / box.width));
  }
</script>

<div class="group relative rise-in" style="--i: {index}">
  <button
    type="button"
    class="press-card block w-full rounded-card text-left"
    aria-label="{item.title}, {item.kind === 'screenshot' ? 'screenshot' : timecode(item.duration)}"
    aria-pressed={selected}
    onclick={onselect}
    ondblclick={onopen}
    onkeydown={(e) => e.key === 'Enter' && onopen()}
    oncontextmenu={(e) => {
      e.preventDefault();
      onselect();
      menuButton?.click();
    }}
  >
    <div
      bind:clientWidth={width}
      class={[
        'relative aspect-[16/10] overflow-hidden rounded-card shadow-[var(--elev-1)] transition-shadow duration-[var(--dur-fast)]',
        selected ? 'ring-2 ring-accent ring-offset-2 ring-offset-bg' : 'group-hover:shadow-[var(--elev-2)]',
      ]}
      style:background={wallpaper(item.wallpaper).css}
      role="presentation"
      {onpointermove}
      onpointerleave={() => (scrub = null)}
    >
      <div
        class="absolute left-[7%] top-[8%] aspect-[16/10] w-[86%] overflow-hidden rounded-[5px] shadow-[var(--elev-2)] transition-transform duration-[var(--dur-base)] ease-[var(--ease-out)]"
        style:transform="scale({push})"
      >
        {#if item.media}
          <video
            bind:this={video}
            src={item.media}
            class="size-full object-cover"
            muted
            playsinline
            preload="metadata"
            disablepictureinpicture
            onloadedmetadata={() => video && (video.currentTime = t)}
          ></video>
        {:else if width}<Scene variant={item.scene} width={width * 0.86} {t} />{/if}
      </div>
      {#if scrub !== null}
        <span
          class="pointer-events-none absolute inset-y-0 w-0.5 bg-accent"
          style:left="{scrub * 100}%"
          aria-hidden="true"
        ></span>
        <span
          class="pointer-events-none absolute bottom-2 left-2 rounded-[5px] bg-scrim px-1.5 py-0.5 text-caption font-semibold text-fg-on-stage tabular"
        >
          {timecode(t)}
        </span>
      {/if}
      {#if item.kind === 'screenshot'}
        <span
          class="absolute bottom-2 right-2 flex items-center gap-1 rounded-[5px] bg-scrim px-1.5 py-0.5 text-caption font-semibold text-fg-on-stage"
        >
          <Icon icon={Camera} size={11} aria-hidden="true" /> Screenshot
        </span>
      {:else}
        <span
          class="absolute bottom-2 right-2 rounded-[5px] bg-scrim px-1.5 py-0.5 text-caption font-semibold text-fg-on-stage tabular"
        >
          {timecode(item.duration)}
        </span>
      {/if}
      {#if item.agent}
        <span
          class="absolute left-2 top-2 flex items-center gap-1 rounded-[5px] bg-scrim px-1.5 py-0.5 text-caption font-semibold text-fg-on-stage"
        >
          <Icon icon={Bot} size={11} aria-hidden="true" />
          {item.agent}
        </span>
      {/if}
    </div>
    <div class="mt-2 px-0.5">
      <p class="truncate text-callout font-medium">{item.title}</p>
      <p class="mt-0.5 truncate text-footnote text-fg-muted">
        {item.app} · {relativeTime(item.created)} · {item.sizeMb} MB
      </p>
    </div>
  </button>

  <div
    class="absolute right-1.5 top-1.5 opacity-0 transition-opacity duration-[var(--dur-fast)] focus-within:opacity-100 group-hover:opacity-100"
  >
    <Menu items={menu} label="Actions for {item.title}" align="end">
      {#snippet trigger({ props })}
        <button
          bind:this={menuButton}
          type="button"
          aria-label="More actions for {item.title}"
          class="press-icon grid size-7 place-items-center rounded-[7px] bg-scrim text-fg-on-stage"
          {...props}
        >
          <Icon icon={Ellipsis} size={15} aria-hidden="true" />
        </button>
      {/snippet}
    </Menu>
  </div>
</div>
