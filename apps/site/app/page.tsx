import { Nav } from '@/components/site/nav';
import { Rise } from '@/components/site/rise';
import { Hero } from '@/components/landing/hero';
import { Install } from '@/components/landing/install';
import { Tour } from '@/components/landing/tour';
import { Agents, Faq, FinalCta, Footer } from '@/components/landing/more';
import { release } from '@/lib/release';

export default function Home() {
  const r = release();
  return (
    <>
      <Nav />
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
