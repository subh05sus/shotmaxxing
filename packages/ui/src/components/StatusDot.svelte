<script lang="ts">
  /** Status as a small dot beside a word (never a coloured edge). The dot is static: no idle pulsing. */
  let {
    tone = 'neutral',
    label,
  }: { tone?: 'neutral' | 'success' | 'warning' | 'danger' | 'accent' | 'record'; label?: string } = $props();

  const colors = {
    neutral: 'bg-fg-subtle',
    success: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    accent: 'bg-accent',
    record: 'bg-record',
  };
</script>

<span class="inline-flex items-center gap-1.5">
  <span class={['size-2 shrink-0 rounded-full', colors[tone]]} aria-hidden="true"></span>
  {#if label}<span class="text-footnote text-fg-muted">{label}</span>{/if}
</span>
