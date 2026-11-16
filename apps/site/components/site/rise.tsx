'use client';

/**
 * Brings in everything marked `data-rise`: a short rise and fade, staggered. What's on the first screen waits for the
 * loader to open; the rest arrives in batches as it scrolls into view. globals.css hides them only when scripts
 * and motion are on, so without either they're simply there.
 */
import { gsap, ScrollTrigger, useGSAP } from '@/lib/gsap';
import { onRevealed } from '@/lib/reveal';

export function Rise() {
  useGSAP(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const all = gsap.utils.toArray<HTMLElement>('[data-rise]');
    const firstScreen = all.filter((el) => el.getBoundingClientRect().top < window.innerHeight);
    const later = all.filter((el) => !firstScreen.includes(el));
    const stop = onRevealed(() =>
      gsap.to(firstScreen, {
        autoAlpha: 1,
        y: 0,
        duration: 0.8,
        ease: 'expo.out',
        stagger: 0.08,
        delay: 0.25,
      }),
    );
    ScrollTrigger.batch(later, {
      start: 'top 90%',
      once: true,
      onEnter: (batch) =>
        gsap.to(batch, {
          autoAlpha: 1,
          y: 0,
          duration: 0.8,
          ease: 'expo.out',
          stagger: 0.07,
          overwrite: true,
        }),
    });
    return stop;
  });
  return null;
}
