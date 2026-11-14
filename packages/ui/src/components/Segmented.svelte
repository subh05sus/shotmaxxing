<script lang="ts" generics="T extends string | number">
  import type { Component } from 'svelte';

  interface Option {
    value: T;
    label: string;
    icon?: Component<any>;
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

  function select(v: T) {
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

<!-- Equal-width segments, so the selection thumb only ever translates (a compositor-only animation). -->
<div
  role="radiogroup"
  aria-label={label}
  tabindex="-1"
  {onkeydown}
  class={[
    'relative grid rounded-control bg-fill p-[2px]',
    size === 'sm' ? 'h-7' : 'h-[var(--os-control-height)] min-h-7',
  ]}
  style:grid-template-columns="repeat({options.length}, minmax(0, 1fr))"
>
  <span
    aria-hidden="true"
    class="absolute inset-y-[2px] left-[2px] rounded-[calc(var(--radius-control)-2px)] bg-surface shadow-[var(--elev-1)] transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]"
    style:width="calc((100% - 4px) / {options.length})"
    style:translate="{index * 100}% 0"
  ></span>
  {#each options as option (option.value)}
    {@const Icon = option.icon}
    <button
      type="button"
      role="radio"
      aria-checked={option.value === value}
      aria-label={iconsOnly ? option.label : undefined}
      title={iconsOnly ? option.label : undefined}
      tabindex={option.value === value ? 0 : -1}
      onclick={() => select(option.value)}
      class={[
        'relative z-10 inline-flex min-w-0 items-center justify-center gap-1.5 truncate rounded-[calc(var(--radius-control)-2px)] px-2 font-medium transition-colors duration-[var(--dur-fast)]',
        size === 'sm' ? 'text-footnote' : 'text-callout',
        option.value === value ? 'text-fg' : 'text-fg-muted hover:text-fg',
      ]}
    >
      {#if Icon}<Icon size={15} strokeWidth={2} aria-hidden="true" />{/if}
      {#if !iconsOnly}<span class="truncate">{option.label}</span>{/if}
    </button>
  {/each}
</div>
