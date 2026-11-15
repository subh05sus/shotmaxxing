<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { IconData } from '../lib/icons';
  import type { Side } from '../lib/position';
  import Icon from './Icon.svelte';
  import Tooltip from './Tooltip.svelte';

  interface Props extends HTMLButtonAttributes {
    icon: IconData;
    /** Required: the accessible name and the tooltip. */
    label: string;
    chord?: string;
    side?: Side;
    size?: 'sm' | 'md' | 'lg';
    active?: boolean;
    tone?: 'default' | 'on-stage';
  }

  let {
    icon,
    label,
    chord,
    side = 'bottom',
    size = 'md',
    active = false,
    tone = 'default',
    class: className = '',
    ...rest
  }: Props = $props();

  // The hit area is larger than the glyph (Fitts's law): 28 px minimum, 32 px for md and 40 px for lg.
  const box = { sm: 'size-7', md: 'size-8', lg: 'size-10' };
  const glyph = { sm: 15, md: 17, lg: 20 };
</script>

<Tooltip {label} {chord} {side}>
  <button
    type="button"
    aria-label={label}
    aria-pressed={active || undefined}
    class={[
      'press-icon inline-flex shrink-0 items-center justify-center rounded-control transition-colors duration-[var(--dur-fast)] disabled:opacity-40',
      box[size],
      tone === 'on-stage'
        ? 'text-fg-on-stage/80 hover:bg-on-stage-hover hover:text-fg-on-stage'
        : active
          ? 'bg-accent-tint text-accent-text'
          : 'text-fg-muted hover:bg-fill hover:text-fg',
      className,
    ]}
    {...rest}
  >
    <Icon {icon} size={glyph[size]} aria-hidden="true" />
  </button>
</Tooltip>
