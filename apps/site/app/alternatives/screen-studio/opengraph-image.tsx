import { OG_SIZE, ogImage } from '@/lib/og';

export const alt = 'The free, open-source Screen Studio alternative';
export const size = OG_SIZE;
export const contentType = 'image/png';

export default function Image() {
  return ogImage({
    eyebrow: 'Screen Studio alternative',
    title: 'The free, open-source Screen Studio alternative.',
  });
}
