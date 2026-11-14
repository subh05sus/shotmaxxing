/**
 * The agents Shotmaxxing can work with, and their state on this computer. The app detects which are installed and
 * connected; a browser preview shows the sample states.
 */

import { toast } from '@shotmaxxing/ui';
import { counts } from './counts.svelte';
import { detectAgents, isTauri, type ConnectResult } from './native';
import { AGENTS, type Agent } from './sample';

export type AgentEntry = Agent & { busy?: boolean };

export const agents = $state<AgentEntry[]>(
  AGENTS.map((a) => (isTauri ? { ...a, state: 'not_installed', lastTake: undefined } : { ...a })),
);

export const agentsLoaded = $state({ value: !isTauri });

export async function refreshAgents(): Promise<void> {
  if (!isTauri) return;
  try {
    for (const status of await detectAgents()) {
      const agent = agents.find((a) => a.id === status.id);
      if (agent)
        agent.state = status.connected ? 'connected' : status.installed ? 'available' : 'not_installed';
    }
  } finally {
    agentsLoaded.value = true;
    counts.connectedAgents = agents.filter((a) => a.state === 'connected').length;
  }
}

/** Says why an agent couldn't be connected, offering to copy the lines to add by hand when that's the way. */
export function connectProblem(result: ConnectResult): void {
  const lines = result.manual?.snippet;
  if (!lines) {
    toast(result.message, { tone: 'danger', timeout: 10000 });
    return;
  }
  toast(result.message, {
    tone: 'danger',
    timeout: 15000,
    action: {
      label: 'Copy the lines',
      run: () =>
        void navigator.clipboard.writeText(lines).then(
          () => toast('Copied. Paste them into the settings file, then restart the agent.'),
          () => toast('Couldn’t reach the clipboard.', { tone: 'danger' }),
        ),
    },
  });
}
