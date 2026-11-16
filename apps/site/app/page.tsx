import type { Metadata } from 'next';
import { Nav } from '@/components/site/nav';
import { Rise } from '@/components/site/rise';
import { Hero } from '@/components/landing/hero';
import { Install } from '@/components/landing/install';
import { Tour } from '@/components/landing/tour';
import { Agents, Faq, FAQ_TEXT, FinalCta, Footer } from '@/components/landing/more';
import { JsonLd } from '@/components/site/json-ld';
import { faqPage, graph, organization, softwareApplication, website } from '@/lib/seo';
import { release } from '@/lib/release';

export const metadata: Metadata = { alternates: { canonical: '/' } };

export default function Home() {
  const r = release();
  return (
    <>
      <JsonLd data={graph(organization, website, softwareApplication(r.version), faqPage(FAQ_TEXT))} />
      <Nav dock />
      <main>
        <Hero release={r} />
        <Tour />
        <Agents />
        <Install release={r} />
        <Faq />
        <FinalCta release={r} />
      </main>
      <Footer version={r.version} />
      <Rise />
    </>
  );
}
