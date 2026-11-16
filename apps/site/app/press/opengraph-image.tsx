import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'Shotmaxxing press kit';
export const size = OG_SIZE;
export const contentType = 'image/png';

export default function Image() {
  return ogImage({ eyebrow: 'Press kit', title: 'Logos, screenshots, videos and facts.' });
}
