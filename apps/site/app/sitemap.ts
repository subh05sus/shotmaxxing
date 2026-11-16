import type { MetadataRoute } from 'next';
import { allSlugs } from '@/lib/docs';
import { MARKETING_PAGES } from '@/lib/marketing';
import { url } from '@/lib/seo';

/** Every page, for search engines: the home page, the guides, the comparisons, the use cases and the press kit. */
export default function sitemap(): MetadataRoute.Sitemap {
  const now = new Date();
  const page = (path: string, priority: number, changeFrequency: 'weekly' | 'monthly' = 'monthly') => ({
    url: url(path),
    lastModified: now,
    changeFrequency,
    priority,
  });
  return [
    page('/', 1, 'weekly'),
    page('/docs', 0.8, 'weekly'),
    ...allSlugs().map((slug) => page(`/docs/${slug}`, slug === 'install' || slug === 'agents' ? 0.8 : 0.6)),
    page('/compare', 0.6),
    page('/use-cases', 0.6),
    ...MARKETING_PAGES.map((p) => page(p.path, 0.7)),
    page('/press', 0.4),
  ];
}
