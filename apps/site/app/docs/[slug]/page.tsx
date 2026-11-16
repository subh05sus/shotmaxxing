import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { ArrowLeftIcon, ArrowRightIcon, ClockIcon, GithubIcon } from '@pikaicons/core-free';
import { DocBody } from '@/components/docs/render';
import { Toc } from '@/components/docs/toc';
import { Icon } from '@/components/site/icon';
import { allSlugs, editUrl, getDoc, getNav, neighbours } from '@/lib/docs';

type Props = { params: Promise<{ slug: string }> };

export const dynamicParams = false;

export function generateStaticParams() {
  return allSlugs().map((slug) => ({ slug }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const doc = await getDoc((await params).slug);
  if (!doc) return {};
  return { title: doc.title, description: doc.lede, openGraph: { title: doc.title, description: doc.lede } };
}

export default async function GuidePage({ params }: Props) {
  const { slug } = await params;
  const doc = await getDoc(slug);
  if (!doc) notFound();
  const { prev, next } = neighbours(slug);
  const titles = new Map((await getNav()).flatMap((s) => s.pages.map((p) => [p.slug, p.title] as const)));

  return (
    <div className="grid gap-12 xl:grid-cols-[minmax(0,1fr)_13rem]">
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
              <Icon icon={GithubIcon} size={14} act="pop" />
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
                <Icon icon={ArrowRightIcon} size={13} act="nudge" />
              </span>
              <span className="mt-1 font-semibold">{titles.get(next.slug) ?? next.slug}</span>
            </Link>
          )}
        </nav>
      </article>
      <aside className="hidden xl:block">
        <Toc headings={doc.headings} />
      </aside>
    </div>
  );
}
