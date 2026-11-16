import { createElement, type SVGProps } from 'react';
import { cn } from '@/lib/utils';

/** An icon's drawing as Pikaicons publish it: SVG elements as [tag, attributes, children]. */
type IconNode = readonly [
  tag: string,
  attrs: { readonly [name: string]: string | number },
  children?: readonly IconNode[],
];
export type IconData = readonly IconNode[];

/** What an icon acts out, once, when the control it sits in is hovered (globals.css, "Icons that act"). */
export type IconAct = 'pop' | 'drop' | 'nudge' | 'tilt' | 'spin' | 'lift';

interface Props extends Omit<SVGProps<SVGSVGElement>, 'fill'> {
  icon: IconData;
  size?: number;
  act?: IconAct;
  /** Fill the shapes as well, for a solid glyph. */
  fill?: string;
}

/**
 * Draws a Pikaicon (https://pikaicons.com, MIT) the way the app does (packages/ui Icon.svelte): one stroke, 1.75 on
 * the 24 px grid, never thinner than about 1.15 px on screen, in the colour of the text around it.
 */
export function Icon({ icon, size = 20, act, fill, className, ...rest }: Props) {
  const stroke = Math.max(1.75, (1.15 * 24) / size);
  const named = rest['aria-label'] != null;
  const draw = (node: IconNode, i: number): React.ReactNode => {
    const { key: _key, ...attrs } = node[1] as Record<string, string | number>;
    if ('strokeWidth' in attrs) attrs.strokeWidth = stroke;
    return createElement(node[0], { key: i, ...attrs }, node[2]?.map(draw));
  };
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={fill ?? 'none'}
      data-act={act}
      aria-hidden={named ? undefined : true}
      role={named ? 'img' : undefined}
      className={cn('shrink-0', className)}
      {...rest}
    >
      {icon.map(draw)}
    </svg>
  );
}
