import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { ArrowLeftIcon, ArrowRightIcon, ClockIcon, GithubIcon } from '@pikaicons/core-free';
import { DocBody } from '@/components/docs/render';
import { Toc } from '@/components/docs/toc';
import { Icon } from '@/components/site/icon';
import { allSlugs, editUrl, getDoc, getNav, neighbours } from '@/lib/docs';
import { LinkCards } from '@/components/marketing/parts';
import { JsonLd } from '@/components/site/json-ld';
import { COMPARISONS, USE_CASES } from '@/lib/marketing';
import { breadcrumbs, graph, url } from '@/lib/seo';

// Where each guide leads next outside the guides: the use cases it serves, and a comparison.
const USE: Record<string, string[]> = {
  install: ['/alternatives/screen-studio', '/use-cases/product-demos'],
  setup: ['/use-cases/product-demos'],
  recording: ['/use-cases/product-demos', '/use-cases/tutorials'],
  screenshots: ['/use-cases/tutorials'],
  editor: ['/use-cases/product-demos', '/alternatives/screen-studio'],
  'auto-zoom': ['/use-cases/product-demos', '/alternatives/screen-studio'],
  cursor: ['/use-cases/product-demos', '/use-cases/tutorials'],
  background: ['/use-cases/launch-videos', '/use-cases/product-demos'],
  annotate: ['/use-cases/tutorials'],
  camera: ['/use-cases/tutorials', '/vs/loom'],
  sound: ['/use-cases/tutorials', '/use-cases/launch-videos'],
  captions: ['/use-cases/tutorials', '/use-cases/launch-videos'],
  export: ['/use-cases/launch-videos'],
  agents: ['/use-cases/ai-agent-demos'],
  'command-line': ['/use-cases/ai-agent-demos'],
};
const MARKETING = new Map([
  ...USE_CASES.map((u) => [u.path, { title: u.name, text: u.description }] as const),
  ...COMPARISONS.map((c) => [c.path, { title: c.eyebrow, text: c.description }] as const),
]);

type Props = { params: Promise<{ slug: string }> };

export const dynamicParams = false;

export function generateStaticParams() {
  return allSlugs().map((slug) => ({ slug }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const doc = await getDoc((await params).slug);
  if (!doc) return {};
  const path = `/docs/${doc.slug}`;
  return {
    title: doc.title,
    description: doc.lede,
    alternates: { canonical: path },
    openGraph: { title: doc.title, description: doc.lede, url: path, type: 'article' },
  };
}

export default async function GuidePage({ params }: Props) {
  const { slug } = await params;
  const doc = await getDoc(slug);
  if (!doc) notFound();
  const { prev, next } = neighbours(slug);
  const titles = new Map((await getNav()).flatMap((s) => s.pages.map((p) => [p.slug, p.title] as const)));

  return (
    <div className="grid gap-12 xl:grid-cols-[minmax(0,1fr)_13rem]">
      <JsonLd
        data={graph(
          breadcrumbs([
            ['Guides', '/docs'],
            [doc.title, `/docs/${slug}`],
          ]),
          {
            '@type': 'TechArticle',
            headline: doc.title,
            description: doc.lede,
            url: url(`/docs/${slug}`),
            articleSection: doc.section,
            timeRequired: `PT${doc.minutes}M`,
            about: { '@id': url('/#app') },
            publisher: { '@id': url('/#organization') },
          },
        )}
      />
      <article className="max-w-[46rem] min-w-0">
        <header>
          <p className="eyebrow text-lime">{doc.section}</p>
          <h1 className="display mt-4 text-[clamp(2.3rem,4.6vw,3.6rem)]">{doc.title}</h1>
          {doc.lede && <p className="mt-5 text-[18px] leading-relaxed text-paper-muted">{doc.lede}</p>}
          <p className="mt-6 flex flex-wrap items-center gap-x-5 gap-y-2 text-[13px] text-paper-subtle">
            <span className="inline-flex items-center gap-1.5">
              <Icon icon={ClockIcon} size={14} />
              {doc.minutes} min read
            </span>
            <a href={editUrl(doc.file)} className="group inline-flex items-center gap-1.5 hover:text-paper">
              <Icon icon={GithubIcon} size={14} />
              Improve this guide
            </a>
          </p>
        </header>
        <hr className="mt-8 mb-10 border-line" />
        <DocBody tree={doc.tree} />

        <nav aria-label="More guides" className="mt-16 grid gap-3 sm:grid-cols-2">
          {prev ? (
            <Link
              href={`/docs/${prev.slug}`}
              className="press group flex flex-col rounded-2xl bg-graphite-2 p-5 ring-1 ring-line ring-inset hover:bg-graphite-3"
            >
              <span className="flex items-center gap-1.5 text-[12px] text-paper-subtle">
                <Icon icon={ArrowLeftIcon} size={13} />
                Previous
              </span>
              <span className="mt-1 font-semibold">{titles.get(prev.slug) ?? prev.slug}</span>
            </Link>
          ) : (
            <span className="max-sm:hidden" />
          )}
          {next && (
            <Link
              href={`/docs/${next.slug}`}
              className="press group flex flex-col items-end rounded-2xl bg-graphite-2 p-5 text-right ring-1 ring-line ring-inset hover:bg-graphite-3"
            >
              <span className="flex items-center gap-1.5 text-[12px] text-paper-subtle">
                Next
                <Icon icon={ArrowRightIcon} size={13} />
              </span>
              <span className="mt-1 font-semibold">{titles.get(next.slug) ?? next.slug}</span>
            </Link>
          )}
        </nav>
        {(USE[slug] ?? []).length > 0 && (
          <section className="mt-14" aria-labelledby="put-to-use">
            <h2 id="put-to-use" className="eyebrow text-paper-subtle">
              Put it to use
            </h2>
            <div className="mt-4">
              <LinkCards
                links={(USE[slug] ?? []).flatMap((href) => {
                  const m = MARKETING.get(href);
                  return m ? [{ href, ...m }] : [];
                })}
              />
            </div>
          </section>
        )}
      </article>
      <aside className="hidden xl:block">
        <Toc headings={doc.headings} />
      </aside>
    </div>
  );
}
