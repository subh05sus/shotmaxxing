import type { MetadataRoute } from 'next';
import { DESCRIPTION } from '@/lib/seo';

// The stage colour, for the splash and the browser's bar (a manifest can't read CSS variables). design-guard-allow colour
const STAGE = '#0e0f11'; // design-guard-allow colour

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: 'Shotmaxxing',
    short_name: 'Shotmaxxing',
    description: DESCRIPTION,
    start_url: '/',
    display: 'standalone',
    background_color: STAGE,
    theme_color: STAGE,
    categories: ['productivity', 'utilities', 'video'],
    icons: [
      { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
      { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
      { src: '/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    ],
  };
}
