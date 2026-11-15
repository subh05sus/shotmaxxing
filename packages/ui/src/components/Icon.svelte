<script lang="ts">
  /**
   * Draws an icon from `lib/icons` at any size, in the colour of the text around it. Every icon gets the same
   * stroke, 1.75 on the 24 px grid, so Pikaicons and Hugeicons read as one set; small icons are drawn a little
   * heavier, never thinner than about 1.15 px on screen, so they stay crisp.
   */
  import type { SVGAttributes } from 'svelte/elements';
  import { iconActs, type IconData, type IconNode } from '../lib/icons';

  interface Props extends Omit<SVGAttributes<SVGSVGElement>, 'fill' | 'stroke-width'> {
    icon: IconData;
    size?: number;
    /** In the grid's units. Leave it alone unless a design calls for a different weight. */
    strokeWidth?: number;
    /** Fill the shapes as well, for a solid glyph such as Stop or Record. */
    fill?: string;
  }

  let { icon, size = 24, strokeWidth = 1.75, fill, ...rest }: Props = $props();

  const stroke = $derived(Math.max(strokeWidth, (1.15 * 24) / size));
  const act = $derived(iconActs.get(icon));
  // Decorative unless it's given a name; a named icon is announced as an image.
  const named = $derived(rest['aria-label'] != null || rest['aria-labelledby'] != null);

  // Both sets publish React-style names (strokeWidth); SVG wants stroke-width. React's `key` isn't an attribute.
  function attributes(attrs: IconNode[1]): Record<string, string | number> {
    const out: Record<string, string | number> = {};
    for (const [name, value] of Object.entries(attrs)) {
      if (name === 'key') continue;
      out[name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)] = value;
    }
    if ('stroke-width' in out) out['stroke-width'] = stroke;
    return out;
  }
</script>

{#snippet part(node: IconNode)}
  <svelte:element this={node[0]} xmlns="http://www.w3.org/2000/svg" {...attributes(node[1])}>
    {#each node[2] ?? [] as child, i (i)}{@render part(child)}{/each}
  </svelte:element>
{/snippet}

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={fill ?? 'none'}
  data-act={act}
  role={named ? 'img' : undefined}
  aria-hidden={named ? undefined : 'true'}
  {...rest}
>
  {#each icon as node, i (i)}{@render part(node)}{/each}
</svg>
