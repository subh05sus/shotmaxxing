/**
 * The logo's iris over the whole window: the full build on the first launch (and Run setup again), a short build
 * each time the app starts, and the shutter that closes setup and opens on the Library. One overlay plays them all
 * (components/IrisOverlay.svelte); pages wait for `revealed` before their own entrance.
 */

export type IrisKind = 'intro' | 'launch' | 'finale';

export const iris = $state({
  kind: null as IrisKind | null,
  /** Bumped by each play, so the overlay starts over even for the same kind. */
  run: 0,
  /** False while the overlay hides the page; pages start their entrance when it turns true. */
  revealed: true,
});

let hooks: { closed?: () => void; done?: () => void } = {};

/** Plays `kind`. A finale calls `closed` once the shutter has shut, to change what's behind it. */
export function playIris(kind: IrisKind, options: { closed?: () => void; done?: () => void } = {}): void {
  hooks = options;
  iris.kind = kind;
  iris.revealed = false;
  iris.run += 1;
}

/** Called by the overlay as it plays. */
export function irisEvent(event: 'reveal' | 'closed' | 'done'): void {
  if (event === 'reveal') iris.revealed = true;
  else if (event === 'closed') {
    hooks.closed?.();
    hooks.closed = undefined;
  } else {
    iris.revealed = true;
    iris.kind = null;
    const done = hooks.done;
    hooks = {};
    done?.();
  }
}
