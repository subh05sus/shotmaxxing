import { USE_CASES } from '@/lib/marketing';
import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'What people make with Shotmaxxing';
export const size = OG_SIZE;
export const contentType = 'image/png';

export function generateStaticParams() {
  return USE_CASES.map((u) => ({ slug: u.path.split('/').pop()! }));
}

export default async function Image({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const page = USE_CASES.find((u) => u.path === `/use-cases/${slug}`);
  return ogImage({ eyebrow: page?.eyebrow ?? 'Use cases', title: page?.h1 ?? 'What people make with it.' });
}
