/**
 * Places a floating element (tooltip, menu, popover) next to its trigger, flipping to the other side when there is
 * no room and keeping 8 px from the window edges. Returns the transform origin so the element grows out of the
 * trigger (origin-aware animation).
 */

export type Side = 'top' | 'bottom' | 'left' | 'right';
export type Align = 'start' | 'center' | 'end';

export interface Placement {
  x: number;
  y: number;
  side: Side;
  origin: string;
}

const EDGE = 8;

export function place(
  trigger: DOMRect,
  floating: { width: number; height: number },
  { side = 'bottom', align = 'center', gap = 6 }: { side?: Side; align?: Align; gap?: number } = {},
): Placement {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const fits = (s: Side) =>
    s === 'bottom'
      ? trigger.bottom + gap + floating.height <= vh - EDGE
      : s === 'top'
        ? trigger.top - gap - floating.height >= EDGE
        : s === 'right'
          ? trigger.right + gap + floating.width <= vw - EDGE
          : trigger.left - gap - floating.width >= EDGE;
  const opposite: Record<Side, Side> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };
  const chosen = fits(side) || !fits(opposite[side]) ? side : opposite[side];

  let x: number;
  let y: number;
  if (chosen === 'top' || chosen === 'bottom') {
    y = chosen === 'bottom' ? trigger.bottom + gap : trigger.top - gap - floating.height;
    x =
      align === 'start'
        ? trigger.left
        : align === 'end'
          ? trigger.right - floating.width
          : trigger.left + trigger.width / 2 - floating.width / 2;
  } else {
    x = chosen === 'right' ? trigger.right + gap : trigger.left - gap - floating.width;
    y =
      align === 'start'
        ? trigger.top
        : align === 'end'
          ? trigger.bottom - floating.height
          : trigger.top + trigger.height / 2 - floating.height / 2;
  }
  x = Math.min(Math.max(EDGE, x), vw - floating.width - EDGE);
  y = Math.min(Math.max(EDGE, y), vh - floating.height - EDGE);

  // The origin is the point of the floating element nearest the trigger's centre.
  const cx = Math.min(Math.max(trigger.left + trigger.width / 2 - x, 0), floating.width);
  const cy = Math.min(Math.max(trigger.top + trigger.height / 2 - y, 0), floating.height);
  const origin =
    chosen === 'bottom'
      ? `${cx}px 0px`
      : chosen === 'top'
        ? `${cx}px ${floating.height}px`
        : chosen === 'right'
          ? `0px ${cy}px`
          : `${floating.width}px ${cy}px`;
  return { x: Math.round(x), y: Math.round(y), side: chosen, origin };
}
