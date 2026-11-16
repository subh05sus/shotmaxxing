import { IndexPage, pageMetadata } from '@/components/marketing/pages';
import { COMPARISONS } from '@/lib/marketing';

export const metadata = pageMetadata({
  path: '/compare',
  title: 'Shotmaxxing compared: Screen Studio, Recordly, Loom and Cap',
  description:
    'How Shotmaxxing, the free open-source screen studio with AI agents, compares with Screen Studio, Recordly, Loom and Cap. Honest tables, including when the other app is the better pick.',
});

export default function Compare() {
  return (
    <IndexPage
      eyebrow="Compare"
      title="How it compares."
      lede="Side-by-side tables with the apps people weigh Shotmaxxing against, including when the other one is the better pick."
      path="/compare"
      links={COMPARISONS.map((c) => ({ href: c.path, title: c.eyebrow, text: c.description }))}
    />
  );
}
