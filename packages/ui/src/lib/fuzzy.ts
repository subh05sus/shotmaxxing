/**
 * Fuzzy matching for the command palette. Letters must appear in order; matches at word starts, consecutive
 * letters and early positions score higher. Forgiving by design: "rdz" finds "Redo auto zoom".
 */
export function fuzzyScore(query: string, text: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const t = text.toLowerCase();
  if (t.includes(q)) return 1000 - t.indexOf(q) * 2 + (t.startsWith(q) ? 200 : 0);
  let score = 0;
  let ti = 0;
  let streak = 0;
  for (const ch of q) {
    if (ch === ' ') continue;
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    const wordStart = found === 0 || /[\s\-_/.]/.test(t[found - 1]);
    streak = found === ti ? streak + 1 : 0;
    score += 10 + (wordStart ? 25 : 0) + streak * 8 - Math.min(found - ti, 10);
    ti = found + 1;
  }
  return score;
}
