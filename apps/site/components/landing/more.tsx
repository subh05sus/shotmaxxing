import Link from 'next/link';
import {
  ArrowRightIcon,
  GithubIcon,
  LockCloseIcon,
  PauseCircleIcon,
  ShieldCheckIcon,
} from '@pikaicons/core-free';
import type { Release } from '@/lib/release';
import { GITHUB } from '@/lib/release';
import { Icon, type IconData } from '@/components/site/icon';
import { CopyButton } from '@/components/site/copy-button';
import { SplitHeading } from '@/components/site/split-heading';
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion';
import { AgentTake } from './agent-take';
import { InstallLine } from './platforms';
import { cn } from '@/lib/utils';
import { ProductHuntBadge } from '@/components/site/product-hunt';
import { COMPARISONS, USE_CASES } from '@/lib/marketing';

const SAFETY: { icon: IconData; title: string; text: string }[] = [
  {
    icon: ShieldCheckIcon,
    title: 'Nothing until you allow it',
    text: 'The take card asks before an agent sees or records your screen, and before every take, showing its plan.',
  },
  {
    icon: PauseCircleIcon,
    title: 'Stop it any time',
    text: 'A thin red edge goes round the screen while the agent drives. Press Esc three times or move the mouse, and the take stops.',
  },
  {
    icon: LockCloseIcon,
    title: 'Passwords stay yours',
    text: 'Agents never type into password fields. Sign-ins come to you, and the agent waits until you hand back.',
  },
];

export function Agents() {
  return (
    <section
      className="frame on-lime relative overflow-hidden pt-28 pb-24 sm:pt-32 sm:pb-28"
      aria-labelledby="agents-title"
    >
      <p className="notch eyebrow flex h-11 items-center gap-2 px-6 text-lime">
        <span aria-hidden className="size-1.5 rounded-full bg-lime" />
        Agents
      </p>
      <div className="shell">
        <SplitHeading className="display max-w-[15ch] text-[clamp(2.8rem,7vw,6.2rem)]">
          <span id="agents-title">Or ask an agent to film it.</span>
        </SplitHeading>
        <p className="measure mt-6 text-[17px] leading-relaxed text-lime-ink-muted" data-rise>
          Connect Claude Code, Codex, Cursor or another agent in one click, then ask for a demo. It plans the
          steps, checks them against the screen, and films one smooth take that opens in the editor with its
          zooms added.
        </p>
        <div className="mt-12">
          <AgentTake />
        </div>
        <ul className="mt-5 grid gap-4 md:grid-cols-3">
          {SAFETY.map((s) => (
            <li key={s.title} data-rise className="flex gap-4 rounded-3xl bg-lime-ink-subtle p-5">
              <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-lime-ink text-lime">
                <Icon icon={s.icon} size={18} />
              </span>
              <span>
                <p className="font-display text-[18px] font-bold tracking-[-0.02em]">{s.title}</p>
                <p className="mt-1 text-[14px] leading-relaxed text-lime-ink-muted">{s.text}</p>
              </span>
            </li>
          ))}
        </ul>
        <Link
          href="/docs/agents"
          className="press group mt-10 inline-flex h-12 items-center gap-2 rounded-full bg-lime-ink px-5 text-[15px] font-semibold text-lime hover:bg-stage"
          data-rise
        >
          Connect an agent
          <Icon icon={ArrowRightIcon} size={17} />
        </Link>
      </div>
    </section>
  );
}

// Each answer, and its plain text when it has links: the text goes out as FAQPage data for search and answer engines.
const FAQ: [question: string, answer: React.ReactNode, text?: string][] = [
  [
    'Is it really free?',
    'Yes. Shotmaxxing is free and open source under the AGPL 3.0. There is no account, no trial and no watermark.',
  ],
  [
    'Why does macOS warn me about a file I downloaded in a browser?',
    <>
      The builds aren’t notarized by Apple yet, which needs a paid Apple account. The install line and
      Homebrew don’t get the warning. For a downloaded disk image, the{' '}
      <Link
        href="/docs/install#download-the-installer-yourself"
        className="font-semibold underline underline-offset-4"
      >
        install guide
      </Link>{' '}
      shows the one-time Open Anyway.
    </>,
    'The builds aren’t notarized by Apple yet, which needs a paid Apple account. The install line and Homebrew don’t get the warning. For a downloaded disk image, the install guide shows the one-time Open Anyway.',
  ],
  [
    'Setup still asks for Screen Recording, but it’s on in System Settings.',
    <>
      macOS is matching the switch to an earlier copy of the app. Select Shotmaxxing in that list, remove it
      with the − button, and choose Allow again. From 0.2.1 on, updates keep the permissions.{' '}
      <Link href="/docs/permissions" className="font-semibold underline underline-offset-4">
        Permissions guide
      </Link>
    </>,
    'macOS is matching the switch to an earlier copy of the app. Select Shotmaxxing in that list, remove it with the − button, and choose Allow again. From 0.2.1 on, updates keep the permissions.',
  ],
  [
    'How is it different from Screen Studio?',
    <>
      It does the same polished job, automatic zooms, a smooth cursor and a framed background, but it’s free
      and open source, and AI agents can film demos with it.{' '}
      <Link href="/alternatives/screen-studio" className="font-semibold underline underline-offset-4">
        See the comparison
      </Link>
    </>,
    'It does the same polished job, automatic zooms, a smooth cursor and a framed background, but it’s free and open source, and AI agents can film demos with it.',
  ],
  [
    'What can I export?',
    'MP4 at 1080p or 4K, shaped for YouTube, X, LinkedIn, Shorts, Reels and TikTok, and GIFs for docs and READMEs. Framed screenshots too.',
  ],
  [
    'Does it work on Windows and Linux?',
    'Recording, export and agent takes on Windows are written but haven’t been tested on a Windows computer yet, so expect rough edges. Linux isn’t supported yet; it’s on the roadmap.',
  ],
  [
    'Which agents can film with it?',
    'Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode and Gemini CLI connect in one click from the Agents page. Any agent that speaks MCP can use `shotmaxxing mcp`.',
  ],
  [
    'Is anything uploaded?',
    'No. Recordings, projects and captions stay on your computer. The app only asks GitHub once a day whether a newer release is out (you can turn that off), and downloads the caption model the first time you make captions.',
  ],
];

/** The questions and their plain answers, for FAQPage data on the home page. */
export const FAQ_TEXT: [string, string][] = FAQ.map(([q, a, text]) => [
  q,
  text ?? (typeof a === 'string' ? a : ''),
]);

export function Faq() {
  return (
    <section className="relative bg-graphite pt-4 pb-24 sm:pb-28" aria-labelledby="faq-title">
      <div className="shell grid gap-12 lg:grid-cols-[0.8fr_1.2fr]">
        <div>
          <p className="eyebrow text-lime" data-rise>
            Questions
          </p>
          <SplitHeading className="display mt-4 text-[clamp(2.4rem,5vw,4.2rem)]">
            <span id="faq-title">Good to know.</span>
          </SplitHeading>
        </div>
        <Accordion type="single" collapsible className="grid gap-3" data-rise>
          {FAQ.map(([q, a], i) => (
            <AccordionItem
              key={i}
              value={String(i)}
              className="overflow-hidden rounded-3xl border border-line bg-graphite-2 px-6 last:border-b data-[state=open]:border-line-strong"
            >
              <AccordionTrigger className="py-5 text-left font-display text-[18px] font-bold tracking-[-0.015em] hover:no-underline">
                {q}
              </AccordionTrigger>
              <AccordionContent className="pb-6 text-[15px] leading-relaxed text-paper-muted">
                {a}
              </AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </div>
    </section>
  );
}

export function FinalCta({ release }: { release: Release }) {
  return (
    <section
      className="frame on-lime relative mb-3 overflow-hidden pt-28 pb-24 sm:mb-4 sm:pt-36 sm:pb-32"
      aria-labelledby="final-title"
    >
      <p className="notch eyebrow flex h-11 items-center gap-2 px-6 text-paper-muted">
        Free and open source · v{release.version}
      </p>
      <div className="shell text-center">
        <SplitHeading className="display mx-auto max-w-[12ch] text-[clamp(3.2rem,10vw,9rem)]">
          <span id="final-title">Film your next demo.</span>
        </SplitHeading>
        <InstallLine mac={release.commands.mac} windows={release.commands.windows} />
      </div>
    </section>
  );
}

export function Footer({ version }: { version: string }) {
  const cols: [string, [string, string][]][] = [
    [
      'Product',
      [
        ['Features', '/#features'],
        ['Install', '/docs/install'],
        ['Guides', '/docs'],
        ['Agents', '/docs/agents'],
        ['Command line', '/docs/command-line'],
      ],
    ],
    [
      'Use cases',
      [...USE_CASES.map((u) => [u.name, u.path] as [string, string]), ['All use cases', '/use-cases']],
    ],
    [
      'Compare',
      [...COMPARISONS.map((c) => [c.eyebrow, c.path] as [string, string]), ['All comparisons', '/compare']],
    ],
    [
      'Project',
      [
        ['Source code', GITHUB],
        ['Changelog', `${GITHUB}/blob/main/CHANGELOG.md`],
        ['Roadmap', `${GITHUB}/blob/main/docs/roadmap.md`],
        ['Releases', `${GITHUB}/releases`],
        ['Press kit', '/press'],
      ],
    ],
    [
      'Help',
      [
        ['Troubleshooting', '/docs/troubleshooting'],
        ['Report a bug', `${GITHUB}/issues/new/choose`],
        ['Contributing', `${GITHUB}/blob/main/CONTRIBUTING.md`],
        ['Security', `${GITHUB}/blob/main/SECURITY.md`],
      ],
    ],
  ];
  return (
    <footer className="bg-stage pt-20 pb-[max(2.5rem,env(safe-area-inset-bottom))]">
      <div className="shell">
        <div className="grid gap-x-8 gap-y-12 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-[1.4fr_repeat(5,1fr)]">
          <div>
            <p className="font-display text-[26px] font-bold tracking-[-0.03em]">Shotmaxxing</p>
            <p className="mt-2 max-w-[32ch] text-[14.5px] leading-relaxed text-paper-muted">
              Open-source screen recorder and studio editor for demos, drivable by people and AI agents.
            </p>
            <ProductHuntBadge className="mt-6" />
          </div>
          {cols.map(([title, links]) => (
            <div key={title}>
              <p className="eyebrow text-paper-subtle">{title}</p>
              <ul className="mt-4 space-y-1">
                {links.map(([label, href]) => (
                  <li key={label}>
                    <Link
                      href={href}
                      className="inline-flex h-9 items-center text-[14.5px] text-paper-muted hover:text-lime"
                    >
                      {label}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <div
          className={cn(
            'mt-16 flex flex-wrap items-center justify-between gap-4 border-t border-line pt-6 text-[13px] text-paper-subtle',
          )}
        >
          <p>
            Version <span className="tabular">{version}</span> · AGPL-3.0 · Icons by Pikaicons (MIT)
          </p>
          <a
            href={GITHUB}
            aria-label="Shotmaxxing on GitHub"
            className="press group hit grid size-8 place-items-center rounded-full text-paper-subtle hover:bg-fill hover:text-paper"
          >
            <Icon icon={GithubIcon} size={16} />
          </a>
        </div>
      </div>
    </footer>
  );
}
