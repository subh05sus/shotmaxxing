<script lang="ts">
  import { cursorShape } from '../../lib/backdrops';

  /** One cursor shape (crates/render/assets/cursors.json) drawn on its own, for the cursor picker. */
  let { style, size = 28 }: { style: string; size?: number } = $props();

  const shape = $derived(cursorShape(style));
  // Round cursors sit in a square box; arrows and the hand in their 20 × 28 one.
  const round = $derived(shape.layers.every((l) => l.circle));
  const TOKEN = { light: 'var(--cursor-light)', dark: 'var(--cursor-dark)' } as const;
</script>

<svg
  width={round ? size * 0.8 : size * (20 / 28)}
  height={round ? size * 0.8 : size}
  viewBox={round ? '0 0 20 20' : '-1 -1 22 30'}
  class="overflow-visible drop-shadow-[0_1px_1px_oklch(0_0_0/0.25)]"
  aria-hidden="true"
>
  {#each shape.layers as layer, i (i)}
    {#if layer.circle}
      <circle
        cx={layer.circle[0]}
        cy={layer.circle[1]}
        r={layer.circle[2]}
        fill={layer.fill ? TOKEN[layer.fill] : 'none'}
        stroke={layer.stroke ? TOKEN[layer.stroke] : 'none'}
        stroke-width={layer.width ?? 0}
        opacity={layer.opacity ?? 1}
      />
    {:else}
      <path
        d={layer.d}
        fill={layer.fill ? TOKEN[layer.fill] : 'none'}
        stroke={layer.stroke ? TOKEN[layer.stroke] : 'none'}
        stroke-width={layer.width ?? 0}
        stroke-linejoin="round"
        opacity={layer.opacity ?? 1}
      />
    {/if}
  {/each}
</svg>
