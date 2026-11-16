import type { Metadata } from 'next';
import Link from 'next/link';
import {
  ArrowRightIcon,
  BookmarkIcon,
  BotIcon,
  ClockIcon,
  PencilEditIcon,
  RocketShipIcon,
  VideoCallIcon,
} from '@pikaicons/core-free';
import { Icon, type IconData } from '@/components/site/icon';
import { getNav } from '@/lib/docs';

export const metadata: Metadata = {
  title: 'Guides',
  description:
    'Install Shotmaxxing, record your first take, edit it, and let an AI agent film for you, step by step.',
  alternates: { canonical: '/docs' },
};

const ICONS: Record<string, IconData> = {
  'Get started': RocketShipIcon,
  Record: VideoCallIcon,
  Edit: PencilEditIcon,
  Agents: BotIcon,
  Reference: BookmarkIcon,
};

export default async function GuidesIndex() {
  const sections = await getNav();
  const [start, ...rest] = sections;
  return (
    <div className="max-w-[52rem]">
      <p className="eyebrow text-lime">Guides</p>
      <h1 className="display mt-4 text-[clamp(2.6rem,5.5vw,4.4rem)]">Everything, step by step.</h1>
      <p className="mt-5 max-w-[56ch] text-[17.5px] leading-relaxed text-paper-muted">
        Install Shotmaxxing, record your first take, make it look good, and let an AI agent film for you.
      </p>

      {/* Get started, bigger: most people arrive here to install. */}
      <ol className="mt-12 grid grid-cols-1 gap-3 sm:grid-cols-3">
        {start.pages.map((p, i) => (
          <li key={p.slug}>
            <Link
              href={`/docs/${p.slug}`}
              className="press group flex h-full flex-col rounded-3xl bg-lime-tint p-5 ring-1 ring-lime/20 ring-inset hover:bg-lime/18 focus-visible:ring-2 focus-visible:ring-lime"
            >
              <span className="grid size-8 place-items-center rounded-full bg-lime font-mono text-[13px] font-bold text-lime-ink">
                {i + 1}
              </span>
              <span className="mt-4 font-display text-[19px] leading-tight font-bold tracking-[-0.02em]">
                {p.title}
              </span>
              <span className="mt-1.5 line-clamp-3 text-[14px] leading-relaxed text-paper-muted">
                {p.lede}
              </span>
              <span className="mt-auto flex items-center gap-1.5 pt-4 text-[13px] font-semibold text-lime">
                Start
                <Icon icon={ArrowRightIcon} size={14} />
              </span>
            </Link>
          </li>
        ))}
      </ol>

      {rest.map((s) => (
        <section key={s.title} className="mt-14" aria-labelledby={`s-${s.title}`}>
          <h2
            id={`s-${s.title}`}
            className="flex items-center gap-3 font-display text-[24px] font-extrabold tracking-[-0.03em]"
          >
            <span className="grid size-9 place-items-center rounded-xl bg-fill text-lime">
              <Icon icon={ICONS[s.title] ?? BookmarkIcon} size={18} />
            </span>
            {s.title}
          </h2>
          <ul className="mt-5 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {s.pages.map((p) => (
              <li key={p.slug}>
                <Link
                  href={`/docs/${p.slug}`}
                  className="press group flex h-full flex-col rounded-2xl bg-graphite-2 p-5 ring-1 ring-line ring-inset hover:bg-graphite-3 focus-visible:ring-2 focus-visible:ring-lime"
                >
                  <span className="flex items-start justify-between gap-3">
                    <span className="font-semibold text-paper">{p.title}</span>
                    <Icon
                      icon={ArrowRightIcon}
                      size={16}
                      className="mt-1 shrink-0 text-paper-subtle group-hover:text-lime"
                    />
                  </span>
                  <span className="mt-1.5 line-clamp-2 text-[14px] leading-relaxed [overflow-wrap:anywhere] text-paper-muted">
                    {p.lede}
                  </span>
                  <span className="mt-auto flex items-center gap-1.5 pt-3 text-[12px] text-paper-subtle">
                    <Icon icon={ClockIcon} size={13} />
                    {p.minutes} min read
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
