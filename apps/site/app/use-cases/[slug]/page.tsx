import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { UseCasePage, pageMetadata } from '@/components/marketing/pages';
import { USE_CASES } from '@/lib/marketing';

type Props = { params: Promise<{ slug: string }> };

const find = (slug: string) => USE_CASES.find((u) => u.path === `/use-cases/${slug}`);

export const dynamicParams = false;

export function generateStaticParams() {
  return USE_CASES.map((u) => ({ slug: u.path.split('/').pop()! }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const page = find((await params).slug);
  return page ? pageMetadata(page) : {};
}

export default async function UseCase({ params }: Props) {
  const page = find((await params).slug);
  if (!page) notFound();
  return <UseCasePage page={page} />;
}
