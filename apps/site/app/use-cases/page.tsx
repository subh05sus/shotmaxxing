import { IndexPage, pageMetadata } from '@/components/marketing/pages';
import { USE_CASES } from '@/lib/marketing';

export const metadata = pageMetadata({
  path: '/use-cases',
  title: 'What people make with Shotmaxxing',
  description:
    'Product demos, tutorials, launch videos and demos filmed by AI agents: what Shotmaxxing, the free open-source screen studio, is for.',
});

export default function UseCases() {
  return (
    <IndexPage
      eyebrow="Use cases"
      title="What people make with it."
      lede="Product demos for the landing page, tutorials for the docs, launch videos for every network, and demos your AI agent films for you."
      path="/use-cases"
      links={USE_CASES.map((u) => ({ href: u.path, title: u.name, text: u.description }))}
    />
  );
}
