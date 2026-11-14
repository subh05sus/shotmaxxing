<script lang="ts">
  import { env } from '../lib/env.svelte';

  interface Props {
    checked?: boolean;
    label: string;
    /** Hide the label visually (it stays the accessible name). */
    hideLabel?: boolean;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
  }

  let { checked = $bindable(false), label, hideLabel = true, disabled = false, onchange }: Props = $props();

  // macOS switches are small capsules; Windows toggles are larger with a dot that grows when on.
  const mac = $derived(env.os === 'mac');

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={hideLabel ? label : undefined}
  {disabled}
  onclick={toggle}
  class="group inline-flex items-center gap-2 py-1.5 disabled:opacity-45"
>
  <span
    class={[
      'relative inline-flex shrink-0 items-center rounded-full transition-colors duration-[var(--dur-base)]',
      mac ? 'h-[18px] w-[30px]' : 'h-5 w-10 border',
      checked ? 'bg-accent border-transparent' : mac ? 'bg-fill-strong' : 'border-fg-muted bg-transparent',
    ]}
  >
    <span
      class={[
        'absolute rounded-full transition-[translate,scale,background-color] duration-[var(--dur-base)] ease-[var(--ease-spring)]',
        mac
          ? 'left-[2px] size-[14px] bg-knob shadow-[var(--knob-shadow)]'
          : 'left-[3px] size-3 group-active:scale-110',
        !mac && (checked ? 'bg-accent-fg' : 'bg-fg-muted'),
      ]}
      style:translate={checked ? (mac ? '12px 0' : '20px 0') : '0 0'}
    ></span>
  </span>
  {#if !hideLabel}<span class="text-callout">{label}</span>{/if}
</button>
