/**
 * Structured data (schema.org JSON-LD) for search engines and answer engines: who makes Shotmaxxing, what it is, and
 * each page's place in the site. Pages render it with components/site/json-ld.tsx.
 */
import { DOMAIN, GITHUB } from './release';

export const SITE_NAME = 'Shotmaxxing';
export const TAGLINE = 'Beautiful screen recordings, by you or your AI';
export const DESCRIPTION =
  'Shotmaxxing is a free, open-source screen recorder and studio editor for Mac. Record yourself or let an AI agent film the demo: it adds automatic zooms, smooths the cursor and frames the recording on a beautiful background.';

/** An absolute address on the site. */
export const url = (path = '/') => `${DOMAIN}${path === '/' ? '' : path}`;

export const organization = {
  '@type': 'Organization',
  '@id': `${DOMAIN}/#organization`,
  name: SITE_NAME,
  url: DOMAIN,
  logo: `${DOMAIN}/icon-512.png`,
  sameAs: [GITHUB],
};

export const website = {
  '@type': 'WebSite',
  '@id': `${DOMAIN}/#website`,
  name: SITE_NAME,
  url: DOMAIN,
  description: DESCRIPTION,
  publisher: { '@id': `${DOMAIN}/#organization` },
  inLanguage: 'en',
};

export function softwareApplication(version: string) {
  return {
    '@type': 'SoftwareApplication',
    '@id': `${DOMAIN}/#app`,
    name: SITE_NAME,
    description: DESCRIPTION,
    url: DOMAIN,
    applicationCategory: 'MultimediaApplication',
    applicationSubCategory: 'Screen recorder',
    operatingSystem: 'macOS 14 or later',
    softwareVersion: version,
    license: 'https://www.gnu.org/licenses/agpl-3.0.html',
    isAccessibleForFree: true,
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    downloadUrl: `${DOMAIN}/#install`,
    screenshot: `${DOMAIN}/hero/hero-poster.jpg`,
    codeRepository: GITHUB,
    publisher: { '@id': `${DOMAIN}/#organization` },
    featureList: [
      'Automatic zooms on clicks and typing',
      'Smooth, bigger cursor with eight styles',
      'Photo, colour and gradient backgrounds with blur',
      'AI agents film demos over MCP',
      'Captions made on your computer',
      'Export MP4, GIF and 4K',
    ],
  };
}

export function breadcrumbs(trail: [name: string, path: string][]) {
  return {
    '@type': 'BreadcrumbList',
    itemListElement: [['Home', '/'] as [string, string], ...trail].map(([name, path], i) => ({
      '@type': 'ListItem',
      position: i + 1,
      name,
      item: url(path),
    })),
  };
}

export function faqPage(faq: [question: string, answer: string][]) {
  return {
    '@type': 'FAQPage',
    mainEntity: faq.map(([q, a]) => ({
      '@type': 'Question',
      name: q,
      acceptedAnswer: { '@type': 'Answer', text: a },
    })),
  };
}

/** Several schema.org things as one graph. */
export const graph = (...things: object[]) => ({ '@context': 'https://schema.org', '@graph': things });
