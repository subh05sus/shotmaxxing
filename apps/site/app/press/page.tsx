import Image from 'next/image';
import { DownloadDownIcon } from '@pikaicons/core-free';
import { pageMetadata } from '@/components/marketing/pages';
import { MarketingFrame, PageHeader, Section } from '@/components/marketing/parts';
import { CopyButton } from '@/components/site/copy-button';
import { Icon } from '@/components/site/icon';
import { JsonLd } from '@/components/site/json-ld';
import { ProductHuntBadge } from '@/components/site/product-hunt';
import { GITHUB, release } from '@/lib/release';
import { breadcrumbs, graph } from '@/lib/seo';

export const metadata = pageMetadata({
  path: '/press',
  title: 'Shotmaxxing press kit: logos, screenshots, videos and facts',
  description:
    'Everything for writing about Shotmaxxing, the free open-source screen recorder that AI agents can drive: descriptions, facts, logos, screenshots and videos.',
});

const SHORT =
  'Shotmaxxing is a free, open-source screen recorder and studio editor for Mac that people and AI agents can both drive.';
const LONG =
  'Shotmaxxing records your screen and comes out edited: it zooms in on clicks and typing, smooths and enlarges the cursor, and frames the recording on a photo, colour or gradient background. AI agents such as Claude Code and Codex can film demos with it over MCP: they read the app’s code, plan the take, and drive it at a human pace while the person watches, with every take allowed first. Everything stays on the computer. It is free under the AGPL-3.0, with no account and no watermark.';

const ASSETS = [
  { title: 'App icon (SVG)', href: '/media/brand/app-icon.svg', preview: '/media/brand/app-icon.svg' },
  { title: 'Logo (SVG)', href: '/media/brand/logo.svg', preview: '/media/brand/logo.svg' },
  { title: 'App icon (PNG, 512 px)', href: '/icon-512.png', preview: '/icon-512.png' },
];

const SHOTS = [
  { src: '/media/images/editor.png', width: 2720, height: 1720, alt: 'The Shotmaxxing editor' },
  {
    src: '/media/images/editor-cursor.png',
    width: 2720,
    height: 1720,
    alt: 'The Cursor tool and its eight styles',
  },
  {
    src: '/media/images/agents.png',
    width: 2240,
    height: 1480,
    alt: 'The Agents page, connecting AI agents',
  },
  { src: '/media/images/record.png', width: 1680, height: 1200, alt: 'Choosing what to record' },
  {
    src: '/media/images/take-card.png',
    width: 840,
    height: 888,
    alt: 'The take card asking before an agent films',
  },
];

const VIDEOS = [
  { title: 'The editor (MP4, 21 s)', href: '/hero/hero.mp4' },
  { title: 'An AI agent films a demo (MP4, 29 s)', href: '/hero/agent.mp4' },
];

export default function Press() {
  const r = release();
  const facts: [string, React.ReactNode][] = [
    ['Name', 'Shotmaxxing'],
    ['What it is', 'Screen recorder and studio editor, drivable by AI agents'],
    ['Platforms', 'macOS 14 or later (Windows builds in testing)'],
    ['Price', 'Free'],
    ['Licence', 'AGPL-3.0'],
    ['Latest version', r.version],
    ['Website', 'shotmaxxing.app'],
    [
      'Source',
      <a key="src" href={GITHUB} className="underline underline-offset-4 hover:text-lime">
        {GITHUB.replace('https://', '')}
      </a>,
    ],
  ];
  return (
    <MarketingFrame>
      <JsonLd data={graph(breadcrumbs([['Press kit', '/press']]))} />
      <PageHeader
        eyebrow="Press kit"
        title="Writing about Shotmaxxing?"
        lede="Descriptions, facts, logos, screenshots and videos, free to use in articles, reviews, launch posts and videos about Shotmaxxing."
        crumbs={[['Press kit', '/press']]}
      />
      <div className="shell -mt-8 pb-12">
        <ProductHuntBadge />
      </div>
      <Section title="Describe it">
        <div className="grid gap-4 lg:grid-cols-2">
          {[
            ['In one line', SHORT],
            ['In a paragraph', LONG],
          ].map(([title, text]) => (
            <div key={title} className="rounded-2xl bg-graphite-2 p-6 ring-1 ring-line ring-inset">
              <div className="flex items-center justify-between gap-3">
                <h3 className="text-[15px] font-semibold text-paper">{title}</h3>
                <CopyButton text={text} label={`Copy “${title.toLowerCase()}”`} />
              </div>
              <p className="mt-3 text-[15.5px] leading-relaxed text-paper-muted">{text}</p>
            </div>
          ))}
        </div>
      </Section>
      <Section title="Facts">
        <dl className="grid max-w-[46rem] gap-x-8 sm:grid-cols-[12rem_1fr]">
          {facts.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="border-b border-line py-3 text-[14px] text-paper-subtle">{k}</dt>
              <dd className="border-b border-line py-3 text-[15px] text-paper">{v}</dd>
            </div>
          ))}
        </dl>
      </Section>
      <Section title="Logos">
        <ul className="grid gap-4 sm:grid-cols-3">
          {ASSETS.map((a) => (
            <li key={a.href} className="rounded-2xl bg-graphite-2 p-5 ring-1 ring-line ring-inset">
              <div className="grid h-36 place-items-center rounded-xl bg-stage">
                {/* eslint-disable-next-line @next/next/no-img-element -- an SVG shown as is */}
                <img src={a.preview} alt="" className="max-h-24 max-w-[80%]" />
              </div>
              <a
                href={a.href}
                download
                className="mt-4 inline-flex items-center gap-2 text-[14.5px] font-semibold text-paper hover:text-lime"
              >
                <Icon icon={DownloadDownIcon} size={16} />
                {a.title}
              </a>
            </li>
          ))}
        </ul>
        <p className="mt-4 max-w-[64ch] text-[13.5px] leading-relaxed text-paper-subtle">
          Please keep the icon’s lime tile and ink mark as they are, and leave room round it.
        </p>
      </Section>
      <Section title="Screenshots">
        <ul className="grid gap-4 sm:grid-cols-2">
          {SHOTS.map((s) => (
            <li key={s.src}>
              <a
                href={s.src}
                download
                className="group block overflow-hidden rounded-2xl bg-stage ring-1 ring-line-strong"
              >
                <Image
                  src={s.src}
                  width={s.width}
                  height={s.height}
                  alt={s.alt}
                  sizes="(min-width: 640px) 50vw, 100vw"
                  className="h-auto w-full"
                />
              </a>
              <p className="mt-2 text-[13.5px] text-paper-muted">{s.alt}</p>
            </li>
          ))}
        </ul>
      </Section>
      <Section title="Videos">
        <ul className="flex flex-wrap gap-3">
          {VIDEOS.map((v) => (
            <li key={v.href}>
              <a
                href={v.href}
                download
                className="press inline-flex h-11 items-center gap-2 rounded-full bg-fill px-5 text-[14.5px] font-semibold text-paper ring-1 ring-line ring-inset hover:bg-fill-hover"
              >
                <Icon icon={DownloadDownIcon} size={16} />
                {v.title}
              </a>
            </li>
          ))}
        </ul>
      </Section>
      <Section title="Get in touch">
        <p className="max-w-[60ch] text-[16px] leading-relaxed text-paper-muted">
          For questions, interviews or a walkthrough,{' '}
          <a
            href={`${GITHUB}/issues/new/choose`}
            className="font-semibold text-paper underline underline-offset-4"
          >
            open an issue on GitHub
          </a>{' '}
          and say it’s for press.
        </p>
      </Section>
    </MarketingFrame>
  );
}
