'use client';

/** "On this page": the guide's headings, with the one being read marked as the page scrolls past it. */
import { useEffect, useState } from 'react';
import type { Heading } from '@/lib/docs';
import { cn } from '@/lib/utils';

export function Toc({ headings }: { headings: Heading[] }) {
  const [active, setActive] = useState(headings[0]?.id ?? '');

  useEffect(() => {
    const els = headings.map((h) => document.getElementById(h.id)).filter((e): e is HTMLElement => !!e);
    if (!els.length) return;
    // The heading that's passed the top fifth of the screen most recently is the one being read.
    const pick = () => {
      const line = innerHeight * 0.2;
      let current = els[0].id;
      for (const e of els) if (e.getBoundingClientRect().top <= line) current = e.id;
      setActive(current);
    };
    const io = new IntersectionObserver(pick, { rootMargin: '0px 0px -60% 0px', threshold: [0, 1] });
    els.forEach((e) => io.observe(e));
    pick();
    return () => io.disconnect();
  }, [headings]);

  if (headings.length < 2) return null;
  return (
    <nav
      aria-label="On this page"
      className="scroll-room sticky top-24 max-h-[calc(100dvh-7rem)] overflow-y-auto pb-8"
    >
      <p className="eyebrow px-3 text-paper-subtle">On this page</p>
      <ul className="mt-3 grid gap-0.5">
        {headings.map((h) => (
          <li key={h.id}>
            <a
              href={`#${h.id}`}
              aria-current={active === h.id ? 'location' : undefined}
              className={cn(
                'flex items-center gap-2 rounded-lg py-1 pr-2 text-[13px] leading-snug transition-colors outline-none focus-visible:ring-2 focus-visible:ring-lime focus-visible:ring-inset',
                h.depth === 3 ? 'pl-6' : 'pl-3',
                active === h.id ? 'text-paper' : 'text-paper-subtle hover:text-paper',
              )}
            >
              <span
                aria-hidden
                className={cn(
                  'size-1.5 shrink-0 rounded-full transition-colors',
                  active === h.id ? 'bg-lime' : 'bg-transparent',
                )}
              />
              {h.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
