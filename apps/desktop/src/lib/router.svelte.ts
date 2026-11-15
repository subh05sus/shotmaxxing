/**
 * A tiny hash router. Each Tauri window loads the same bundle at a different hash (`#/home/library`, `#/editor/…`),
 * so a window only builds the screen it shows.
 */

import { isTauri, openWindow } from './native';

export const route = $state({ path: [] as string[], query: new URLSearchParams() });

function read() {
  const raw = location.hash.replace(/^#\/?/, '');
  const [path, query = ''] = raw.split('?');
  route.path = path.split('/').filter(Boolean);
  route.query = new URLSearchParams(query);
}

export function startRouter(): () => void {
  read();
  window.addEventListener('hashchange', read);
  return () => window.removeEventListener('hashchange', read);
}

/**
 * Shows `path`. In the app, Home pages live in the Home window and each project in its own editor window, so a page
 * of the other kind opens in its own window and this one stays as it is.
 */
export function go(path: string): void {
  const next = path.startsWith('#') ? path : `#/${path.replace(/^\//, '')}`;
  const target = next.replace(/^#\/?/, '');
  const kind = target.split(/[/?]/)[0];
  if (isTauri && kind !== route.path[0] && (kind === 'home' || kind === 'editor')) {
    void openWindow(kind, kind === 'home' ? target : target.slice('editor/'.length) || undefined);
    return;
  }
  if (location.hash !== next) location.hash = next;
}
