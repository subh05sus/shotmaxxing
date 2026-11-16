import type { MetadataRoute } from 'next';
import { url } from '@/lib/seo';

/** Everything is open to crawlers, answer engines included; the install scripts aren't pages. */
export default function robots(): MetadataRoute.Robots {
  return {
    rules: [{ userAgent: '*', allow: '/', disallow: ['/install', '/install.ps1'] }],
    sitemap: url('/sitemap.xml'),
    host: url('/'),
  };
}
