/**
 * What to offer when Screen Recording stops something. macOS gives the permission to a running app only once it
 * reopens, so when the switch is already on in System Settings the way out is reopening; otherwise it's the
 * settings page.
 */

import { toast } from '@shotmaxxing/ui';
import { openPermissionSettings, permissionStatus, relaunch } from './native';

export interface ScreenFix {
  /** On in System Settings already: Shotmaxxing only has to reopen. */
  reopen: boolean;
  label: string;
  run: () => void;
}

/** Whether a message is about Screen Recording not being allowed. */
export const isScreenRecordingProblem = (message: string) => /Screen Recording/i.test(message);

/** The way out, or null when Shotmaxxing can record the screen now (trying again works). */
export async function screenRecordingFix(): Promise<ScreenFix | null> {
  const screen = (await permissionStatus().catch(() => [])).find((p) => p.kind === 'screen');
  if (screen?.state === 'granted' || screen?.state === 'not_needed') return null;
  if (screen?.state === 'needs_restart')
    return { reopen: true, label: 'Reopen', run: () => void reopenApp() };
  return { reopen: false, label: 'Open settings', run: () => void openPermissionSettings('screen') };
}

/** Reopens Shotmaxxing, on the page `to` of Home when given. When it can't (it's recording or exporting), says why
 *  and returns false. */
export async function reopenApp(to?: string): Promise<boolean> {
  try {
    await relaunch(to);
    return true;
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), { tone: 'danger', timeout: 6000 });
    return false;
  }
}
