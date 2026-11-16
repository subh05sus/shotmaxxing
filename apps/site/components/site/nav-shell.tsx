'use client';

/**
 * The top bar's frame. With `dock` (the home page) the bar sits cut into the hero's frame near the top of the page,
 * and floats off as a pill below that. Where the browser has scroll-linked animation, the CSS does all of it from the
 * scroll position (see .nav-bar in globals.css); elsewhere this sets data-docked for the CSS fallback. It reads the
 * scroll position and never moves the page.
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
    // Undock once the frame's top edge, and the arcs on it, have scrolled past the top of the page.
    const measure = () => {
      const css = getComputedStyle(el);
      reach =
        parseFloat(css.getPropertyValue('--frame-gap')) + parseFloat(css.getPropertyValue('--nav-fillet')) ||
        36;
    };
    const check = () => {
      frame = 0;
      setDocked(scrollY < reach);
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
    <header
      ref={ref}
      data-dock={dock ? '' : undefined}
      data-docked={docked ? '' : undefined}
      className="nav-shell sticky top-0 z-40"
    >
      {children}
    </header>
  );
}
