/**
 * The content packs added on this computer. Their wallpapers and looks show in the editor's Background tool, and
 * Settings adds and removes them. Each window reads them when it needs them: the Background tool reads them again
 * each time it opens, so a pack added in Settings shows there without a restart.
 */

import { listPacks, type ContentPack, type PackLook } from './native';
import { WALLPAPERS } from './wallpapers';

export const packs = $state({ list: [] as ContentPack[], loaded: false });

/** Reads the packs again. Failing (a broken pack folder) leaves the list as it was. */
export async function loadPacks(): Promise<void> {
  try {
    packs.list = await listPacks();
  } catch {
    // Keep what was shown.
  } finally {
    packs.loaded = true;
  }
}

/** The id a project uses for one of a pack's wallpapers. */
export function packWallpaperId(pack: ContentPack, id: string): string {
  return `${pack.id}.${id}`;
}

/** A look's wallpaper as a project keeps it: the id, and the CSS when it's the pack's own. */
export function lookWallpaper(pack: ContentPack, look: PackLook): { id: string; css?: string } | null {
  if (!look.wallpaper) return null;
  const own = pack.wallpapers.find((w) => w.id === look.wallpaper);
  if (own) return { id: packWallpaperId(pack, own.id), css: own.css };
  return WALLPAPERS.some((w) => w.id === look.wallpaper) ? { id: look.wallpaper } : null;
}
