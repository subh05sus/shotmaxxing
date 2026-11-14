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
  const percent = $derived(((value - min) / (max - min)) * 100);

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

<div class="flex flex-col gap-1.5 py-1">
  <div class="flex items-center justify-between gap-2">
    <label for={id} class="flex items-center gap-1 text-callout text-fg">
      {label}
      {#if info}<InfoTip text={info} />{/if}
    </label>
    {#if editing}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="h-6 w-16 rounded-[5px] border border-border bg-surface px-1.5 text-right text-footnote tabular"
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
        class="rounded-[5px] px-1.5 py-0.5 text-footnote text-fg-muted tabular hover:bg-fill hover:text-fg"
        title="Type a value"
        {disabled}
        onclick={() => {
          draft = format(value);
          editing = true;
        }}>{format(value)}</button
      >
    {/if}
  </div>
  <input
    {id}
    type="range"
    {min}
    {max}
    {step}
    {disabled}
    bind:value
    oninput={() => onchange?.(value)}
    class="slider w-full"
    style:--fill="{percent}%"
  />
</div>

<style>
  .slider {
    appearance: none;
    height: 20px;
    background: transparent;
    margin: 0;
  }
  .slider::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 999px;
    background: linear-gradient(to right, var(--accent) var(--fill), var(--fill-strong) var(--fill));
  }
  .slider::-webkit-slider-thumb {
    appearance: none;
    width: 16px;
    height: 16px;
    margin-top: -6px;
    border-radius: 999px;
    background: var(--knob);
    box-shadow: var(--thumb-shadow);
    transition: scale var(--dur-fast) var(--ease-spring);
  }
  .slider:active::-webkit-slider-thumb {
    scale: 1.15;
  }
  .slider:disabled {
    opacity: 0.45;
  }
</style>
