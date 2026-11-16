/**
 * `curl -fsSL https://shotmaxxing.com/install | bash`: the macOS installer, served from the site so the line is short.
 * It's the repository's install.sh, read when the site is built.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ROOT } from '@/lib/release';

export const dynamic = 'force-static';

export function GET() {
  return new Response(readFileSync(join(ROOT, 'install.sh'), 'utf8'), {
    headers: { 'content-type': 'text/x-shellscript; charset=utf-8', 'x-content-type-options': 'nosniff' },
  });
}
