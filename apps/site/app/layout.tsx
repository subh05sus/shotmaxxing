import type { Metadata, Viewport } from 'next';
import { Bricolage_Grotesque, Geist, Geist_Mono } from 'next/font/google';
import { IrisLoader } from '@/components/site/iris-loader';
import { SITE } from '@/lib/release';
import './globals.css';

const display = Bricolage_Grotesque({ subsets: ['latin'], variable: '--font-bricolage', display: 'swap' });
const sans = Geist({ subsets: ['latin'], variable: '--font-geist', display: 'swap' });
const mono = Geist_Mono({ subsets: ['latin'], variable: '--font-geist-mono', display: 'swap' });

const site = SITE ?? 'http://localhost:3100';

export const metadata: Metadata = {
  metadataBase: new URL(site),
  title: { default: 'Shotmaxxing: demos that edit themselves', template: '%s · Shotmaxxing' },
  description:
    'An open-source screen recorder and studio editor. Record by hand or let an AI agent film it: Shotmaxxing adds the zooms, smooths the cursor and keeps your computer cool.',
  applicationName: 'Shotmaxxing',
  openGraph: { type: 'website', siteName: 'Shotmaxxing' },
  twitter: { card: 'summary_large_image' },
};

export const viewport: Viewport = { colorScheme: 'dark' };

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
