/**
 * Updates, from finding one to opening the new version:
 *
 * - At launch and every six hours (while checking is on in Settings), asks whether a newer version exists.
 * - A newer one downloads quietly straight away, unless the person skipped that version. The sidebar shows a pill
 *   that opens the update dialog: what changed in every version since theirs, read from CHANGELOG.md at its tag.
 * - It never restarts by itself: Restart to update installs it and reopens the app, and is refused while recording
 *   or working, with the reason.
 * - The first launch of a new version shows what's new in it (and in any version skipped over on the way).
 *
 * Only GitHub and shotmaxxing.app are asked, and nothing about the person is sent.
 */

import { toast } from '@shotmaxxing/ui';
import CHANGELOG from '../../../../CHANGELOG.md?raw';
import { between, isNewer, parseChangelog, type Version } from './changelog';
import {
  checkUpdate,
  downloadUpdate,
  installUpdate,
  isTauri,
  onUpdateProgress,
  type FoundUpdate,
} from './native';
import { settings } from './settings.svelte';

export const CURRENT = import.meta.env.VITE_APP_VERSION;
export const CHANGELOG_URL = 'https://github.com/subh05sus/shotmaxxing/blob/main/CHANGELOG.md';
const RAW = 'https://raw.githubusercontent.com/subh05sus/shotmaxxing';
const EVERY_MS = 6 * 60 * 60 * 1000;

export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'current'
  | 'unreachable'
  | 'available'
  | 'downloading'
  | 'failed'
  | 'ready'
  | 'installing';

export const updates = $state({
  status: 'idle' as UpdateStatus,
  found: null as FoundUpdate | null,
  /** 0 to 1 while downloading, or null when the size isn't known. */
  progress: null as number | null,
  /** The download's size in bytes, once known. */
  bytes: null as number | null,
  error: '',
  checkedAt: 0,
  /** What changed in every version after this one up to the update; null while it loads, [] if it couldn't. */
  notes: null as Version[] | null,
  dialog: false,
  /** What's new in the version just opened (and any skipped on the way); shown once, after an update. */
  whatsNew: [] as Version[],
});

/** Whether the sidebar offers the update: one exists and the person hasn't skipped it. */
export function offered(): boolean {
  const found = updates.found;
  return (
    !!found &&
    found.version !== settings.skippedVersion &&
    ['available', 'downloading', 'failed', 'ready', 'installing'].includes(updates.status)
  );
}

async function loadNotes(found: FoundUpdate) {
  updates.notes = null;
  try {
    // The preview has no release to read, so it shows the notes the app came with.
    const markdown = isTauri
      ? await fetch(`${RAW}/v${found.version}/CHANGELOG.md`).then((r) => (r.ok ? r.text() : Promise.reject()))
      : CHANGELOG;
    const from = isTauri ? found.current : '0.1.0';
    if (updates.found?.version === found.version)
      updates.notes = between(parseChangelog(markdown), from, found.version);
  } catch {
    if (updates.found?.version === found.version) updates.notes = [];
  }
}

/** Looks for a newer version. `manual` (Check for updates) opens the dialog when one exists, even a skipped one. */
export async function check(manual = false): Promise<void> {
  if (updates.status === 'checking' || updates.status === 'downloading' || updates.status === 'installing') {
    if (manual && updates.found) updates.dialog = true;
    return;
  }
  const before = updates.status;
  updates.status = 'checking';
  let found: FoundUpdate | null;
  try {
    found = await checkUpdate();
  } catch {
    updates.status = updates.found ? before : 'unreachable';
    if (manual)
      toast('Couldn’t reach the update server. Check your connection and try again.', { tone: 'danger' });
    return;
  }
  updates.checkedAt = Date.now();
  settings.updatesCheckedAt = updates.checkedAt;
  if (!found) {
    updates.found = null;
    updates.status = 'current';
    if (manual) toast(`Shotmaxxing ${CURRENT} is the newest version.`, { tone: 'success' });
    return;
  }
  const isNew = updates.found?.version !== found.version;
  updates.found = found;
  if (isNew) void loadNotes(found);
  if (found.ready) updates.status = 'ready';
  else if (isNew || before !== 'ready') updates.status = 'available';
  if (manual) updates.dialog = true;
  if (updates.status === 'available' && (manual || found.version !== settings.skippedVersion))
    void download();
}

/** Downloads the update found, in the background. */
export async function download(): Promise<void> {
  if (!updates.found || updates.status === 'downloading' || updates.status === 'ready') return;
  const version = updates.found.version;
  updates.status = 'downloading';
  updates.progress = 0;
  updates.error = '';
  const stop = await onUpdateProgress(({ done, total }) => {
    updates.bytes = total;
    updates.progress = total ? Math.min(1, done / total) : null;
  });
  try {
    await downloadUpdate();
    if (updates.found?.version === version) updates.status = 'ready';
  } catch (error) {
    updates.status = 'failed';
    updates.error = String(error instanceof Error ? error.message : error);
  } finally {
    stop();
  }
}

/** Installs the downloaded update and reopens the app on it. */
export async function install(): Promise<void> {
  if (updates.status !== 'ready') return;
  updates.status = 'installing';
  try {
    await installUpdate();
  } catch (error) {
    updates.status = 'ready';
    toast(String(error instanceof Error ? error.message : error), { tone: 'danger', timeout: 8000 });
  }
}

/** Stops offering this version; a newer one is offered again. */
export function skip(): void {
  if (updates.found) settings.skippedVersion = updates.found.version;
  updates.dialog = false;
}

let started = false;

/** Starts the checks (once per window) and works out whether this launch follows an update. */
export function startUpdates(): void {
  if (started) return;
  started = true;

  const last = settings.lastOpenedVersion;
  if (last !== CURRENT) {
    const versions = parseChangelog(CHANGELOG);
    // Someone who used a version from before this was noted sees what's new in this one.
    if (last && isNewer(CURRENT, last)) updates.whatsNew = between(versions, last, CURRENT);
    else if (!last && settings.onboardingDone)
      updates.whatsNew = versions.filter((v) => v.version === CURRENT);
    settings.lastOpenedVersion = CURRENT;
  }

  const run = () => {
    if (settings.checkUpdates) void check();
  };
  run();
  setInterval(run, EVERY_MS);
}
