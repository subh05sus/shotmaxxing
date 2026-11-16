import Link from 'next/link';
import { ArrowRightIcon, DownloadDownIcon } from '@pikaicons/core-free';
import type { Release } from '@/lib/release';
import { Icon } from '@/components/site/icon';
import { SplitHeading } from '@/components/site/split-heading';
import { HeroVideo } from './hero-video';

export function Hero({ release }: { release: Release }) {
  return (
    <section className="relative -mt-[4.25rem]" aria-labelledby="hero-title">
      <div className="on-lime pt-[4.25rem]">
        <div className="shell pt-14 pb-[clamp(9rem,22vw,19rem)] sm:pt-20">
          <p className="flex justify-center" data-rise>
            <a
              href={release.notes}
              className="press group inline-flex h-9 items-center gap-2 rounded-full bg-lime-ink px-1.5 pr-3.5 text-[13px] font-medium text-lime"
            >
              <span className="rounded-full bg-lime px-2 py-0.5 font-mono text-[11px] font-semibold text-lime-ink">
                v{release.version}
              </span>
              Permissions now carry over between updates
              <Icon icon={ArrowRightIcon} size={14} act="nudge" />
            </a>
          </p>
          <SplitHeading
            as="h1"
            onLoad
            className="display mx-auto mt-6 max-w-[14ch] text-center text-[clamp(3rem,8vw,7rem)]"
          >
            <span id="hero-title">Demos that edit themselves.</span>
          </SplitHeading>
          <p
            className="mx-auto mt-6 max-w-[52ch] text-center text-[clamp(1.05rem,1.5vw,1.25rem)] leading-[1.5] text-lime-ink-muted"
            data-rise
          >
            Record your screen, or let an AI agent film it. Shotmaxxing adds the zooms, smooths the cursor and
            keeps your computer cool.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3" data-rise>
            <Link
              href="#install"
              className="press group inline-flex h-13 items-center gap-2.5 rounded-full bg-lime-ink px-7 text-[16px] font-semibold text-lime shadow-[0_14px_30px_-12px_var(--shadow)] hover:bg-stage"
            >
              <Icon icon={DownloadDownIcon} size={19} act="drop" />
              Install free
            </Link>
            <Link
              href="/docs"
              className="press group inline-flex h-13 items-center gap-2 rounded-full px-6 text-[16px] font-semibold ring-2 ring-lime-ink/80 ring-inset hover:bg-lime-ink-subtle"
            >
              Read the guides
              <Icon icon={ArrowRightIcon} size={18} act="nudge" />
            </Link>
          </div>
          <p className="mt-5 text-center font-mono text-[12px] tracking-wide text-lime-ink-muted" data-rise>
            macOS 14 or later · Windows in preview · Free and open source
          </p>
        </div>
      </div>
      {/* The video sits across the edge between the lime and the graphite below. */}
      <div className="shell relative -mt-[clamp(7rem,19vw,16.5rem)]" data-rise>
        <HeroVideo />
      </div>
    </section>
  );
}
