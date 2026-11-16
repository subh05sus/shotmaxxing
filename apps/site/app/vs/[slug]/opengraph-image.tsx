import { COMPARISONS } from '@/lib/marketing';
import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'Shotmaxxing compared';
export const size = OG_SIZE;
export const contentType = 'image/png';

export function generateStaticParams() {
  return COMPARISONS.filter((c) => c.path.startsWith('/vs/')).map((c) => ({ slug: c.path.slice(4) }));
}

export default async function Image({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const page = COMPARISONS.find((c) => c.path === `/vs/${slug}`);
  return ogImage({ eyebrow: 'Compare', title: page?.h1 ?? 'How it compares.' });
}
