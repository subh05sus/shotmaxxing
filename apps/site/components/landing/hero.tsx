import Link from 'next/link';
import { ArrowRightIcon } from '@pikaicons/core-free';
import type { Release } from '@/lib/release';
import { Icon } from '@/components/site/icon';
import { SplitHeading } from '@/components/site/split-heading';
import { HeroLight } from './hero-light';
import { HeroVideo } from './hero-video';
import { HeroInstall, PlatformLinks } from './platforms';

export function Hero({ release }: { release: Release }) {
  return (
    <section className="relative -mt-14" aria-labelledby="hero-title">
      <div className="frame relative isolate mt-(--frame-gap) overflow-hidden bg-stage pt-14">
        {/* The arcs that join the docked top bar to the frame's edge; they scroll with the frame. */}
        <div aria-hidden className="nav-arcs" />
        {/* The light glows across the top; its dark horizon rises behind the title, so the type is white. */}
        <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-64 lg:h-80">
          <HeroLight />
        </div>
        <div className="shell pt-14 pb-[clamp(9rem,22vw,19rem)] sm:pt-20">
          <p className="flex justify-center" data-rise>
            <a
              href={release.notes}
              className="press group inline-flex h-9 items-center gap-2 rounded-full bg-graphite-2 px-1.5 pr-3.5 text-[13px] font-medium text-paper shadow-lift ring-1 ring-line-strong ring-inset hover:bg-graphite-3"
            >
              <span className="rounded-full bg-lime px-2 py-0.5 font-mono text-[11px] font-semibold text-lime-ink">
                v{release.version}
              </span>
              New cursors, backgrounds and blur
              <Icon icon={ArrowRightIcon} size={14} />
            </a>
          </p>
          <SplitHeading
            as="h1"
            onLoad
            className="display mx-auto mt-6 max-w-[17ch] text-center text-[clamp(2.75rem,7vw,6.25rem)] text-paper"
          >
            <span id="hero-title">Beautiful screen recordings, by you or your AI.</span>
          </SplitHeading>
          <p
            className="mx-auto mt-6 max-w-[52ch] text-center text-[clamp(1.05rem,1.5vw,1.25rem)] leading-[1.5] text-paper-muted"
            data-rise
          >
            Record your screen yourself, or ask an AI agent to film the demo for you. Either way, Shotmaxxing
            adds the zooms, smooths the cursor and frames it on a beautiful background. Free and open source.
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3" data-rise>
            <HeroInstall />
            <Link
              href="/docs"
              className="press group inline-flex h-13 items-center gap-2 rounded-full bg-graphite-2 px-6 text-[16px] font-semibold text-paper ring-1 ring-line-strong ring-inset hover:bg-graphite-3"
            >
              Read the guides
              <Icon icon={ArrowRightIcon} size={18} />
            </Link>
          </div>
          <div className="mt-6" data-rise>
            <PlatformLinks />
          </div>
        </div>
      </div>
      {/* The video sits across the frame's bottom edge. */}
      <div className="shell relative -mt-[clamp(7rem,19vw,16.5rem)]" data-rise>
        <HeroVideo />
      </div>
    </section>
  );
}
