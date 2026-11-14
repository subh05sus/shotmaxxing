<script lang="ts">
  /** The record mark: a solid dot in the current text colour, sized like a Lucide icon so buttons can use it. */
  let { size = 16 }: { size?: number; strokeWidth?: number; class?: string } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
  <circle cx="8" cy="8" r="5" fill="currentColor" />
</svg>
