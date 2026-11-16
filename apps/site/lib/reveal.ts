/**
 * The loader opening onto the page. Entrances wait for it, so nothing animates behind the closed iris.
 */
const EVENT = 'shotmaxxing:revealed';

declare global {
  interface Window {
    __shotmaxxingRevealed?: boolean;
  }
}

export function markRevealed() {
  if (window.__shotmaxxingRevealed) return;
  window.__shotmaxxingRevealed = true;
  window.dispatchEvent(new Event(EVENT));
}

/** Runs `run` once the page is revealed (straight away if it is already). Returns a cancel. */
export function onRevealed(run: () => void): () => void {
  if (window.__shotmaxxingRevealed) {
    run();
    return () => {};
  }
  const handler = () => run();
  window.addEventListener(EVENT, handler, { once: true });
  return () => window.removeEventListener(EVENT, handler);
}
