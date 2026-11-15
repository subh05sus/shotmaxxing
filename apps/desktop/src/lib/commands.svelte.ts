/**
 * The command registry behind ⌘K: the Home window's search and the editor's palette. Screens register their
 * commands while they're shown, so both always offer what makes sense right now, plus the global ones.
 */

import type { Command } from '@shotmaxxing/ui';

const sources = $state<Record<string, Command[]>>({});

export function registerCommands(source: string, commands: Command[]): () => void {
  sources[source] = commands;
  return () => {
    delete sources[source];
  };
}

export function allCommands(): Command[] {
  return Object.values(sources).flat();
}

export const ui = $state({ palette: false, shortcuts: false, tour: false, tourStart: 0 });
