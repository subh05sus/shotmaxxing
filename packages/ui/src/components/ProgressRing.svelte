<script lang="ts">
  /** Progress for work that takes a while (exports, model downloads). Drawn only while the work runs. */
  let {
    value,
    size = 18,
    stroke = 2.5,
    label = 'Progress',
  }: { value: number; size?: number; stroke?: number; label?: string } = $props();

  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
  const clamped = $derived(Math.min(1, Math.max(0, value)));
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 {size} {size}"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={Math.round(clamped * 100)}
  class="-rotate-90"
>
  <circle
    cx={size / 2}
    cy={size / 2}
    {r}
    fill="none"
    stroke="currentColor"
    stroke-opacity="0.22"
    stroke-width={stroke}
  />
  <circle
    cx={size / 2}
    cy={size / 2}
    {r}
    fill="none"
    stroke="currentColor"
    stroke-width={stroke}
    stroke-linecap="round"
    stroke-dasharray={c}
    stroke-dashoffset={c * (1 - clamped)}
  />
</svg>
