/** Taking stretches out of the kept clips and putting them back, for "Cut silences". */

import type { Clip } from './store.svelte';

type Span = { start: number; end: number };

/** The clips with `spans` (seconds of the recording) taken out. */
export function dropRanges(clips: Clip[], spans: Span[]): Clip[] {
  let out = [...clips];
  for (const s of spans) {
    out = out.flatMap((c) => {
      if (s.end <= c.start || s.start >= c.end) return [c];
      const parts: Clip[] = [];
      if (s.start - c.start > 0.01) parts.push({ ...c, end: s.start });
      if (c.end - s.end > 0.01) parts.push({ ...c, id: `${c.id}~${Math.round(s.end * 1000)}`, start: s.end });
      return parts;
    });
  }
  return out;
}

/**
 * The clips with `spans` put back. A span rejoins the clips on either side of it and plays at the speed of the clip
 * before it; clips it doesn't touch stay exactly as they were.
 */
export function keepRanges(clips: Clip[], spans: Span[]): Clip[] {
  type Piece = Clip & { restored: boolean };
  const pieces: Piece[] = clips.map((c) => ({ ...c, restored: false }));
  for (const s of spans) {
    const before = clips.find(
      (c) => Math.abs(c.end - s.start) < 0.02 || (c.start <= s.start && c.end > s.start),
    );
    pieces.push({
      id: `k${Math.round(s.start * 1000)}`,
      start: s.start,
      end: s.end,
      speed: before?.speed ?? 1,
      restored: true,
    });
  }
  pieces.sort((a, b) => a.start - b.start);
  const out: Piece[] = [];
  for (const piece of pieces) {
    const last = out.at(-1);
    const touches = last && piece.start <= last.end + 0.02;
    if (last && touches && (piece.restored || last.restored) && piece.speed === last.speed) {
      last.end = Math.max(last.end, piece.end);
      last.restored = piece.restored;
    } else if (last && piece.start < last.end) {
      if (piece.end > last.end) out.push({ ...piece, start: last.end });
    } else {
      out.push({ ...piece });
    }
  }
  return out.map(({ restored: _, ...clip }) => clip);
}
