'use client';

import Link from 'next/link';
import { BurgerMenuThreeIcon, DownloadDownIcon } from '@pikaicons/core-free';
import { useState } from 'react';
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from '@/components/ui/sheet';
import { Icon } from './icon';

interface Props {
  links: { href: string; label: string }[];
  sections: { title: string; pages: { slug: string; title: string }[] }[];
  download: string;
}

/** Phones: the links and every guide in a sheet from the right. */
export function MobileMenu({ links, sections, download }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger
        className="press grid size-11 place-items-center rounded-full text-paper hover:bg-fill md:hidden"
        aria-label="Menu"
      >
        <Icon icon={BurgerMenuThreeIcon} size={20} />
      </SheetTrigger>
      <SheetContent side="right" className="w-[min(88vw,360px)] bg-graphite-2 p-0 shadow-float">
        <SheetTitle className="sr-only">Menu</SheetTitle>
        <div className="scroll-room flex h-full flex-col overflow-y-auto px-4 pt-14 pb-[max(1.5rem,env(safe-area-inset-bottom))]">
          <Link
            href={download}
            onClick={() => setOpen(false)}
            className="press flex h-12 items-center justify-center gap-2 rounded-full bg-lime font-semibold text-lime-ink"
          >
            <Icon icon={DownloadDownIcon} size={17} />
            Download
          </Link>
          <ul className="mt-4">
            {links.map((l) => (
              <li key={l.href}>
                <Link
                  href={l.href}
                  onClick={() => setOpen(false)}
                  className="flex h-12 items-center rounded-xl px-3 text-[17px] font-medium hover:bg-fill"
                >
                  {l.label}
                </Link>
              </li>
            ))}
          </ul>
          {sections.map((s) => (
            <div key={s.title} className="mt-5">
              <p className="eyebrow px-3 text-paper-subtle">{s.title}</p>
              <ul className="mt-1">
                {s.pages.map((p) => (
                  <li key={p.slug}>
                    <Link
                      href={`/docs/${p.slug}`}
                      onClick={() => setOpen(false)}
                      className="flex h-11 items-center rounded-xl px-3 text-[15px] text-paper-muted hover:bg-fill hover:text-paper"
                    >
                      {p.title}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </SheetContent>
    </Sheet>
  );
}
