import { getNav } from '@/lib/docs';
import { MARKETING_PAGES } from '@/lib/marketing';
import { GITHUB } from '@/lib/release';
import { DESCRIPTION, url } from '@/lib/seo';

export const dynamic = 'force-static';

/** llms.txt (https://llmstxt.org): what Shotmaxxing is and where each guide is, for AI assistants. */
export async function GET() {
  const sections = await getNav();
  const lines = [
    '# Shotmaxxing',
    '',
    `> ${DESCRIPTION}`,
    '',
    'Free and open source (AGPL-3.0), no account or watermark. macOS 14 or later; Windows builds are in testing.',
    'AI agents (Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode, Gemini CLI) connect over MCP',
    'and can plan and film demo takes; the person allows each take first and can stop it with Esc three times.',
    '',
    `- Website: ${url('/')}`,
    `- Source code: ${GITHUB}`,
    `- Every guide in one file: ${url('/llms-full.txt')}`,
    '',
    ...sections.flatMap((s) => [
      `## ${s.title}`,
      '',
      ...s.pages.map(
        (p) => `- [${p.title}](${url(`/docs/${p.slug}`)})${p.lede ? `: ${p.lede.replace(/\s+/g, ' ')}` : ''}`,
      ),
      '',
    ]),
    '## Compare and use cases',
    '',
    ...MARKETING_PAGES.map((p) => `- [${p.title}](${url(p.path)}): ${p.description}`),
    '',
    '## Optional',
    '',
    `- [Press kit](${url('/press')}): descriptions, facts, logos, screenshots and videos`,
    `- [Changelog](${GITHUB}/blob/main/CHANGELOG.md)`,
    '',
  ];
  return new Response(lines.join('\n'), { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
