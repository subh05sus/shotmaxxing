<script lang="ts" generics="T extends string | number">
  import type { IconData } from '../lib/icons';
  import Icon from './Icon.svelte';

  interface Option {
    value: T;
    label: string;
    icon?: IconData;
  }

  interface Props {
    options: Option[];
    value: T;
    label: string;
    size?: 'sm' | 'md';
    /** Show icons only; labels become tooltips and accessible names. */
    iconsOnly?: boolean;
    onchange?: (value: T) => void;
  }

  let { options, value = $bindable(), label, size = 'md', iconsOnly = false, onchange }: Props = $props();

  const index = $derived(
    Math.max(
      0,
      options.findIndex((o) => o.value === value),
    ),
  );

  /** The option the person last picked, which pops as the thumb lands. Nothing pops on first show. */
  let picked = $state<T | null>(null);

  function select(v: T) {
    if (v !== value) picked = v;
    value = v;
    onchange?.(v);
  }

  function onkeydown(event: KeyboardEvent) {
    const step =
      event.key === 'ArrowRight' || event.key === 'ArrowDown'
        ? 1
        : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
          ? -1
          : 0;
    if (!step) return;
    event.preventDefault();
    const next = (index + step + options.length) % options.length;
    select(options[next].value);
    (event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="radio"]')[next]?.focus();
  }
</script>

<!-- A capsule of equal-width segments, so the thumb only ever translates (a compositor-only animation), on the
     tabs' spring. -->
<div
  role="radiogroup"
  aria-label={label}
  tabindex="-1"
  {onkeydown}
  class={[
    'relative grid rounded-full bg-fill p-[2px]',
    size === 'sm' ? 'h-7' : 'h-[var(--os-control-height)] min-h-7',
  ]}
  style:grid-template-columns="repeat({options.length}, minmax(0, 1fr))"
>
  <span
    aria-hidden="true"
    class="tab-thumb absolute inset-y-[2px] left-[2px] rounded-full bg-segment-thumb shadow-[var(--elev-1)]"
    style:width="calc((100% - 4px) / {options.length})"
    style:translate="{index * 100}% 0"
  ></span>
  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={option.value === value}
      aria-label={iconsOnly ? option.label : undefined}
      title={iconsOnly ? option.label : undefined}
      tabindex={option.value === value ? 0 : -1}
      onclick={() => select(option.value)}
      class={[
        'press-tab relative z-10 inline-flex min-w-0 items-center justify-center rounded-full px-2.5 font-medium',
        size === 'sm' ? 'text-footnote' : 'text-callout',
        option.value === value ? 'text-fg' : 'text-fg-muted hover:text-fg',
      ]}
    >
      <span
        class={[
          'flex min-w-0 max-w-full items-center justify-center gap-1.5',
          option.value === picked && 'tab-pop',
        ]}
      >
        {#if option.icon}<Icon icon={option.icon} size={15} aria-hidden="true" />{/if}
        {#if !iconsOnly}<span class="truncate">{option.label}</span>{/if}
      </span>
    </button>
  {/each}
</div>
