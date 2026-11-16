'use client';

/**
 * A headline whose lines rise from behind a mask, one after another, as it scrolls into view. SplitText splits it
 * into lines after the fonts have loaded and again when the width changes (autoSplit), so the lines are always the
 * real ones. The text is in the HTML untouched; with Reduce Motion it simply stays.
 */
import { useRef, type ElementType, type ReactNode } from 'react';
import { SplitText } from 'gsap/SplitText';
import { gsap, useGSAP } from '@/lib/gsap';
import { onRevealed } from '@/lib/reveal';

gsap.registerPlugin(SplitText);

export function SplitHeading({
  as: Tag = 'h2',
  children,
  className,
  /** Play when the page is revealed rather than on scroll (for the first screen). */
  onLoad = false,
  delay = 0,
}: {
  as?: ElementType;
  children: ReactNode;
  className?: string;
  onLoad?: boolean;
  delay?: number;
}) {
  const el = useRef<HTMLElement>(null);

  useGSAP(
    () => {
      const node = el.current;
      if (!node || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
      let cancel = () => {};
      const split = SplitText.create(node, {
        type: 'lines',
        mask: 'lines',
        linesClass: 'split-line',
        autoSplit: true,
        onSplit(self) {
          const tween = gsap.from(self.lines, {
            yPercent: 110,
            rotate: 2.5,
            duration: 0.9,
            ease: 'expo.out',
            stagger: 0.09,
            delay,
            paused: true,
            ...(onLoad ? {} : { scrollTrigger: { trigger: node, start: 'top 88%', once: true } }),
          });
          // On scroll, ScrollTrigger plays it on entering; on load, the loader opening does.
          if (onLoad) cancel = onRevealed(() => tween.play());
          return tween;
        },
      });
      return () => {
        cancel();
        split.revert();
      };
    },
    { scope: el },
  );

  return (
    <Tag ref={el} className={className}>
      {children}
    </Tag>
  );
}
