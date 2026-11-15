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
 * Shows `path`. In the app, Home pages live in the Home window and each project in its own editor window, so a Home
 * page, or a project other than this window's, opens in its own window and this one stays as it is.
 */
export function go(path: string): void {
  const next = path.startsWith('#') ? path : `#/${path.replace(/^\//, '')}`;
  const target = next.replace(/^#\/?/, '');
  const [kind, project] = target.split('?')[0].split('/');
  const [here, shown] = route.path;
  if (isTauri && kind === 'home' && here !== 'home') return void openWindow('home', target);
  if (isTauri && kind === 'editor' && (here !== 'editor' || shown !== project))
    return void openWindow('editor', target.slice('editor/'.length) || undefined);
  if (location.hash !== next) location.hash = next;
}
