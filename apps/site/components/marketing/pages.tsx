/** A comparison page and a use-case page, from the data in lib/marketing.ts. */
import type { Metadata } from 'next';
import Link from 'next/link';
import { JsonLd } from '@/components/site/json-ld';
import { CHECKED, COMPARISONS, USE_CASES, type Comparison, type UseCase } from '@/lib/marketing';
import { GITHUB } from '@/lib/release';
import { breadcrumbs, faqPage, graph, url } from '@/lib/seo';
import { LinkCards, MarketingFrame, PageHeader, PlainFaq, Section } from './parts';

export function pageMetadata(page: { path: string; title: string; description: string }): Metadata {
  return {
    title: { absolute: page.title },
    description: page.description,
    alternates: { canonical: page.path },
    openGraph: { title: page.title, description: page.description, url: page.path, type: 'article' },
    twitter: { title: page.title, description: page.description },
  };
}

export function ComparisonPage({ page }: { page: Comparison }) {
  const others = COMPARISONS.filter((c) => c.path !== page.path);
  return (
    <MarketingFrame>
      <JsonLd
        data={graph(
          breadcrumbs([
            ['Compare', '/compare'],
            [page.eyebrow, page.path],
          ]),
          faqPage(page.faq),
          { '@type': 'WebPage', name: page.title, description: page.description, url: url(page.path) },
        )}
      />
      <PageHeader
        eyebrow={page.eyebrow}
        title={page.h1}
        lede={page.lede}
        crumbs={[
          ['Compare', '/compare'],
          [page.name, page.path],
        ]}
      />
      <Section title={`Shotmaxxing and ${page.name}, side by side`} id="side-by-side">
        <div className="scroll-room -mx-1 overflow-x-auto px-1">
          <table className="w-full min-w-[36rem] border-collapse text-left text-[15px]">
            <thead>
              <tr className="border-b border-line-strong text-[13px] text-paper-subtle">
                <th scope="col" className="w-[34%] py-3 pr-4 font-medium">
                  &nbsp;
                </th>
                <th scope="col" className="w-[33%] py-3 pr-4 font-semibold text-lime">
                  Shotmaxxing
                </th>
                <th scope="col" className="py-3 font-semibold text-paper">
                  {page.name}
                </th>
              </tr>
            </thead>
            <tbody>
              {page.rows.map((r) => (
                <tr key={r.feature} className="border-b border-line align-top">
                  <th scope="row" className="py-4 pr-4 font-medium text-paper">
                    {r.feature}
                  </th>
                  <td className="py-4 pr-4 text-paper">{r.us}</td>
                  <td className="py-4 text-paper-muted">{r.them}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 max-w-[70ch] text-[13px] leading-relaxed text-paper-subtle">
          We read{' '}
          <a href={page.site} className="underline underline-offset-4 hover:text-paper" rel="noopener">
            {page.name}’s site
          </a>{' '}
          in {CHECKED}. Apps change; if something here is out of date,{' '}
          <a href={`${GITHUB}/issues/new/choose`} className="underline underline-offset-4 hover:text-paper">
            tell us
          </a>{' '}
          and we’ll fix it.
        </p>
      </Section>
      <Section title="Why people pick Shotmaxxing">
        <ul className="grid gap-4 md:grid-cols-3">
          {page.reasons.map((r) => (
            <li key={r.title} className="rounded-2xl bg-graphite-2 p-6 ring-1 ring-line ring-inset">
              <h3 className="text-[17px] font-semibold text-paper">{r.title}</h3>
              <p className="mt-2 text-[15px] leading-relaxed text-paper-muted">{r.text}</p>
            </li>
          ))}
        </ul>
      </Section>
      <Section title={`When ${page.name} is the better pick`} id="their-strengths">
        <p className="max-w-[64ch] text-[16px] leading-relaxed text-paper-muted">{page.theirs}</p>
      </Section>
      <Section title="Questions">
        <PlainFaq items={page.faq} />
      </Section>
      <Section title="Keep reading">
        <LinkCards
          links={[
            {
              href: '/docs/install',
              title: 'Install Shotmaxxing',
              text: 'One line in the terminal, or a download.',
            },
            {
              href: '/docs/auto-zoom',
              title: 'Automatic zoom',
              text: 'How the zooms follow clicks and typing.',
            },
            {
              href: '/use-cases/ai-agent-demos',
              title: 'Let an AI agent film it',
              text: 'From one sentence to an edited take.',
            },
            ...others.map((c) => ({ href: c.path, title: c.eyebrow })),
          ]}
        />
      </Section>
    </MarketingFrame>
  );
}

export function UseCasePage({ page }: { page: UseCase }) {
  const others = USE_CASES.filter((u) => u.path !== page.path);
  return (
    <MarketingFrame>
      <JsonLd
        data={graph(
          breadcrumbs([
            ['Use cases', '/use-cases'],
            [page.name, page.path],
          ]),
          faqPage(page.faq),
          {
            '@type': 'HowTo',
            name: page.title,
            description: page.description,
            tool: { '@type': 'HowToTool', name: 'Shotmaxxing' },
            step: page.steps.map((s, i) => ({
              '@type': 'HowToStep',
              position: i + 1,
              name: s.title,
              text: s.text,
              url: url(s.guide[0]),
            })),
          },
        )}
      />
      <PageHeader
        eyebrow={page.eyebrow}
        title={page.h1}
        lede={page.lede}
        crumbs={[
          ['Use cases', '/use-cases'],
          [page.name, page.path],
        ]}
      />
      <Section title="How it goes">
        <ol className="grid gap-4 md:grid-cols-2">
          {page.steps.map((s, i) => (
            <li
              key={s.title}
              className="flex gap-4 rounded-2xl bg-graphite-2 p-6 ring-1 ring-line ring-inset"
            >
              <span className="grid size-8 shrink-0 place-items-center rounded-full bg-lime font-mono text-[13px] font-semibold text-lime-ink">
                {i + 1}
              </span>
              <div>
                <h3 className="text-[17px] font-semibold text-paper">{s.title}</h3>
                <p className="mt-1.5 text-[15px] leading-relaxed text-paper-muted">{s.text}</p>
                <Link
                  href={s.guide[0]}
                  className="mt-3 inline-flex text-[14px] font-semibold text-lime underline-offset-4 hover:underline"
                >
                  Guide: {s.guide[1]}
                </Link>
              </div>
            </li>
          ))}
        </ol>
      </Section>
      <Section title="Tips">
        <ul className="grid max-w-[52rem] gap-3">
          {page.tips.map((t) => (
            <li key={t} className="flex gap-3 text-[15.5px] leading-relaxed text-paper-muted">
              <span className="mt-[0.6em] size-1.5 shrink-0 rounded-full bg-lime" />
              <span>{t}</span>
            </li>
          ))}
        </ul>
      </Section>
      <Section title="Questions">
        <PlainFaq items={page.faq} />
      </Section>
      <Section title="More ways to use it">
        <LinkCards
          links={[
            ...others.map((u) => ({ href: u.path, title: u.name, text: u.description })),
            { href: '/alternatives/screen-studio', title: 'A free Screen Studio alternative' },
            { href: '/docs', title: 'All the guides' },
          ]}
        />
      </Section>
    </MarketingFrame>
  );
}

/** The index of a group of pages: comparisons or use cases. */
export function IndexPage({
  eyebrow,
  title,
  lede,
  path,
  links,
}: {
  eyebrow: string;
  title: string;
  lede: string;
  path: string;
  links: { href: string; title: string; text: string }[];
}) {
  return (
    <MarketingFrame>
      <JsonLd data={graph(breadcrumbs([[eyebrow, path]]))} />
      <PageHeader eyebrow={eyebrow} title={title} lede={lede} crumbs={[[eyebrow, path]]} />
      <section className="shell pb-20" aria-label={eyebrow}>
        <LinkCards links={links} />
      </section>
    </MarketingFrame>
  );
}
