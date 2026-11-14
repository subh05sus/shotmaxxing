/**
 * Built-in wallpapers, drawn from gradients so they cost no download and scale to any size. The compositor reads
 * the same definitions. Colour literals are allowed here (this is data, not styling).
 */

export interface Wallpaper {
  id: string;
  name: string;
  css: string;
}

export const WALLPAPERS: Wallpaper[] = [
  {
    id: 'aurora-lime',
    name: 'Aurora lime',
    css: 'radial-gradient(120% 90% at 15% 10%, oklch(0.88 0.2 128) 0%, transparent 55%), radial-gradient(90% 80% at 85% 90%, oklch(0.55 0.14 160) 0%, transparent 60%), linear-gradient(135deg, oklch(0.3 0.05 150), oklch(0.18 0.03 200))',
  },
  {
    id: 'graphite',
    name: 'Graphite',
    css: 'radial-gradient(100% 80% at 20% 0%, oklch(0.42 0.01 264) 0%, transparent 60%), linear-gradient(160deg, oklch(0.3 0.008 264), oklch(0.16 0.006 264))',
  },
  {
    id: 'dusk',
    name: 'Dusk',
    css: 'radial-gradient(110% 90% at 80% 0%, oklch(0.72 0.16 40) 0%, transparent 55%), radial-gradient(90% 90% at 10% 100%, oklch(0.45 0.18 300) 0%, transparent 60%), linear-gradient(150deg, oklch(0.35 0.12 330), oklch(0.2 0.06 280))',
  },
  {
    id: 'ocean',
    name: 'Ocean',
    css: 'radial-gradient(100% 80% at 0% 0%, oklch(0.78 0.11 210) 0%, transparent 55%), radial-gradient(90% 90% at 100% 100%, oklch(0.4 0.14 260) 0%, transparent 60%), linear-gradient(140deg, oklch(0.45 0.12 230), oklch(0.22 0.07 250))',
  },
  {
    id: 'peach',
    name: 'Peach',
    css: 'radial-gradient(110% 90% at 85% 15%, oklch(0.92 0.06 60) 0%, transparent 55%), radial-gradient(100% 80% at 10% 90%, oklch(0.78 0.12 25) 0%, transparent 60%), linear-gradient(135deg, oklch(0.85 0.08 45), oklch(0.7 0.12 20))',
  },
  {
    id: 'mint',
    name: 'Mint',
    css: 'radial-gradient(100% 80% at 20% 10%, oklch(0.95 0.06 160) 0%, transparent 55%), linear-gradient(145deg, oklch(0.86 0.09 165), oklch(0.7 0.1 190))',
  },
  {
    id: 'violet-haze',
    name: 'Violet haze',
    css: 'radial-gradient(100% 90% at 75% 10%, oklch(0.8 0.12 300) 0%, transparent 55%), radial-gradient(90% 80% at 15% 95%, oklch(0.55 0.2 280) 0%, transparent 60%), linear-gradient(150deg, oklch(0.5 0.16 290), oklch(0.25 0.1 280))',
  },
  {
    id: 'paper',
    name: 'Paper',
    css: 'linear-gradient(160deg, oklch(0.97 0.005 90), oklch(0.91 0.01 90))',
  },
  {
    id: 'midnight',
    name: 'Midnight',
    css: 'radial-gradient(80% 70% at 50% 0%, oklch(0.35 0.08 264) 0%, transparent 70%), linear-gradient(180deg, oklch(0.18 0.03 264), oklch(0.1 0.01 264))',
  },
];

export function wallpaper(id: string): Wallpaper {
  return WALLPAPERS.find((w) => w.id === id) ?? WALLPAPERS[0];
}
