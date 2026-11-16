import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'Shotmaxxing guides';
export const size = OG_SIZE;
export const contentType = 'image/png';

export default function Image() {
  return ogImage({ eyebrow: 'Guides', title: 'Everything, step by step.' });
}
