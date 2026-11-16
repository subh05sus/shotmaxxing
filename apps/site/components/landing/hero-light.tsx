'use client';

/**
 * The light behind the hero: React Bits' GradientASCII, a horizon glowing in dots. Its colours are the --hero-light-*
 * tokens in globals.css, read once it mounts (WebGL needs real colours, not CSS variables).
 */
import { useEffect, useState } from 'react';
import GradientASCII from '@/components/site/gradient-ascii';

export function HeroLight() {
  const [colors, setColors] = useState<string[] | null>(null);
  useEffect(() => {
    const css = getComputedStyle(document.documentElement);
    setColors([1, 2, 3, 4].map((i) => css.getPropertyValue(`--hero-light-${i}`).trim()));
  }, []);
  if (!colors) return null;
  return (
    <GradientASCII
      colors={colors}
      shape="horizon"
      characters=" .·•●"
      cellSize={18}
      density={1}
      softness={0.51}
      flow={1.3}
      speed={0.7}
      gradientOpacity={0.86}
      variation={14}
      dpr={1.5}
    />
  );
}
