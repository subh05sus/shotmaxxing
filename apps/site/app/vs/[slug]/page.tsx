import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { ComparisonPage, pageMetadata } from '@/components/marketing/pages';
import { COMPARISONS } from '@/lib/marketing';

type Props = { params: Promise<{ slug: string }> };

const find = (slug: string) => COMPARISONS.find((c) => c.path === `/vs/${slug}`);

export const dynamicParams = false;

export function generateStaticParams() {
  return COMPARISONS.filter((c) => c.path.startsWith('/vs/')).map((c) => ({ slug: c.path.slice(4) }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const page = find((await params).slug);
  return page ? pageMetadata(page) : {};
}

export default async function Versus({ params }: Props) {
  const page = find((await params).slug);
  if (!page) notFound();
  return <ComparisonPage page={page} />;
}
