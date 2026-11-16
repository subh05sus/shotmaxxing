'use client';

/**
 * The top bar's frame. With `dock` (the home page) it sits cut into the hero's frame while the page is at the top,
 * and floats as a pill once it scrolls. It only reads the scroll position; it never moves the page.
 */
import { useEffect, useState } from 'react';

export function NavShell({ dock, children }: { dock: boolean; children: React.ReactNode }) {
  const [docked, setDocked] = useState(dock);
  useEffect(() => {
    if (!dock) return;
    let frame = 0;
    const check = () => {
      frame = 0;
      setDocked(scrollY < 24);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(check);
    };
    check();
    addEventListener('scroll', onScroll, { passive: true });
    return () => {
      removeEventListener('scroll', onScroll);
      cancelAnimationFrame(frame);
    };
  }, [dock]);
  return (
    <header data-docked={docked ? '' : undefined} className="nav-shell sticky top-0 z-40">
      {children}
    </header>
  );
}
