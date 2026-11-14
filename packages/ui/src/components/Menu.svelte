<script lang="ts" module>
  import type { Component } from 'svelte';

  export interface MenuItem {
    label: string;
    icon?: Component<any>;
    chord?: string;
    danger?: boolean;
    disabled?: boolean;
    /** Draws a divider above this item. */
    separated?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Align, Side } from '../lib/position';
  import Popover from './Popover.svelte';
  import Keycap from './Keycap.svelte';

  interface Props {
    items: MenuItem[];
    label: string;
    side?: Side;
    align?: Align;
    /** Renders the trigger. Spread `props` onto a button. */
    trigger: Snippet<[{ props: Record<string, unknown>; open: boolean }]>;
  }

  let { items, label, side = 'bottom', align = 'start', trigger }: Props = $props();

  let open = $state(false);
  let anchorEl: HTMLElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();
  let active = $state(-1);

  const triggerProps = $derived({
    'aria-haspopup': 'menu',
    'aria-expanded': open,
    onclick: (e: MouseEvent) => {
      anchorEl = e.currentTarget as HTMLElement;
      open = !open;
      active = -1;
    },
    onkeydown: (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        anchorEl = e.currentTarget as HTMLElement;
        open = true;
        active = items.findIndex((i) => !i.disabled);
      }
    },
  });

  $effect(() => {
    if (open && active >= 0) list?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')[active]?.focus();
  });

  function move(step: number) {
    const enabled = items.map((item, i) => (item.disabled ? -1 : i)).filter((i) => i >= 0);
    if (!enabled.length) return;
    const at = enabled.indexOf(active);
    active = enabled[(at + step + enabled.length) % enabled.length];
  }

  function choose(item: MenuItem) {
    if (item.disabled) return;
    open = false;
    anchorEl?.focus();
    item.run();
  }
</script>

{@render trigger({ props: triggerProps, open })}

<Popover bind:open anchor={anchorEl} {side} {align} role="menu" {label} class="min-w-52 p-1">
  <div
    bind:this={list}
    role="none"
    onkeydown={(e) => {
      if (e.key === 'ArrowDown') (e.preventDefault(), move(1));
      if (e.key === 'ArrowUp') (e.preventDefault(), move(-1));
    }}
  >
    {#each items as item, i (item.label)}
      {@const Icon = item.icon}
      {#if item.separated}<div class="mx-2 my-1 h-px bg-border" role="separator"></div>{/if}
      <button
        type="button"
        role="menuitem"
        disabled={item.disabled}
        tabindex={i === active ? 0 : -1}
        onclick={() => choose(item)}
        onpointermove={() => (active = i)}
        class={[
          'ring-inside flex h-8 w-full items-center gap-2.5 rounded-[6px] px-2 text-left text-callout disabled:opacity-40',
          i === active
            ? item.danger
              ? 'bg-danger text-record-fg'
              : 'bg-accent text-accent-fg'
            : item.danger
              ? 'text-danger'
              : 'text-fg',
        ]}
      >
        {#if Icon}<Icon size={15} strokeWidth={2} aria-hidden="true" />{/if}
        <span class="flex-1 truncate">{item.label}</span>
        {#if item.chord}<Keycap chord={item.chord} tone={i === active ? 'on-accent' : 'default'} />{/if}
      </button>
    {/each}
  </div>
</Popover>
