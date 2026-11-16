import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'Shotmaxxing: beautiful screen recordings, by you or your AI';
export const size = OG_SIZE;
export const contentType = 'image/png';

export default function Image() {
  return ogImage({
    eyebrow: 'Screen recorder and studio editor',
    title: 'Beautiful screen recordings, by you or your AI.',
  });
}
