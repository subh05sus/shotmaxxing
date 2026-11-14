/**
 * Confetti for the rare, earned moment (the first export). About 36 small pieces, each animated with transform and
 * opacity only, removed as soon as they land. Skipped entirely with Reduce Motion or Low power.
 */

import { motionLevel } from './env.svelte';

const COLORS = ['var(--accent)', 'var(--accent-hover)', 'var(--fg)', 'var(--accent-text)'];

export function confetti(origin?: { x: number; y: number }): void {
  if (motionLevel() === 'fades' || typeof document === 'undefined') return;
  const layer = document.createElement('div');
  layer.setAttribute('aria-hidden', 'true');
  layer.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:9999;overflow:hidden';
  document.body.appendChild(layer);
  const x0 = origin?.x ?? window.innerWidth / 2;
  const y0 = origin?.y ?? window.innerHeight * 0.4;
  const count = 36;
  let finished = 0;
  for (let i = 0; i < count; i++) {
    const piece = document.createElement('span');
    const size = 5 + Math.random() * 5;
    const angle = -Math.PI / 2 + (Math.random() - 0.5) * Math.PI * 0.9;
    const speed = 160 + Math.random() * 220;
    const dx = Math.cos(angle) * speed;
    const dy = Math.sin(angle) * speed;
    const spin = (Math.random() - 0.5) * 720;
    piece.style.cssText = `position:absolute;left:${x0}px;top:${y0}px;width:${size}px;height:${size * 0.6}px;border-radius:2px;background:${COLORS[i % COLORS.length]};will-change:transform,opacity`;
    layer.appendChild(piece);
    // Rises, then falls along an arc.
    const animation = piece.animate(
      [
        { transform: 'translate(0,0) rotate(0deg)', opacity: 1 },
        {
          transform: `translate(${dx * 0.6}px, ${dy * 0.6}px) rotate(${spin * 0.5}deg)`,
          opacity: 1,
          offset: 0.4,
        },
        { transform: `translate(${dx}px, ${dy + 260}px) rotate(${spin}deg)`, opacity: 0 },
      ],
      { duration: 1100 + Math.random() * 300, easing: 'cubic-bezier(0.2, 0, 0, 1)', fill: 'forwards' },
    );
    animation.onfinish = () => {
      if (++finished === count) layer.remove();
    };
  }
}
