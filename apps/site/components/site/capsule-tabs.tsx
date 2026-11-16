'use client';

/**
 * Capsule tabs: a rounded track with a thumb that springs to the chosen tab.
 *
 * The thumb is a coloured copy of the whole row clipped to the chosen tab (clip-path with round corners), so the
 * label under it turns ink exactly as the thumb passes, and its corners stay perfectly round at any width. On a move
 * the leading edge sets off first and overshoots a little; the trailing edge follows a beat later, so the thumb
 * stretches in the direction it travels and squashes back as it lands. A new choice mid-move takes over from where
 * the thumb is (GSAP overwrites the running tween), so it never has to finish first.
 */
import { Tabs as TabsPrimitive } from 'radix-ui';
import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { gsap } from '@/lib/gsap';
import { cn } from '@/lib/utils';
import { Icon, type IconData } from './icon';

export interface TabItem {
  value: string;
  label: string;
  icon?: IconData;
}

interface Props {
  items: TabItem[];
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  tone?: 'dark' | 'lime';
  size?: 'sm' | 'md';
  label: string;
  className?: string;
  listClassName?: string;
  children: ReactNode;
}

export function CapsuleTabs({
  items,
  value: controlled,
  defaultValue,
  onValueChange,
  tone = 'dark',
  size = 'md',
  label,
  className,
  listClassName,
  children,
}: Props) {
  const [own, setOwn] = useState(defaultValue ?? items[0]?.value);
  const value = controlled ?? own;
  const list = useRef<HTMLDivElement>(null);
  const thumb = useRef<HTMLDivElement>(null);
  const edges = useRef({ l: 0, r: 0, ready: false });

  const paint = useCallback(() => {
    const e = edges.current;
    if (thumb.current) thumb.current.style.clipPath = `inset(0 ${e.r}px 0 ${e.l}px round 999px)`;
  }, []);

  const place = useCallback(
    (animate: boolean) => {
      const row = list.current;
      const tab = row?.querySelector<HTMLElement>(`[data-value="${CSS.escape(value ?? '')}"]`);
      if (!row || !tab) return;
      const l = tab.offsetLeft;
      const r = row.clientWidth - (tab.offsetLeft + tab.offsetWidth);
      const e = edges.current;
      const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
      if (!animate || !e.ready || reduced) {
        gsap.killTweensOf(e);
        Object.assign(e, { l, r, ready: true });
        paint();
        return;
      }
      // Moving right, the right edge leads; moving left, the left edge does.
      const right = l > e.l;
      const lead = { duration: 0.34, ease: 'back.out(1.7)' };
      const trail = { duration: 0.42, ease: 'power3.out', delay: 0.05 };
      gsap.to(e, { l, ...(right ? trail : lead), overwrite: false, onUpdate: paint });
      gsap.to(e, { r, ...(right ? lead : trail), overwrite: false, onUpdate: paint });
    },
    [value, paint],
  );

  useLayoutEffect(() => place(true), [place]);
  useLayoutEffect(() => {
    const row = list.current;
    if (!row) return;
    const ro = new ResizeObserver(() => place(false));
    ro.observe(row);
    return () => ro.disconnect();
  }, [place]);

  const pad = size === 'sm' ? 'h-9 px-3.5 text-[13px]' : 'h-11 px-3.5 text-sm sm:px-4.5';
  const row = (ink: boolean) =>
    items.map((item) => (
      <span
        key={item.value}
        className={cn(
          'flex items-center justify-center gap-2 font-medium whitespace-nowrap',
          pad,
          ink ? (tone === 'dark' ? 'text-lime-ink' : 'text-lime') : '',
        )}
      >
        {item.icon && <Icon icon={item.icon} size={size === 'sm' ? 15 : 17} act="lift" />}
        {item.label}
      </span>
    ));

  return (
    <TabsPrimitive.Root
      value={value}
      onValueChange={(v) => {
        if (controlled === undefined) setOwn(v);
        onValueChange?.(v);
      }}
      className={className}
    >
      <div
        className={cn(
          'relative inline-flex max-w-full overflow-x-auto rounded-full p-1 [scrollbar-width:none]',
          tone === 'dark' ? 'bg-fill ring-1 ring-line ring-inset' : 'bg-lime-ink-subtle',
          listClassName,
        )}
      >
        <div className="relative" ref={list}>
          <TabsPrimitive.List aria-label={label} className="relative flex">
            {items.map((item, i) => (
              <TabsPrimitive.Trigger
                key={item.value}
                value={item.value}
                data-value={item.value}
                className={cn(
                  'press rounded-full outline-none focus-visible:ring-2 focus-visible:ring-inset',
                  tone === 'dark'
                    ? 'text-paper-muted hover:text-paper focus-visible:ring-lime'
                    : 'text-lime-ink-muted hover:text-lime-ink focus-visible:ring-lime-ink',
                )}
              >
                {row(false)[i]}
              </TabsPrimitive.Trigger>
            ))}
          </TabsPrimitive.List>
          {/* The thumb: the same row in ink on lime, clipped to the chosen tab. */}
          <div
            ref={thumb}
            aria-hidden="true"
            className={cn(
              'pointer-events-none absolute inset-0 flex rounded-full',
              tone === 'dark' ? 'bg-lime' : 'bg-lime-ink',
            )}
            style={{ clipPath: 'inset(0 100% 0 0 round 999px)' }}
          >
            {row(true)}
          </div>
        </div>
      </div>
      {children}
    </TabsPrimitive.Root>
  );
}

/** A tab's panel: it rises into place as it's chosen. */
export function TabPanel({
  value,
  className,
  children,
}: {
  value: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <TabsPrimitive.Content
      value={value}
      className={cn(
        'outline-none data-[state=active]:animate-in data-[state=active]:fade-in-0 data-[state=active]:slide-in-from-bottom-2 data-[state=active]:duration-300',
        className,
      )}
    >
      {children}
    </TabsPrimitive.Content>
  );
}
