/**
 * Svelte transitions for Shotmaxxing. Each one animates only opacity and transform, which Svelte turns into CSS
 * keyframes that run on the compositor. Exits run at about two thirds of the entrance. With Reduce Motion or Low
 * power, every transition becomes a short fade.
 */

import type { TransitionConfig } from 'svelte/transition';
import { motionLevel } from './env.svelte';

/** cubic-bezier(x1, y1, x2, y2) as an easing function. */
export function bezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const cx = 3 * x1;
  const bx = 3 * (x2 - x1) - cx;
  const ax = 1 - cx - bx;
  const cy = 3 * y1;
  const by = 3 * (y2 - y1) - cy;
  const ay = 1 - cy - by;
  const sampleX = (t: number) => ((ax * t + bx) * t + cx) * t;
  const sampleY = (t: number) => ((ay * t + by) * t + cy) * t;
  const slopeX = (t: number) => (3 * ax * t + 2 * bx) * t + cx;
  return (x: number) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let t = x;
    for (let i = 0; i < 6; i++) {
      const err = sampleX(t) - x;
      const d = slopeX(t);
      if (Math.abs(err) < 1e-6 || Math.abs(d) < 1e-6) break;
      t -= err / d;
    }
    return sampleY(Math.min(1, Math.max(0, t)));
  };
}

export const easeOut = bezier(0.2, 0, 0, 1);
export const easeExit = bezier(0.4, 0, 1, 1);

/** A calm spring (damping ratio about 0.75): a small overshoot, settled by the end. */
export function spring(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const zeta = 0.75;
  const omega = 9;
  const wd = omega * Math.sqrt(1 - zeta * zeta);
  const decay = Math.exp(-zeta * omega * t);
  const value = 1 - decay * (Math.cos(wd * t) + ((zeta * omega) / wd) * Math.sin(wd * t));
  // Blend into exactly 1 over the last 15% so the animation ends where it should.
  const tail = Math.max(0, (t - 0.85) / 0.15);
  return value + (1 - value) * tail;
}

export const DURATION = { fast: 140, base: 200, slow: 260 } as const;

function fadeOnly(duration: number, delay = 0): TransitionConfig {
  return { duration: Math.min(duration, 150), delay, css: (t) => `opacity: ${t}` };
}

export interface RiseParams {
  y?: number;
  duration?: number;
  delay?: number;
  /** Index in a group, for a stagger capped at 8 items. */
  index?: number;
}

/** Content arriving: rises a few pixels while fading in. */
export function rise(
  _node: Element,
  { y = 6, duration = DURATION.slow, delay = 0, index = 0 }: RiseParams = {},
): TransitionConfig {
  const staggered = delay + Math.min(index, 8) * 35;
  if (motionLevel() === 'fades') return fadeOnly(duration, staggered);
  return {
    duration,
    delay: staggered,
    easing: easeOut,
    css: (t, u) => `opacity: ${t}; transform: translateY(${u * y}px)`,
  };
}

/** Leaving: a quick fade with a slight drop, faster than any entrance. */
export function sink(
  _node: Element,
  { y = 4, duration = 140 }: { y?: number; duration?: number } = {},
): TransitionConfig {
  if (motionLevel() === 'fades') return fadeOnly(duration);
  return { duration, easing: easeExit, css: (t, u) => `opacity: ${t}; transform: translateY(${u * y}px)` };
}

export interface PopParams {
  from?: number;
  duration?: number;
  /** CSS transform-origin; popovers pass the side their trigger is on, so they grow out of it. */
  origin?: string;
}

/** Menus, popovers and sheets: grow from the trigger with a calm spring. */
export function pop(
  node: Element,
  { from = 0.96, duration = DURATION.base, origin }: PopParams = {},
): TransitionConfig {
  if (origin) (node as HTMLElement).style.transformOrigin = origin;
  if (motionLevel() === 'fades') return fadeOnly(duration);
  return {
    duration,
    easing: spring,
    css: (t) => `opacity: ${Math.min(1, t * 1.6)}; transform: scale(${from + (1 - from) * t})`,
  };
}

/** Exit for popovers: shrink back toward the trigger, quickly. */
export function unpop(
  node: Element,
  { from = 0.97, duration = 120, origin }: PopParams = {},
): TransitionConfig {
  if (origin) (node as HTMLElement).style.transformOrigin = origin;
  if (motionLevel() === 'fades') return fadeOnly(duration);
  return {
    duration,
    easing: easeExit,
    css: (t) => `opacity: ${t}; transform: scale(${from + (1 - from) * t})`,
  };
}

/** Panels sliding in from a side. */
export function slide(
  _node: Element,
  { x = 0, y = 0, duration = DURATION.slow }: { x?: number; y?: number; duration?: number } = {},
): TransitionConfig {
  if (motionLevel() === 'fades') return fadeOnly(duration);
  return {
    duration,
    easing: spring,
    css: (t, u) => `opacity: ${Math.min(1, t * 2)}; transform: translate(${u * x}px, ${u * y}px)`,
  };
}

/** A plain cross-fade, for one view replacing another. */
export function fade(
  _node: Element,
  { duration = 150, delay = 0 }: { duration?: number; delay?: number } = {},
): TransitionConfig {
  return { duration, delay, css: (t) => `opacity: ${t}` };
}
