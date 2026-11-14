/**
 * Built-in wallpapers, drawn from gradients so they cost no download and scale to any size. The definitions live in
 * crates/render/assets/wallpapers.json, which the native exporter draws from too, so exports match the preview.
 */

import DATA from '../../../../crates/render/assets/wallpapers.json';

export interface Wallpaper {
  id: string;
  name: string;
  css: string;
}

export const WALLPAPERS: Wallpaper[] = DATA;

export function wallpaper(id: string): Wallpaper {
  return WALLPAPERS.find((w) => w.id === id) ?? WALLPAPERS[0];
}
