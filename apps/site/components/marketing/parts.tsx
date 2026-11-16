/**
 * The pieces the comparison, use-case and press pages share: the page frame with the top bar and footer, a header,
 * a plain FAQ (its text also goes out as FAQPage data), and links onward to related pages and guides.
 */
import Link from 'next/link';
import { ArrowRightIcon, DownloadDownIcon } from '@pikaicons/core-free';
import { Nav } from '@/components/site/nav';
import { Icon } from '@/components/site/icon';
import { FinalCta, Footer } from '@/components/landing/more';
import { release } from '@/lib/release';

export function MarketingFrame({ children }: { children: React.ReactNode }) {
  const r = release();
  return (
    <>
      <Nav />
      <main id="main">
        {children}
        <FinalCta release={r} />
      </main>
      <Footer version={r.version} />
    </>
  );
}

export function PageHeader({
  eyebrow,
  title,
  lede,
  crumbs,
}: {
  eyebrow: string;
  title: string;
  lede: string;
  crumbs: [name: string, href: string][];
}) {
  return (
    <header className="shell pt-12 pb-16 sm:pt-20 sm:pb-20">
      <nav aria-label="Breadcrumb" className="text-[13px] text-paper-subtle">
        <ol className="flex flex-wrap items-center gap-1.5">
          <li>
            <Link href="/" className="hover:text-paper">
              Home
            </Link>
          </li>
          {crumbs.map(([name, href], i) => (
            <li key={href} className="flex items-center gap-1.5">
              <span aria-hidden>/</span>
              {i === crumbs.length - 1 ? (
                <span aria-current="page" className="text-paper-muted">
                  {name}
                </span>
              ) : (
                <Link href={href} className="hover:text-paper">
                  {name}
                </Link>
              )}
            </li>
          ))}
        </ol>
      </nav>
      <p className="eyebrow mt-10 text-lime">{eyebrow}</p>
      <h1 className="display mt-4 max-w-[20ch] text-[clamp(2.6rem,6vw,5rem)]">{title}</h1>
      <p className="mt-6 max-w-[60ch] text-[clamp(1.05rem,1.4vw,1.2rem)] leading-[1.55] text-paper-muted">
        {lede}
      </p>
      <div className="mt-8 flex flex-wrap items-center gap-3">
        <Link
          href="/#install"
          className="press group inline-flex h-13 items-center gap-2.5 rounded-full bg-lime px-7 text-[16px] font-semibold text-lime-ink shadow-[0_14px_30px_-12px_var(--lime-glow)] hover:bg-lime-hover"
        >
          <Icon icon={DownloadDownIcon} size={19} />
          Download free
        </Link>
        <Link
          href="/docs"
          className="press group inline-flex h-13 items-center gap-2 rounded-full bg-graphite-2 px-6 text-[16px] font-semibold text-paper ring-1 ring-line-strong ring-inset hover:bg-graphite-3"
        >
          Read the guides
          <Icon icon={ArrowRightIcon} size={18} />
        </Link>
      </div>
    </header>
  );
}

export function Section({ title, children, id }: { title: string; children: React.ReactNode; id?: string }) {
  const heading = id ?? title.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  return (
    <section className="shell pb-20" aria-labelledby={heading}>
      <h2
        id={heading}
        className="font-display text-[clamp(1.7rem,3vw,2.4rem)] leading-[1.05] font-extrabold tracking-[-0.03em]"
      >
        {title}
      </h2>
      <div className="mt-8">{children}</div>
    </section>
  );
}

export function PlainFaq({ items }: { items: [string, string][] }) {
  return (
    <dl className="grid max-w-[52rem] gap-8">
      {items.map(([q, a]) => (
        <div key={q}>
          <dt className="text-[17px] font-semibold text-paper">{q}</dt>
          <dd className="mt-2 text-[15.5px] leading-relaxed text-paper-muted">{a}</dd>
        </div>
      ))}
    </dl>
  );
}

export function LinkCards({ links }: { links: { href: string; title: string; text?: string }[] }) {
  return (
    <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {links.map((l) => (
        <li key={l.href}>
          <Link
            href={l.href}
            className="press group flex h-full flex-col rounded-2xl bg-graphite-2 p-5 ring-1 ring-line ring-inset hover:bg-graphite-3"
          >
            <span className="flex items-center justify-between gap-3 font-semibold text-paper">
              {l.title}
              <Icon icon={ArrowRightIcon} size={15} className="text-paper-subtle group-hover:text-lime" />
            </span>
            {l.text && <span className="mt-1.5 text-[14px] leading-relaxed text-paper-muted">{l.text}</span>}
          </Link>
        </li>
      ))}
    </ul>
  );
}
