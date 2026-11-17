/**
 * What the app's updater asks for: the newest release's latest.json (its version, and each platform's signed
 * download), straight from GitHub. Every release so far is a pre-release, which GitHub's `releases/latest` leaves
 * out, so this picks the highest published version itself. 204 means there's nothing to offer.
 *
 * Kept for five minutes, so GitHub sees a handful of requests an hour however many apps ask.
 */
import { REPO } from '@/lib/release';

export const revalidate = 300;

type Asset = { name: string; browser_download_url: string };
type Release = { tag_name: string; draft: boolean; assets: Asset[] };

function version(tag: string): number[] {
  return tag
    .replace(/^v/, '')
    .split(/[.-]/)
    .slice(0, 3)
    .map((n) => Number.parseInt(n, 10) || 0);
}

function newer(a: string, b: string): boolean {
  const [x, y] = [version(a), version(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

export async function GET() {
  const headers: Record<string, string> = { Accept: 'application/vnd.github+json' };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const none = new Response(null, { status: 204 });
  try {
    const response = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=20`, {
      headers,
      next: { revalidate },
    });
    if (!response.ok) return new Response(null, { status: 502 });
    const releases = (await response.json()) as Release[];
    // Only releases built with the updater carry latest.json; older ones are left out.
    const newest = releases
      .filter((r) => !r.draft && r.assets.some((a) => a.name === 'latest.json'))
      .reduce<Release | undefined>(
        (best, r) => (!best || newer(r.tag_name, best.tag_name) ? r : best),
        undefined,
      );
    const asset = newest?.assets.find((a) => a.name === 'latest.json');
    if (!asset) return none;
    const manifest = await fetch(asset.browser_download_url, { next: { revalidate } });
    if (!manifest.ok) return new Response(null, { status: 502 });
    return Response.json(await manifest.json(), {
      headers: { 'cache-control': 'public, max-age=300, s-maxage=300' },
    });
  } catch {
    return new Response(null, { status: 502 });
  }
}
