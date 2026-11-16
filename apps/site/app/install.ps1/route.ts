/**
 * `irm https://shotmaxxing.com/install.ps1 | iex`: the Windows installer, served from the site so the line is short.
 * It's the repository's install.ps1, read when the site is built.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ROOT } from '@/lib/release';

export const dynamic = 'force-static';

export function GET() {
  return new Response(readFileSync(join(ROOT, 'install.ps1'), 'utf8'), {
    headers: { 'content-type': 'text/plain; charset=utf-8', 'x-content-type-options': 'nosniff' },
  });
}
