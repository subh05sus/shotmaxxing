'use client';

/** Every guide by section, with the open one marked. Wide screens only; phones get the menu sheet. */
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { cn } from '@/lib/utils';

interface Props {
  sections: { title: string; pages: { slug: string; title: string }[] }[];
}

export function Sidebar({ sections }: Props) {
  const path = usePathname();
  return (
    <nav
      aria-label="Guides"
      className="scroll-room sticky top-24 max-h-[calc(100dvh-7rem)] overflow-y-auto pr-2 pb-8"
    >
      <Link
        href="/docs"
        aria-current={path === '/docs' ? 'page' : undefined}
        className={cn(
          'press flex h-9 items-center rounded-xl px-3 text-[14px] font-semibold hover:bg-fill',
          path === '/docs' ? 'bg-fill text-paper' : 'text-paper-muted',
        )}
      >
        All guides
      </Link>
      {sections.map((s) => (
        <div key={s.title} className="mt-6">
          <p className="eyebrow px-3 text-paper-subtle">{s.title}</p>
          <ul className="mt-2 grid gap-0.5">
            {s.pages.map((p) => {
              const on = path === `/docs/${p.slug}`;
              return (
                <li key={p.slug}>
                  <Link
                    href={`/docs/${p.slug}`}
                    aria-current={on ? 'page' : undefined}
                    className={cn(
                      'flex min-h-9 items-center gap-2.5 rounded-xl px-3 py-1.5 text-[14px] leading-snug transition-colors outline-none focus-visible:ring-2 focus-visible:ring-lime focus-visible:ring-inset',
                      on
                        ? 'bg-lime-tint font-semibold text-paper'
                        : 'text-paper-muted hover:bg-fill hover:text-paper',
                    )}
                  >
                    <span
                      aria-hidden
                      className={cn(
                        'size-1.5 shrink-0 rounded-full transition-colors',
                        on ? 'bg-lime' : 'bg-transparent',
                      )}
                    />
                    {p.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
