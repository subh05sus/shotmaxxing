/**
 * A tiny hash router. Each Tauri window loads the same bundle at a different hash (`#/home/library`, `#/editor/…`),
 * so a window only builds the screen it shows.
 */

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

export function go(path: string): void {
  const next = path.startsWith('#') ? path : `#/${path.replace(/^\//, '')}`;
  if (location.hash !== next) location.hash = next;
}
