'use client';

/**
 * Which computer the visitor is on, so the site offers the right install line by itself, and a way for any button to
 * pick one: the Install section listens and switches its tab.
 */
import { useEffect, useState } from 'react';

export type Os = 'mac' | 'windows';

export function detectOs(): Os | null {
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const p = `${nav.userAgentData?.platform ?? ''} ${navigator.platform ?? ''} ${navigator.userAgent}`;
  if (/Win/i.test(p)) return 'windows';
  if (/Mac/i.test(p) && !/iPhone|iPad/i.test(p)) return 'mac';
  return null;
}

const EVENT = 'smx:os';

/** Switches the Install section to `os` and brings it into view. */
export function chooseOs(os: Os) {
  window.dispatchEvent(new CustomEvent<Os>(EVENT, { detail: os }));
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  document
    .getElementById('install')
    ?.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' });
  history.replaceState(null, '', '#install');
}

/** The visitor's computer (null until it's known, or when it's neither), and whatever a button picks after that. */
export function useOs() {
  const [os, setOs] = useState<Os | null>(null);
  useEffect(() => {
    setOs(detectOs());
    const on = (e: Event) => setOs((e as CustomEvent<Os>).detail);
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  return os;
}
