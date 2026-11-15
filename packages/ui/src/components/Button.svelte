<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { IconData } from '../lib/icons';
  import Icon from './Icon.svelte';
  import Spinner from './Spinner.svelte';

  type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
  type Size = 'sm' | 'md' | 'lg';

  interface Props extends HTMLButtonAttributes {
    variant?: Variant;
    size?: Size;
    icon?: IconData;
    iconEnd?: IconData;
    loading?: boolean;
    /** Fill the width of the container. */
    block?: boolean;
    children?: Snippet;
  }

  let {
    variant = 'secondary',
    size = 'md',
    icon,
    iconEnd,
    loading = false,
    block = false,
    disabled,
    type = 'button',
    class: className = '',
    children,
    ...rest
  }: Props = $props();

  const sizes: Record<Size, string> = {
    sm: 'h-7 px-2.5 text-footnote gap-1.5',
    md: 'h-[var(--os-control-height)] min-h-7 px-3 text-callout gap-1.5',
    lg: 'h-10 px-4 text-body gap-2',
  };
  const variants: Record<Variant, string> = {
    primary: 'bg-accent text-accent-fg hover:bg-accent-hover font-semibold shadow-[var(--top-light)]',
    secondary: 'bg-fill text-fg hover:bg-fill-hover border border-border',
    ghost: 'text-fg-muted hover:text-fg hover:bg-fill',
    danger: 'bg-danger text-record-fg hover:opacity-90 font-semibold',
  };
  const iconSize = $derived(size === 'lg' ? 18 : size === 'sm' ? 14 : 16);
</script>

<button
  {type}
  disabled={disabled || loading}
  aria-busy={loading || undefined}
  class={[
    'press relative inline-flex select-none items-center justify-center whitespace-nowrap rounded-control font-medium',
    'transition-colors duration-[var(--dur-fast)] disabled:opacity-45',
    sizes[size],
    variants[variant],
    block && 'w-full',
    className,
  ]}
  {...rest}
>
  {#if loading}
    <Spinner size={iconSize} />
  {:else if icon}
    <Icon {icon} size={iconSize} aria-hidden="true" />
  {/if}
  {@render children?.()}
  {#if iconEnd}
    <Icon icon={iconEnd} size={iconSize} aria-hidden="true" />
  {/if}
</button>
