'use client';

/**
 * The top bar's frame. With `dock` (the home page) the bar sits cut into the hero's frame near the top of the page,
 * and floats off as a pill below that. While docked it passes the scroll to the CSS (--sy, --p) so the notch's arcs
 * stay on the frame's top edge as it scrolls, and tighten until they're gone at the point the bar undocks. It only
 * reads the scroll position; it never moves the page.
 */
import { useEffect, useRef, useState } from 'react';

export function NavShell({ dock, children }: { dock: boolean; children: React.ReactNode }) {
  const ref = useRef<HTMLElement>(null);
  const [docked, setDocked] = useState(dock);
  useEffect(() => {
    const el = ref.current;
    if (!dock || !el) return;
    let frame = 0;
    let reach = 36;
    let last = -1;
    // The arcs are gone once the frame's top edge has scrolled a gap plus an arc past the top of the page.
    const measure = () => {
      const css = getComputedStyle(el);
      reach =
        parseFloat(css.getPropertyValue('--frame-gap')) + parseFloat(css.getPropertyValue('--nav-fillet')) ||
        36;
    };
    const check = () => {
      frame = 0;
      const y = Math.min(Math.max(scrollY, 0), reach);
      const p = 1 - y / reach;
      if (p !== last) {
        last = p;
        el.style.setProperty('--sy', `${y}px`);
        el.style.setProperty('--p', p.toFixed(4));
      }
      setDocked(y < reach);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(check);
    };
    const onResize = () => {
      measure();
      onScroll();
    };
    measure();
    check();
    addEventListener('scroll', onScroll, { passive: true });
    addEventListener('resize', onResize);
    return () => {
      removeEventListener('scroll', onScroll);
      removeEventListener('resize', onResize);
      cancelAnimationFrame(frame);
    };
  }, [dock]);
  return (
    <header ref={ref} data-docked={docked ? '' : undefined} className="nav-shell sticky top-0 z-40">
      {children}
    </header>
  );
}
