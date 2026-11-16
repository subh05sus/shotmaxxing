import Link from 'next/link';
import { DownloadDownIcon } from '@pikaicons/core-free';
import { getNav } from '@/lib/docs';
import { GITHUB } from '@/lib/release';
import { cn } from '@/lib/utils';
import { Icon } from './icon';
import { AppIcon } from './mark';
import { MobileMenu } from './mobile-menu';
import { NavShell } from './nav-shell';

const LINKS = [
  { href: '/docs', label: 'Guides' },
  { href: '/docs/agents', label: 'Agents' },
  { href: '/docs/command-line', label: 'Command line' },
  { href: `${GITHUB}/blob/main/CHANGELOG.md`, label: 'Changelog' },
];

/** The top bar: a floating capsule, solid (no blur), that stays at the top as you scroll. */
export async function Nav({ download = '/#install', dock = false }: { download?: string; dock?: boolean }) {
  const sections = await getNav();
  return (
    <NavShell dock={dock}>
      <nav
        aria-label="Main"
        className="nav-bar mx-auto flex h-14 max-w-[76rem] items-center gap-2 border border-line-strong bg-graphite-2 pr-2 pl-2 shadow-lift"
      >
        <Link
          href="/"
          className="press group flex h-11 items-center gap-2.5 rounded-full pr-3 pl-1.5 outline-none focus-visible:ring-2 focus-visible:ring-lime focus-visible:ring-inset"
          aria-label="Shotmaxxing home"
        >
          <AppIcon className="size-8 transition-transform duration-300 ease-[var(--spring)] group-hover:-rotate-12" />
          <span className="font-display text-[17px] font-bold tracking-[-0.03em]">Shotmaxxing</span>
        </Link>
        <ul className="ml-2 hidden items-center lg:flex">
          {LINKS.map((l) => (
            <li key={l.href}>
              <Link
                href={l.href}
                className="press flex h-10 items-center rounded-full px-3.5 text-sm whitespace-nowrap text-paper-muted outline-none hover:bg-fill hover:text-paper focus-visible:ring-2 focus-visible:ring-lime focus-visible:ring-inset"
              >
                {l.label}
              </Link>
            </li>
          ))}
        </ul>
        <div className="ml-auto flex items-center gap-1.5">
          <Link
            href={download}
            className={cn(
              'press group hidden h-10 items-center gap-2 rounded-full bg-lime px-4 text-sm font-semibold text-lime-ink outline-none hover:bg-lime-hover focus-visible:ring-2 focus-visible:ring-paper focus-visible:ring-offset-2 focus-visible:ring-offset-graphite-2 sm:inline-flex',
            )}
          >
            <Icon icon={DownloadDownIcon} size={16} act="drop" />
            Download
          </Link>
          <MobileMenu links={LINKS} sections={sections} download={download} />
        </div>
      </nav>
    </NavShell>
  );
}
