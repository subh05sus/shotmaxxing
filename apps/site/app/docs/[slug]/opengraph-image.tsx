import { getDoc, allSlugs } from '@/lib/docs';
import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'A Shotmaxxing guide';
export const size = OG_SIZE;
export const contentType = 'image/png';

export function generateStaticParams() {
  return allSlugs().map((slug) => ({ slug }));
}

export default async function Image({ params }: { params: Promise<{ slug: string }> }) {
  const doc = await getDoc((await params).slug);
  return ogImage({ eyebrow: `Guide · ${doc?.section ?? 'Shotmaxxing'}`, title: doc?.title ?? 'Guides' });
}
