'use client';

import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { useGSAP } from '@gsap/react';

gsap.registerPlugin(ScrollTrigger, useGSAP);

/** Reduce Motion: scroll scenes show their finished state and nothing travels. */
export function reducedMotion() {
  return typeof window !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export { gsap, ScrollTrigger, useGSAP };
