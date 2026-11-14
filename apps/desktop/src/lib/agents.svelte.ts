/**
 * The agents Shotmaxxing can work with, and their state on this computer. The app detects which are installed and
 * connected; a browser preview shows the sample states.
 */

import { counts } from './counts.svelte';
import { detectAgents, isTauri } from './native';
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
