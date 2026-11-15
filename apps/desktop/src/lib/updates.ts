/**
 * Once a day, asks GitHub whether a newer release exists and offers it in a toast. Nothing else is sent. It stays
 * quiet when offline, when there is nothing new, and when the check is off in Settings.
 */

import { toast } from '@shotmaxxing/ui';
import { isTauri, openLink } from './native';
import { settings } from './settings.svelte';

// Every release so far is a preview, marked as a pre-release, which `releases/latest` leaves out: so the list.
const RELEASES = 'https://api.github.com/repos/subh05sus/shotmaxxing/releases?per_page=20';
const DAY_MS = 24 * 60 * 60 * 1000;

/** Whether version `a` is newer than `b` (`v1.2.3` and `1.2.3` both work). */
export function isNewer(a: string, b: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/, '')
      .split(/[.-]/)
      .slice(0, 3)
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
}

export async function checkForUpdates(): Promise<void> {
  if (!isTauri || !settings.checkUpdates || Date.now() - settings.updatesCheckedAt < DAY_MS) return;
  settings.updatesCheckedAt = Date.now();
  try {
    const response = await fetch(RELEASES, { headers: { Accept: 'application/vnd.github+json' } });
    if (!response.ok) return;
    const releases = (await response.json()) as { tag_name?: string; html_url?: string; draft?: boolean }[];
    // The highest version published, whichever came out last.
    const newest = releases
      .filter((r) => !r.draft && r.tag_name && r.html_url)
      .reduce<(typeof releases)[number] | undefined>(
        (best, r) => (!best || isNewer(r.tag_name!, best.tag_name!) ? r : best),
        undefined,
      );
    const tag = newest?.tag_name;
    const url = newest?.html_url;
    if (!tag || !url || !isNewer(tag, import.meta.env.VITE_APP_VERSION)) return;
    toast(`Shotmaxxing ${tag.replace(/^v/, '')} is out.`, {
      timeout: 15_000,
      action: { label: 'Download', run: () => void openLink(url) },
    });
  } catch {
    // Offline or rate-limited: try again tomorrow.
  }
}
