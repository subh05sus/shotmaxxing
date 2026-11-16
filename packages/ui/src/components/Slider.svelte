<script lang="ts">
  import InfoTip from './InfoTip.svelte';

  interface Props {
    label: string;
    value: number;
    min?: number;
    max?: number;
    step?: number;
    /** Formats the value for display, e.g. `(v) => v.toFixed(1) + '×'`. */
    format?: (value: number) => string;
    info?: string;
    disabled?: boolean;
    onchange?: (value: number) => void;
  }

  let {
    label,
    value = $bindable(),
    min = 0,
    max = 1,
    step = 0.01,
    format = (v: number) => String(Math.round(v * 100) / 100),
    info,
    disabled = false,
    onchange,
  }: Props = $props();

  const id = $props.id();
  let editing = $state(false);
  let draft = $state('');
  const percent = $derived(Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100)));

  /** Accepts messy input (Postel's law): "1.5x", "150%", " 2 ", "0,5". */
  function commit() {
    editing = false;
    const cleaned = draft
      .trim()
      .replace(',', '.')
      .replace(/[×x\s]/gi, '');
    let parsed = parseFloat(cleaned.replace('%', ''));
    if (Number.isNaN(parsed)) return;
    if (cleaned.endsWith('%') && max <= 4) parsed = parsed / 100;
    value = Math.min(max, Math.max(min, Math.round(parsed / step) * step));
    onchange?.(value);
  }
</script>

<!--
  The whole bar is the slider: it fills up to the value, a thin line marks the value, and the label and the value sit
  inside it. A native range input lies over it, unseen, so dragging anywhere, the arrow keys and screen readers work
  as they do on any slider. The value can be clicked to type one.
-->
<div
  class={[
    'bar relative my-0.5 flex h-9 items-center overflow-hidden rounded-[10px] bg-fill',
    'has-[input:focus-visible]:ring-2 has-[input:focus-visible]:ring-inset has-[input:focus-visible]:ring-accent',
    disabled ? 'opacity-45' : 'hover:bg-fill-hover',
  ]}
  style:--fill="{percent}%"
>
  <span class="fill pointer-events-none absolute inset-y-0 left-0 bg-fill-strong" aria-hidden="true"></span>
  <span
    class="mark pointer-events-none absolute top-1/2 h-[18px] w-[2px] -translate-y-1/2 rounded-full bg-fg"
    aria-hidden="true"
  ></span>
  <input
    {id}
    type="range"
    {min}
    {max}
    {step}
    {disabled}
    bind:value
    oninput={() => onchange?.(value)}
    aria-valuetext={format(value)}
    class="absolute inset-0 size-full cursor-ew-resize appearance-none opacity-0 disabled:cursor-default"
  />
  <label
    for={id}
    class="pointer-events-none relative flex min-w-0 flex-1 items-center gap-1 truncate pl-3 text-callout text-fg"
  >
    {label}
    {#if info}<span class="pointer-events-auto"><InfoTip text={info} /></span>{/if}
  </label>
  {#if editing}
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="relative mr-1.5 h-6 w-16 rounded-[6px] border border-border bg-surface px-1.5 text-right text-footnote tabular"
      bind:value={draft}
      aria-label="{label} value"
      autofocus
      onblur={commit}
      onkeydown={(e) => {
        if (e.key === 'Enter') commit();
        if (e.key === 'Escape') editing = false;
      }}
    />
  {:else}
    <button
      type="button"
      class="relative mr-1.5 rounded-[6px] px-1.5 py-0.5 text-callout text-fg-muted tabular hover:bg-fill hover:text-fg"
      title="Type a value"
      {disabled}
      onclick={() => {
        draft = format(value);
        editing = true;
      }}>{format(value)}</button
    >
  {/if}
</div>

<style>
  .fill {
    width: var(--fill);
  }
  /* The line stays inside the rounded ends at 0% and 100%. */
  .mark {
    left: clamp(5px, calc(var(--fill) - 1px), calc(100% - 7px));
    transition: scale var(--dur-fast) var(--ease-spring);
  }
  .bar:active .mark {
    scale: 1 1.25;
  }
</style>
