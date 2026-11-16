import 'server-only';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, normalize, relative } from 'node:path';
import { cache } from 'react';
import type { Element, ElementContent, Root, RootContent, Text } from 'hast';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkRehype from 'remark-rehype';
import rehypeRaw from 'rehype-raw';
import rehypeSlug from 'rehype-slug';
import { createCssVariablesTheme, createHighlighter, type Highlighter } from 'shiki';
import { GITHUB, ROOT } from './release';

/** The guides, in the sidebar's order. Each is a Markdown file in the repository, so GitHub shows the same text. */
export const SECTIONS = [
  {
    title: 'Get started',
    pages: [
      { slug: 'install', file: 'docs/guides/install.md' },
      { slug: 'setup', file: 'docs/guides/setup.md' },
      { slug: 'permissions', file: 'docs/guides/permissions.md' },
    ],
  },
  {
    title: 'Record',
    pages: [
      { slug: 'recording', file: 'docs/guides/recording.md' },
      { slug: 'screenshots', file: 'docs/guides/screenshots.md' },
    ],
  },
  {
    title: 'Edit',
    pages: [
      { slug: 'editor', file: 'docs/guides/editor.md' },
      { slug: 'auto-zoom', file: 'docs/guides/auto-zoom.md' },
      { slug: 'cursor', file: 'docs/guides/cursor.md' },
      { slug: 'background', file: 'docs/guides/background.md' },
      { slug: 'annotate', file: 'docs/guides/annotate.md' },
      { slug: 'camera', file: 'docs/guides/camera.md' },
      { slug: 'sound', file: 'docs/guides/sound.md' },
      { slug: 'captions', file: 'docs/guides/captions.md' },
      { slug: 'export', file: 'docs/guides/export.md' },
    ],
  },
  {
    title: 'Agents',
    pages: [
      { slug: 'agents', file: 'docs/agents.md' },
      { slug: 'command-line', file: 'docs/guides/command-line.md' },
    ],
  },
  {
    title: 'Reference',
    pages: [
      { slug: 'shortcuts', file: 'docs/guides/shortcuts.md' },
      { slug: 'content-packs', file: 'docs/content-packs.md' },
      { slug: 'privacy', file: 'docs/guides/privacy.md' },
      { slug: 'troubleshooting', file: 'docs/guides/troubleshooting.md' },
    ],
  },
] as const;

export type Slug = (typeof SECTIONS)[number]['pages'][number]['slug'];

export interface Heading {
  id: string;
  text: string;
  depth: 2 | 3;
}

export interface Doc {
  slug: string;
  file: string;
  section: string;
  title: string;
  /** The first paragraph, shown under the title. */
  lede: string;
  headings: Heading[];
  /** The body after the title and lede, ready for components/docs/render. */
  tree: Root;
  /** Plain text, for search. */
  text: string;
  /** Minutes to read, at 230 words a minute. */
  minutes: number;
}

const PAGES = SECTIONS.flatMap((s) => s.pages.map((p) => ({ ...p, section: s.title })));
const BY_FILE = new Map<string, string>(PAGES.map((p) => [p.file, p.slug]));

export function allSlugs(): string[] {
  return PAGES.map((p) => p.slug);
}

export function neighbours(slug: string) {
  const i = PAGES.findIndex((p) => p.slug === slug);
  return { prev: i > 0 ? PAGES[i - 1] : null, next: i >= 0 && i < PAGES.length - 1 ? PAGES[i + 1] : null };
}

// Highlighting ------------------------------------------------------------------------------------------------------

/** Shiki writes `var(--shiki-…)` instead of colours, so code takes the site's palette from globals.css. */
const theme = createCssVariablesTheme({ name: 'shotmaxxing', variablePrefix: '--shiki-', fontStyle: true });
const LANGS = [
  'sh',
  'bash',
  'powershell',
  'json',
  'jsonc',
  'yaml',
  'toml',
  'ts',
  'js',
  'rust',
  'text',
] as const;
const ALIASES: Record<string, string> = {
  shell: 'sh',
  zsh: 'sh',
  console: 'sh',
  ps1: 'powershell',
  yml: 'yaml',
};

let highlighter: Promise<Highlighter> | null = null;
function getHighlighter() {
  highlighter ??= createHighlighter({ themes: [theme], langs: [...LANGS] });
  return highlighter;
}

// Tree helpers ------------------------------------------------------------------------------------------------------

function textOf(node: RootContent | ElementContent | Root): string {
  if (node.type === 'text') return node.value;
  if ('children' in node) return (node.children as (ElementContent | RootContent)[]).map(textOf).join('');
  return '';
}

const isElement = (node: unknown, tag?: string): node is Element =>
  !!node && (node as Element).type === 'element' && (!tag || (node as Element).tagName === tag);

const isBlank = (node: RootContent | ElementContent) => node.type === 'text' && !node.value.trim();

function el(tagName: string, properties: Element['properties'], children: ElementContent[]): Element {
  return { type: 'element', tagName, properties, children };
}

/** Walks every element, parents first, letting `visit` replace a node's children. */
function walk(node: Root | Element, visit: (el: Element, parent: Root | Element, index: number) => void) {
  node.children.forEach((child, i) => {
    if (child.type === 'element') {
      visit(child, node, i);
      walk(child, visit);
    }
  });
}

// Links and images --------------------------------------------------------------------------------------------------

/**
 * A link in a guide is written for GitHub, relative to its file. Guides become site pages, pictures in docs/images
 * are served from /media, and anything else in the repository opens on GitHub.
 */
function resolveLink(href: string, file: string, kind: 'link' | 'image'): string {
  if (/^([a-z]+:|#|\/\/)/i.test(href)) return href;
  const [path, hash = ''] = href.split('#');
  const target = normalize(join(dirname(file), path))
    .split('\\')
    .join('/');
  if (kind === 'image' || target.startsWith('docs/images/') || target.startsWith('assets/brand/')) {
    if (target.startsWith('docs/images/')) return `/media/images/${target.slice('docs/images/'.length)}`;
    if (target.startsWith('assets/brand/')) return `/media/brand/${target.slice('assets/brand/'.length)}`;
  }
  const slug = BY_FILE.get(target);
  if (slug) return `/docs/${slug}${hash ? `#${hash}` : ''}`;
  if (target === 'docs/guides/README.md') return `/docs${hash ? `#${hash}` : ''}`;
  const isFolder = !/\.[a-z0-9]+$/i.test(target);
  return `${GITHUB}/${isFolder ? 'tree' : 'blob'}/main/${target}${hash ? `#${hash}` : ''}`;
}

// Rendering ---------------------------------------------------------------------------------------------------------

const ALERTS = ['NOTE', 'TIP', 'IMPORTANT', 'WARNING', 'CAUTION'] as const;

async function transform(tree: Root, file: string) {
  const shiki = await getHighlighter();

  walk(tree, (node, parent, index) => {
    // Links and pictures.
    if (node.tagName === 'a' && typeof node.properties.href === 'string') {
      node.properties.href = resolveLink(node.properties.href, file, 'link');
    }
    if (node.tagName === 'img' && typeof node.properties.src === 'string') {
      node.properties.src = resolveLink(node.properties.src, file, 'image');
    }

    // GitHub alerts (> [!TIP]) become callouts.
    if (node.tagName === 'blockquote') {
      const first = node.children.find((c) => isElement(c, 'p')) as Element | undefined;
      const lead = first?.children[0];
      const match = lead?.type === 'text' ? /^\[!(\w+)\]\s*/.exec(lead.value) : null;
      const kind = match?.[1].toUpperCase();
      if (first && lead && match && kind && (ALERTS as readonly string[]).includes(kind)) {
        (lead as Text).value = (lead as Text).value.slice(match[0].length);
        if (!(lead as Text).value && isElement(first.children[1], 'br')) first.children.splice(0, 2);
        parent.children[index] = el('callout', { kind: kind.toLowerCase() }, node.children);
      }
    }

    // Code: highlighted, with its language and title kept for the frame.
    if (node.tagName === 'pre' && isElement(node.children[0], 'code')) {
      const code = node.children[0];
      const classes = (code.properties.className as string[] | undefined) ?? [];
      const raw = classes.find((c) => c.startsWith('language-'))?.slice('language-'.length) ?? 'text';
      const lang = ALIASES[raw] ?? raw;
      const meta = String(code.properties.dataMeta ?? '');
      const title = /title="([^"]+)"/.exec(meta)?.[1] ?? '';
      const source = textOf(code).replace(/\n$/, '');
      const known = (LANGS as readonly string[]).includes(lang);
      const hast = shiki.codeToHast(source, { lang: known ? lang : 'text', theme: 'shotmaxxing' });
      const pre = hast.children[0] as Element;
      const lines = (pre.children[0] as Element).children;
      parent.children[index] = el('codeblock', { lang: raw, title, source }, lines as ElementContent[]);
    }
  });

  // Code blocks in a row that all have titles become tabs (macOS | Windows, Homebrew | Script…).
  walk(tree, (node) => groupTabs(node));
  groupTabs(tree);

  // A picture alone in its paragraph is a figure; its title (![alt](src "title")) is the caption.
  walk(tree, (node, parent, index) => {
    if (node.tagName !== 'p') return;
    const kids = node.children.filter((c) => !isBlank(c));
    if (kids.length === 1 && isElement(kids[0], 'img')) parent.children[index] = el('figure', {}, [kids[0]]);
    if (kids.length > 1 && kids.every((k) => isElement(k, 'img'))) {
      parent.children[index] = el('figure', { gallery: true }, kids as ElementContent[]);
    }
  });
}

function groupTabs(parent: Root | Element) {
  const out: (RootContent | ElementContent)[] = [];
  let run: Element[] = [];
  const flush = () => {
    if (run.length > 1) out.push(el('codetabs', {}, run));
    else out.push(...run);
    run = [];
  };
  for (const child of parent.children as (RootContent | ElementContent)[]) {
    if (isElement(child, 'codeblock') && child.properties.title) run.push(child);
    else if (run.length && isBlank(child)) continue;
    else {
      flush();
      out.push(child);
    }
  }
  flush();
  parent.children = out as typeof parent.children;
}

/** The title (first h1) and lede (first paragraph) come out of the body: the page header shows them. */
function splitHeader(tree: Root) {
  let title = '';
  let lede = '';
  const body: RootContent[] = [];
  for (const node of tree.children) {
    if (!title && isElement(node, 'h1')) title = textOf(node).trim();
    else if (title && !lede && isElement(node, 'p')) lede = textOf(node).trim();
    else if (!isBlank(node) || body.length) body.push(node);
  }
  tree.children = body;
  return { title, lede };
}

function headingsOf(tree: Root): Heading[] {
  const out: Heading[] = [];
  walk(tree, (node) => {
    if ((node.tagName === 'h2' || node.tagName === 'h3') && typeof node.properties.id === 'string') {
      out.push({ id: node.properties.id, text: textOf(node).trim(), depth: node.tagName === 'h2' ? 2 : 3 });
    }
  });
  return out;
}

export const getDoc = cache(async (slug: string): Promise<Doc | null> => {
  const page = PAGES.find((p) => p.slug === slug);
  if (!page || !existsSync(join(ROOT, page.file))) return null;
  const markdown = readFileSync(join(ROOT, page.file), 'utf8');
  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(() => (tree) => {
      // Keep a code block's info string (```sh title="macOS") for the frame and the tabs.
      const visit = (node: { type: string; meta?: string | null; data?: object; children?: unknown[] }) => {
        if (node.type === 'code' && node.meta) {
          node.data = { ...(node.data ?? {}), hProperties: { dataMeta: node.meta } };
        }
        (node.children as (typeof node)[] | undefined)?.forEach(visit);
      };
      visit(tree as never);
    })
    .use(remarkRehype, { allowDangerousHtml: true })
    .use(rehypeRaw)
    .use(rehypeSlug);
  const tree = (await processor.run(processor.parse(markdown))) as Root;
  const { title, lede } = splitHeader(tree);
  await transform(tree, page.file);
  const text = textOf(tree);
  return {
    slug: page.slug,
    file: page.file,
    section: page.section,
    title,
    lede,
    headings: headingsOf(tree),
    tree,
    text,
    minutes: Math.max(1, Math.round(text.split(/\s+/).length / 230)),
  };
});

/** The guides by section, with their titles and ledes, for the sidebar and the guides index. */
export const getNav = cache(async () =>
  Promise.all(
    SECTIONS.map(async (s) => ({
      title: s.title,
      pages: await Promise.all(
        s.pages.map(async (p) => {
          const doc = await getDoc(p.slug);
          return {
            slug: p.slug,
            title: doc?.title ?? p.slug,
            lede: doc?.lede ?? '',
            minutes: doc?.minutes ?? 1,
          };
        }),
      ),
    })),
  ),
);

export function editUrl(file: string) {
  return `${GITHUB}/edit/main/${relative('.', file)}`;
}
