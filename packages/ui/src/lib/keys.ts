/**
 * Key chords for display, matching the Rust core's rules: `mod` is Command on macOS and Ctrl on Windows, and many
 * spellings are accepted (`cmd+shift+z`, `⌘⇧Z`, `Ctrl-Shift-Z`).
 */

import type { OS } from './env.svelte';

export interface Chord {
  primary: boolean;
  command: boolean;
  control: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
}

const NAMED: Record<string, string> = {
  return: 'Return',
  enter: 'Return',
  esc: 'Escape',
  escape: 'Escape',
  tab: 'Tab',
  space: 'Space',
  backspace: 'Backspace',
  delete: 'Delete',
  del: 'Delete',
  up: 'Up',
  down: 'Down',
  left: 'Left',
  right: 'Right',
  home: 'Home',
  end: 'End',
  pageup: 'PageUp',
  pagedown: 'PageDown',
};

export function parseChord(input: string): Chord | null {
  const expanded = input
    .trim()
    .replace(/⌘/g, 'cmd+')
    .replace(/⇧/g, 'shift+')
    .replace(/⌥/g, 'alt+')
    .replace(/⌃/g, 'ctrl+');
  let tokens = expanded.split(/[+\s]+/).filter(Boolean);
  if (expanded.endsWith('++')) tokens.push('+');
  if (tokens.length === 1 && /^[a-z]+-./i.test(tokens[0]) && !tokens[0].endsWith('-'))
    tokens = tokens[0].split('-');
  const chord: Chord = { primary: false, command: false, control: false, alt: false, shift: false, key: '' };
  for (const token of tokens) {
    const t = token.toLowerCase();
    if (['cmd', 'command', 'meta', 'super', 'win'].includes(t)) chord.command = true;
    else if (['ctrl', 'control'].includes(t)) chord.control = true;
    else if (['alt', 'option', 'opt'].includes(t)) chord.alt = true;
    else if (t === 'shift') chord.shift = true;
    else if (['mod', 'primary', 'cmdorctrl'].includes(t)) chord.primary = true;
    else if (chord.key) return null;
    else
      chord.key =
        NAMED[t] ??
        (/^f([1-9]|1\d|2[0-4])$/.test(t) ? t.toUpperCase() : token.length === 1 ? token.toUpperCase() : '');
  }
  return chord.key ? chord : null;
}

const MAC_KEYS: Record<string, string> = {
  Return: '↩',
  Escape: 'esc',
  Tab: '⇥',
  Backspace: '⌫',
  Delete: '⌦',
  Up: '↑',
  Down: '↓',
  Left: '←',
  Right: '→',
  Space: 'Space',
};

const WIN_KEYS: Record<string, string> = {
  Return: 'Enter',
  Escape: 'Esc',
  Delete: 'Del',
  Up: '↑',
  Down: '↓',
  Left: '←',
  Right: '→',
};

/** The keycaps to draw for a chord, in platform order. */
export function chordCaps(input: string, os: OS): string[] {
  const chord = parseChord(input);
  if (!chord) return [input];
  const command = chord.command || (chord.primary && os === 'mac');
  const control = chord.control || (chord.primary && os === 'windows');
  if (os === 'mac') {
    const caps: string[] = [];
    if (control) caps.push('⌃');
    if (chord.alt) caps.push('⌥');
    if (chord.shift) caps.push('⇧');
    if (command) caps.push('⌘');
    caps.push(MAC_KEYS[chord.key] ?? chord.key);
    return caps;
  }
  const caps: string[] = [];
  if (control) caps.push('Ctrl');
  if (command) caps.push('Win');
  if (chord.alt) caps.push('Alt');
  if (chord.shift) caps.push('Shift');
  caps.push(WIN_KEYS[chord.key] ?? chord.key);
  return caps;
}

/** A chord as one string: `⇧⌘Z` on macOS, `Ctrl+Shift+Z` on Windows. */
export function chordLabel(input: string, os: OS): string {
  const caps = chordCaps(input, os);
  return os === 'mac' ? caps.join('') : caps.join('+');
}

/** Whether a keyboard event matches a chord. */
export function matchesChord(event: KeyboardEvent, input: string, os: OS): boolean {
  const chord = parseChord(input);
  if (!chord) return false;
  const wantMeta = chord.command || (chord.primary && os === 'mac');
  const wantCtrl = chord.control || (chord.primary && os === 'windows');
  if (event.metaKey !== wantMeta || event.ctrlKey !== wantCtrl) return false;
  if (event.altKey !== chord.alt || event.shiftKey !== chord.shift) return false;
  const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
  const named: Record<string, string> = {
    Enter: 'Return',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    ' ': 'Space',
  };
  return (
    (named[event.key] ?? key) === chord.key || (event.code === `Key${chord.key}` && chord.key.length === 1)
  );
}
