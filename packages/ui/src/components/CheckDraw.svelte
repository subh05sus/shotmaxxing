<script lang="ts">
  /** A check mark that draws itself once, for "Saved", "Exported" and finished steps. */
  let { size = 16, class: className = '' }: { size?: number; class?: string } = $props();
</script>

<svg
  class={['shrink-0', className]}
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  aria-hidden="true"
>
  <path
    class="check-draw"
    d="M5 12.5l4.5 4.5L19 7.5"
    stroke="currentColor"
    stroke-width="2.6"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>
