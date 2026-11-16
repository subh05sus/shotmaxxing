/**
 * Allow everything: agents' requests go through without the take card asking. The app holds the switch in memory
 * and turns it off when it quits, so no file can turn it on. Every window keeps this copy in step with the app.
 */

import { toast } from '@shotmaxxing/ui';
import { agentTrust, onAgentTrust, setAgentTrust } from './native';
import { settings } from './settings.svelte';

export const trust = $state({
  on: false,
  /** The first-time question is showing. */
  confirming: false,
});

let watching = false;

/** Reads the switch from the app and follows its changes. Once per window. */
export function watchTrust(): void {
  if (watching) return;
  watching = true;
  void agentTrust().then((on) => (trust.on = on));
  void onAgentTrust((on) => {
    trust.on = on;
    if (!on) trust.confirming = false;
  });
}

/** Turns Allow everything on or off. Turning it on asks first, the first time. */
export function turnTrust(on: boolean): void {
  if (on && !settings.allowEverythingConfirmed) {
    trust.confirming = true;
    return;
  }
  void apply(on);
}

/** The person read the first-time question and turned it on. */
export function confirmTrust(): void {
  settings.allowEverythingConfirmed = true;
  trust.confirming = false;
  void apply(true);
}

async function apply(on: boolean): Promise<void> {
  try {
    await setAgentTrust(on);
    trust.on = on;
  } catch (error) {
    toast(`Couldn’t turn Allow everything ${on ? 'on' : 'off'}: ${error}`, { tone: 'danger' });
  }
}
