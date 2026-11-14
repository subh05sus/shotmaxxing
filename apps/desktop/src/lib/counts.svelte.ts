/**
 * Counts the Home sidebar shows. The app fills them from real data (the Library from the projects folder, agents
 * from what's connected); a browser preview starts from the samples.
 */

import { isTauri } from './native';
import { AGENTS, LIBRARY } from './sample';

export const counts = $state({
  library: isTauri ? 0 : LIBRARY.length,
  connectedAgents: isTauri ? 0 : AGENTS.filter((a) => a.state === 'connected').length,
});
