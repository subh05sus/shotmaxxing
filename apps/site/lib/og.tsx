/**
 * Social share images (Open Graph and X), drawn with next/og at build time: the app icon, an eyebrow and a title on
 * the site's dark stage, with the lime horizon glow along the top. Each page group's opengraph-image.tsx uses it.
 */
import { ImageResponse } from 'next/og';

export const OG_SIZE = { width: 1200, height: 630 };

/** Bricolage Grotesque, the site's display face, from Google Fonts (as next/font fetches it). Null when offline. */
async function display(): Promise<ArrayBuffer | null> {
  try {
    const css = await (
      await fetch(
        'https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@96,800&display=swap',
        {
          headers: { 'User-Agent': 'Mozilla/5.0' },
        },
      )
    ).text();
    const src = /src: url\(([^)]+)\) format\('(?:truetype|opentype)'\)/.exec(css)?.[1];
    return src ? await (await fetch(src)).arrayBuffer() : null;
  } catch {
    return null;
  }
}

const LIME = '#c6ff00';
const INK = '#18210c';

export async function ogImage({ eyebrow, title }: { eyebrow: string; title: string }) {
  const font = await display();
  const size = title.length > 60 ? 64 : title.length > 36 ? 76 : 92;
  return new ImageResponse(
    <div
      style={{
        width: '100%',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'space-between',
        padding: '72px 80px',
        background: 'radial-gradient(120% 70% at 50% -10%, #3f6212 0%, #1a2e05 28%, #0e0f11 62%)',
        color: '#f5f5f4',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
        <div
          style={{
            width: 72,
            height: 72,
            borderRadius: 18,
            background: 'linear-gradient(#c9fb5a, #9fe024)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <svg width="48" height="48" viewBox="-12 -12 24 24">
            <g fill="none" stroke={INK} strokeWidth="2.1" strokeLinecap="round">
              <circle r="10" />
              {[0, 60, 120, 180, 240, 300].map((a) => (
                <path key={a} d="M0 -3.6 L9.79 2.05" transform={`rotate(${a})`} />
              ))}
            </g>
          </svg>
        </div>
        <div
          style={{
            fontSize: 34,
            fontWeight: 800,
            letterSpacing: -1,
            fontFamily: font ? 'Display' : undefined,
          }}
        >
          Shotmaxxing
        </div>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 22 }}>
        <div style={{ fontSize: 26, color: LIME, letterSpacing: 3, textTransform: 'uppercase' }}>
          {eyebrow}
        </div>
        <div
          style={{
            fontSize: size,
            lineHeight: 1.02,
            letterSpacing: -2.5,
            fontWeight: 800,
            fontFamily: font ? 'Display' : undefined,
            maxWidth: 1000,
          }}
        >
          {title}
        </div>
      </div>
      <div style={{ display: 'flex', fontSize: 24, color: '#a8a29e' }}>
        Free and open source · shotmaxxing.app
      </div>
    </div>,
    { ...OG_SIZE, fonts: font ? [{ name: 'Display', data: font, weight: 800, style: 'normal' }] : undefined },
  );
}
