import { allSlugs, getDoc } from '@/lib/docs';
import { DESCRIPTION, url } from '@/lib/seo';

export const dynamic = 'force-static';

/** Every guide as plain text in one file, for AI assistants that want the whole manual. */
export async function GET() {
  const docs = await Promise.all(allSlugs().map((slug) => getDoc(slug)));
  const body = docs
    .filter((d) => d != null)
    .map((d) => `# ${d.title}\n\nSource: ${url(`/docs/${d.slug}`)}\n\n${d.lede}\n\n${d.text}`)
    .join('\n\n---\n\n');
  return new Response(`# Shotmaxxing guides\n\n> ${DESCRIPTION}\n\n${body}\n`, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
}
