/**
 * Caption rows: how a caption breaks across the frame. The same rules as the export (`crates/core/src/captions.rs`),
 * so a caption breaks in the preview exactly where it will in the video. Change both together.
 */

export type CaptionStyle = 'bar' | 'bold';

/** The most a subtitle file's row holds, the broadcast convention. */
export const FILE_ROW = 42;

/** The text's size in each style, as a fraction of the frame's height. */
export const CAPTION_SIZE: Record<CaptionStyle, number> = { bar: 0.038, bold: 0.05 };

/** How wide a character is: East Asian wide and full-width characters, and emoji, take two units; the rest one. */
export function units(c: string): number {
  const u = c.codePointAt(0) ?? 0;
  const wide =
    (u >= 0x1100 && u <= 0x115f) ||
    (u >= 0x2e80 && u <= 0x303e) ||
    (u >= 0x3041 && u <= 0x33ff) ||
    (u >= 0x3400 && u <= 0x4dbf) ||
    (u >= 0x4e00 && u <= 0x9fff) ||
    (u >= 0xa000 && u <= 0xa4cf) ||
    (u >= 0xac00 && u <= 0xd7a3) ||
    (u >= 0xf900 && u <= 0xfaff) ||
    (u >= 0xfe30 && u <= 0xfe4f) ||
    (u >= 0xff00 && u <= 0xff60) ||
    (u >= 0xffe0 && u <= 0xffe6) ||
    (u >= 0x1f300 && u <= 0x1f64f) ||
    (u >= 0x1f900 && u <= 0x1f9ff) ||
    (u >= 0x20000 && u <= 0x2fffd) ||
    (u >= 0x30000 && u <= 0x3fffd);
  return wide ? 2 : 1;
}

const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });

/** What reads as one character each: a letter with its accents, an emoji with its modifiers. */
function graphemes(text: string): string[] {
  return Array.from(segmenter.segment(text), (part) => part.segment);
}

/** How wide a grapheme is: as wide as its first character, since the marks after it take no room of their own. */
function clusterUnits(grapheme: string): number {
  return grapheme ? units(grapheme) : 0;
}

/** How wide `text` is, in units. */
export function width(text: string): number {
  let total = 0;
  for (const g of graphemes(text)) total += clusterUnits(g);
  return total;
}

/** The most units a row holds on a frame `aspect` wide (width ÷ height): 80% of the frame at about 0.55 em a unit. */
export function rowUnits(aspect: number, style: CaptionStyle): number {
  const fit = (0.8 * aspect) / (0.55 * CAPTION_SIZE[style]);
  return Number.isFinite(fit) && fit > 0 ? Math.min(FILE_ROW, Math.max(14, Math.floor(fit))) : FILE_ROW;
}

type Piece = { text: string; units: number; spaced: boolean };

/** The words of `text`, any word wider than `max` cut into even pieces between graphemes (text without spaces is
 * one long word). */
function pieces(text: string, max: number): Piece[] {
  const out: Piece[] = [];
  for (const word of text.split(/\s+/).filter(Boolean)) {
    const total = width(word);
    if (total <= max) {
      out.push({ text: word, units: total, spaced: out.length > 0 });
      continue;
    }
    const parts = Math.ceil(total / max);
    const target = Math.ceil(total / parts);
    let piece: Piece = { text: '', units: 0, spaced: out.length > 0 };
    for (const g of graphemes(word)) {
      const u = clusterUnits(g);
      if (piece.units > 0 && piece.units + u > target) {
        out.push(piece);
        piece = { text: '', units: 0, spaced: false };
      }
      piece.text += g;
      piece.units += u;
    }
    if (piece.units > 0) out.push(piece);
  }
  return out;
}

function joined(list: Piece[]): [string, number] {
  let text = '';
  let total = 0;
  list.forEach((piece, i) => {
    if (i > 0 && piece.spaced) {
      text += ' ';
      total += 1;
    }
    text += piece.text;
    total += piece.units;
  });
  return [text, total];
}

/** `list` broken into rows of at most `max` units: one row when it fits, two rows as even as they can be when two
 * are enough, and full rows after that. */
function rowsOf(list: Piece[], max: number): string[] {
  if (!list.length) return [];
  const [all, total] = joined(list);
  if (total <= max) return [all];
  if (total <= 2 * max) {
    let best: [number, number] | null = null;
    for (let i = 1; i < list.length; i++) {
      const a = joined(list.slice(0, i))[1];
      const b = joined(list.slice(i))[1];
      if (a <= max && b <= max && (best === null || Math.max(a, b) < best[0])) best = [Math.max(a, b), i];
    }
    if (best) return [joined(list.slice(0, best[1]))[0], joined(list.slice(best[1]))[0]];
  }
  const out: string[] = [];
  let start = 0;
  for (let i = 1; i <= list.length; i++) {
    if (i === list.length || joined(list.slice(start, i + 1))[1] > max) {
      out.push(joined(list.slice(start, i))[0]);
      start = i;
    }
  }
  return out;
}

/**
 * `text` broken into rows of at most `max` units: one row when it fits, two rows as even as they can be when two are
 * enough, and full rows after that. Words stay whole unless one is wider than a row.
 */
export function rows(text: string, max: number): string[] {
  const limit = Math.max(1, max);
  return rowsOf(pieces(text, limit), limit);
}

/**
 * How a caption shows with rows of at most `max` units: pages of one or two rows, one after another. A caption that
 * fits in two rows is one page; a longer one is split between words into as few pages as will do, as even as they
 * can be, so a long caption never covers the picture.
 */
export function pages(text: string, max: number): string[][] {
  const limit = Math.max(1, max);
  const list = pieces(text, limit);
  // best[j]: the fewest pages for the first j pieces, the widest of them, and where the last one starts.
  const best: ([number, number, number] | null)[] = new Array(list.length + 1).fill(null);
  best[0] = [0, 0, 0];
  for (let j = 1; j <= list.length; j++) {
    for (let i = j - 1; i >= 0; i--) {
      const wide = joined(list.slice(i, j))[1];
      if (wide > 2 * limit) break;
      const before = best[i];
      if (!before) continue;
      if (rowsOf(list.slice(i, j), limit).length > 2) continue;
      const candidate: [number, number, number] = [before[0] + 1, Math.max(before[1], wide), i];
      const current = best[j];
      if (
        !current ||
        candidate[0] < current[0] ||
        (candidate[0] === current[0] && candidate[1] < current[1])
      ) {
        best[j] = candidate;
      }
    }
  }
  const out: string[][] = [];
  let j = list.length;
  while (j > 0) {
    const found = best[j];
    if (!found) break;
    out.push(rowsOf(list.slice(found[2], j), limit));
    j = found[2];
  }
  return out.reverse();
}

/** Where each of `pages` stops showing, as a fraction of its caption's time: each shows for its share of the
 * caption's width, as the words take about that long to say. */
export function pageEnds(list: string[][]): number[] {
  const widths = list.map((page) =>
    Math.max(
      1,
      page.reduce((sum, row) => sum + width(row), 0),
    ),
  );
  const total = Math.max(
    1,
    widths.reduce((a, b) => a + b, 0),
  );
  let at = 0;
  return widths.map((w, i) => {
    at += w;
    return i + 1 === widths.length ? 1 : at / total;
  });
}

/** Which of `pages` shows `fraction` (0 to 1) of the way through its caption. */
export function pageAt(list: string[][], fraction: number): number {
  const ends = pageEnds(list);
  const i = ends.findIndex((end) => fraction < end);
  return i === -1 ? Math.max(0, ends.length - 1) : i;
}

/** A caption on a timeline, in seconds: the recording's, or for a voice-over's caption the finished video's. */
export interface TimedCaption {
  start: number;
  end: number;
  text: string;
  /** The voice-over it transcribes, by its file: timed on the finished video, like the voice-over. */
  voiceOver?: string;
}

/**
 * The captions the video shows, in the order the export picks from: those with words and time, and a voice-over's
 * only while that voice-over is in the project; the recording's first, then the voice-overs', each by when it starts.
 */
export function shownCaptions<T extends TimedCaption>(lines: T[], voiceOvers: string[]): T[] {
  const kept = new Set(voiceOvers);
  return lines
    .filter((c) => c.end > c.start && c.text.trim() !== '' && (!c.voiceOver || kept.has(c.voiceOver)))
    .sort((a, b) => Number(!!a.voiceOver) - Number(!!b.voiceOver) || a.start - b.start);
}

/**
 * Which of `lines` (from `shownCaptions`) shows at moment `t` of the recording, and how far through its time it is
 * (0 to 1): the last one that covers it, as the export picks. `out` is where `t` plays in the finished video (null
 * when it was cut), for voice-overs' captions.
 */
export function captionAt<T extends TimedCaption>(
  lines: T[],
  t: number,
  out: number | null,
): { line: T; fraction: number } | null {
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i];
    const at = line.voiceOver ? out : t;
    if (at !== null && at >= line.start && at < line.end) {
      return { line, fraction: Math.min(1, Math.max(0, (at - line.start) / (line.end - line.start))) };
    }
  }
  return null;
}

/** A language's name in the person's own language, from Whisper's code for it ("en" is “English”). */
export function languageName(code: string): string {
  try {
    return new Intl.DisplayNames(undefined, { type: 'language' }).of(code) ?? code;
  } catch {
    return code;
  }
}
