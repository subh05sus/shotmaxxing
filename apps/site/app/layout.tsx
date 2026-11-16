import type { Metadata, Viewport } from 'next';
import { Bricolage_Grotesque, Geist, Geist_Mono } from 'next/font/google';
import { IrisLoader } from '@/components/site/iris-loader';
import { GITHUB, SITE } from '@/lib/release';
import { DESCRIPTION, SITE_NAME } from '@/lib/seo';
import './globals.css';

const display = Bricolage_Grotesque({ subsets: ['latin'], variable: '--font-bricolage', display: 'swap' });
const sans = Geist({ subsets: ['latin'], variable: '--font-geist', display: 'swap' });
const mono = Geist_Mono({ subsets: ['latin'], variable: '--font-geist-mono', display: 'swap' });

const site = SITE ?? 'http://localhost:3100';

export const metadata: Metadata = {
  metadataBase: new URL(site),
  title: {
    default: `${SITE_NAME}: beautiful screen recordings, by you or your AI`,
    template: `%s · ${SITE_NAME}`,
  },
  description: DESCRIPTION,
  applicationName: SITE_NAME,
  keywords: [
    'screen recorder',
    'screen recording software',
    'Mac screen recorder',
    'open source screen recorder',
    'Screen Studio alternative',
    'product demo video',
    'auto zoom screen recording',
    'AI agent screen recording',
    'MCP screen recorder',
    'Claude Code',
  ],
  authors: [{ name: 'The Shotmaxxing contributors', url: GITHUB }],
  creator: SITE_NAME,
  publisher: SITE_NAME,
  category: 'technology',
  openGraph: { type: 'website', siteName: SITE_NAME, locale: 'en_US' },
  twitter: { card: 'summary_large_image' },
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      'max-image-preview': 'large',
      'max-snippet': -1,
      'max-video-preview': -1,
    },
  },
  formatDetection: { telephone: false, email: false, address: false },
};

// The browser's own bar, which can't read CSS variables: the stage colour. design-guard-allow colour
export const viewport: Viewport = { colorScheme: 'dark', themeColor: '#0e0f11' }; // design-guard-allow colour

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html
      lang="en"
      className={`${display.variable} ${sans.variable} ${mono.variable}`}
      suppressHydrationWarning
    >
      <head>
        {/* Scroll scenes hide what they'll bring in only when scripts run and motion is on. */}
        <script
          dangerouslySetInnerHTML={{
            __html:
              "document.documentElement.classList.add('js');if(matchMedia('(prefers-reduced-motion: reduce)').matches)document.documentElement.classList.add('reduce-motion')",
          }}
        />
        <noscript>
          <style>{'.iris-loader{display:none}'}</style>
        </noscript>
      </head>
      <body>
        <IrisLoader />
        {children}
      </body>
    </html>
  );
}
