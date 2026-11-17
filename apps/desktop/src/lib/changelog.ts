/**
 * CHANGELOG.md, read into versions the update dialog and the What's new sheet can show: each version's date, its
 * opening lines, its sections and bullet points, with bold, code and links kept. The installed version's notes come
 * with the app; a newer version's are read from GitHub at its tag.
 */

/** A run of text: plain, bold, code or a link. */
export type Span =
  | { kind: 'text'; text: string }
  | { kind: 'bold'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'link'; text: string; href: string };

export type Block =
  | { kind: 'paragraph'; spans: Span[] }
  | { kind: 'heading'; text: string }
  | { kind: 'list'; items: Span[][] };

export interface Version {
  version: string;
  /** As written, e.g. 2026-11-17. */
  date: string | null;
  blocks: Block[];
}

/** Bold, code and links in one line of Markdown; anything else stays text. */
export function spans(line: string): Span[] {
  const out: Span[] = [];
  const pattern = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)/g;
  let at = 0;
  for (const match of line.matchAll(pattern)) {
    if (match.index > at) out.push({ kind: 'text', text: line.slice(at, match.index) });
    if (match[1] !== undefined) out.push({ kind: 'bold', text: match[1] });
    else if (match[2] !== undefined) out.push({ kind: 'code', text: match[2] });
    else out.push({ kind: 'link', text: match[3], href: match[4] });
    at = match.index + match[0].length;
  }
  if (at < line.length) out.push({ kind: 'text', text: line.slice(at) });
  return out;
}

/** Every released version in a changelog, newest first as written. Unreleased notes are left out. */
export function parseChangelog(markdown: string): Version[] {
  const versions: Version[] = [];
  for (const section of markdown.split(/^## /m).slice(1)) {
    const [head, ...lines] = section.split('\n');
    const match = /^v?(\d+\.\d+\.\d+)\s*(?:\(([^)]+)\))?/.exec(head.trim());
    if (!match) continue;
    const blocks: Block[] = [];
    let paragraph: string[] = [];
    let items: string[] = [];
    const flush = () => {
      if (paragraph.length) blocks.push({ kind: 'paragraph', spans: spans(paragraph.join(' ')) });
      if (items.length) blocks.push({ kind: 'list', items: items.map(spans) });
      paragraph = [];
      items = [];
    };
    for (const raw of lines) {
      const line = raw.trimEnd();
      if (!line.trim()) {
        if (paragraph.length) flush();
      } else if (line.startsWith('### ')) {
        flush();
        blocks.push({ kind: 'heading', text: line.slice(4).trim() });
      } else if (/^[-*] /.test(line)) {
        if (paragraph.length) flush();
        items.push(line.slice(2).trim());
      } else if (items.length && /^\s/.test(raw)) {
        items[items.length - 1] += ` ${line.trim()}`;
      } else {
        if (items.length) flush();
        paragraph.push(line.trim());
      }
    }
    flush();
    versions.push({ version: match[1], date: match[2]?.trim() ?? null, blocks });
  }
  return versions;
}

/** Whether version `a` is newer than `b` (`v1.2.3` and `1.2.3` both work). */
export function isNewer(a: string, b: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/, '')
      .split(/[.-]/)
      .slice(0, 3)
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
}

/** The versions after `from` up to and including `to`, newest first. */
export function between(versions: Version[], from: string, to: string): Version[] {
  return versions
    .filter((v) => isNewer(v.version, from) && !isNewer(v.version, to))
    .sort((a, b) => (isNewer(a.version, b.version) ? -1 : 1));
}

/** A date as written in the changelog (2026-11-17), as people say it (17 Nov 2026). */
export function readableDate(date: string | null): string | null {
  if (!date || !/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
  return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  });
}
