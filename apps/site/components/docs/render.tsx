/**
 * Draws a guide's body from the tree lib/docs made: GitHub alerts become callouts, code blocks get a frame with their
 * title and a copy button, titled code blocks in a row become tabs, pictures become figures with captions, and every
 * heading gets a link to itself.
 */
import 'server-only';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import Image from 'next/image';
import Link from 'next/link';
import type { Element, Root } from 'hast';
import { toJsxRuntime, type Components } from 'hast-util-to-jsx-runtime';
import { Fragment, jsx, jsxs } from 'react/jsx-runtime';
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  BulbOnIcon,
  InformationCircleIcon,
  LinkSlantIcon,
  StarIcon,
} from '@pikaicons/core-free';
import { CopyButton } from '@/components/site/copy-button';
import { Icon, type IconData } from '@/components/site/icon';
import { ROOT } from '@/lib/release';
import { CodeTabs } from './code-tabs';

const CALLOUTS: Record<string, { label: string; icon: IconData; tone: string; dot: string }> = {
  note: { label: 'Note', icon: InformationCircleIcon, tone: 'bg-info-tint', dot: 'text-info' },
  tip: { label: 'Tip', icon: BulbOnIcon, tone: 'bg-lime-tint', dot: 'text-lime' },
  important: { label: 'Important', icon: StarIcon, tone: 'bg-lime-tint', dot: 'text-lime' },
  warning: { label: 'Warning', icon: AlertTriangleIcon, tone: 'bg-warning-tint', dot: 'text-warning' },
  caution: { label: 'Caution', icon: AlertCircleIcon, tone: 'bg-danger-tint', dot: 'text-danger' },
};

/** A PNG's size, read from its header, so a figure takes its space before it loads. */
const sizes = new Map<string, { width: number; height: number } | null>();
function sizeOf(src: string) {
  if (!sizes.has(src)) {
    let size = null;
    const file = src.startsWith('/media/images/')
      ? join(ROOT, 'docs', 'images', src.slice('/media/images/'.length))
      : src.startsWith('/media/brand/')
        ? join(ROOT, 'assets', 'brand', src.slice('/media/brand/'.length))
        : null;
    try {
      const b = file && src.endsWith('.png') ? readFileSync(file) : null;
      if (b) size = { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
    } catch {
      /* drawn without a size */
    }
    sizes.set(src, size);
  }
  return sizes.get(src) ?? null;
}

type WithNode = { node?: Element; children?: React.ReactNode } & Record<string, unknown>;

function CodeFrame({
  title,
  lang,
  source,
  children,
}: {
  title?: string;
  lang?: string;
  source: string;
  children: React.ReactNode;
}) {
  return (
    <div className="overflow-hidden rounded-2xl bg-code ring-1 ring-line ring-inset">
      <div className="flex h-11 items-center justify-between gap-3 border-b border-line pr-1.5 pl-4">
        <span className="truncate font-mono text-[12px] text-paper-subtle">{title || lang || 'text'}</span>
        <CopyButton text={source} />
      </div>
      <pre className="scroll-room overflow-x-auto px-4 py-3.5 font-mono text-[13.5px] leading-6 text-paper">
        <code>{children}</code>
      </pre>
    </div>
  );
}

function Heading({ level, id, children }: { level: 2 | 3; id?: string; children: React.ReactNode }) {
  const Tag = level === 2 ? 'h2' : 'h3';
  return (
    <Tag id={id} className="group relative">
      {children}
      {id && (
        <a
          href={`#${id}`}
          data-plain
          aria-label="Link to this section"
          className="ml-2 inline-grid size-7 translate-y-[-0.1em] place-items-center rounded-lg align-middle text-paper-subtle opacity-0 transition-opacity group-hover:opacity-100 hover:bg-fill hover:text-paper focus-visible:opacity-100"
        >
          <Icon icon={LinkSlantIcon} size={15} />
        </a>
      )}
    </Tag>
  );
}

const components = {
  callout: ({ node, children }: WithNode) => {
    const c = CALLOUTS[String(node?.properties.kind)] ?? CALLOUTS.note;
    return (
      <aside
        className={`flex gap-3.5 rounded-2xl ${c.tone} px-5 py-4 text-[15.5px] leading-relaxed text-paper`}
      >
        <Icon icon={c.icon} size={19} className={`mt-[0.2em] shrink-0 ${c.dot}`} />
        <div className="min-w-0 [&>*+*]:mt-2">
          <p className="sr-only">{c.label}:</p>
          {children}
        </div>
      </aside>
    );
  },
  codeblock: ({ node, children }: WithNode) => {
    const p = node?.properties ?? {};
    return (
      <CodeFrame title={String(p.title ?? '')} lang={String(p.lang ?? '')} source={String(p.source ?? '')}>
        {children}
      </CodeFrame>
    );
  },
  codetabs: ({ node, children }: WithNode) => {
    const titles = (node?.children ?? [])
      .filter((c): c is Element => c.type === 'element')
      .map((c) => String(c.properties.title));
    return <CodeTabs titles={titles}>{children}</CodeTabs>;
  },
  figure: ({ node }: WithNode) => {
    const imgs = (node?.children ?? []).filter(
      (c): c is Element => c.type === 'element' && c.tagName === 'img',
    );
    const gallery = !!node?.properties.gallery;
    return (
      <figure className={gallery ? 'grid gap-4 sm:grid-cols-2' : ''}>
        {imgs.map((img, i) => {
          const src = String(img.properties.src);
          const alt = String(img.properties.alt ?? '');
          const caption = img.properties.title ? String(img.properties.title) : '';
          const size = sizeOf(src);
          return (
            <div key={i}>
              <div className="overflow-hidden rounded-2xl bg-stage shadow-lift ring-1 ring-line-strong">
                {size ? (
                  <Image
                    src={src}
                    alt={alt}
                    width={size.width}
                    height={size.height}
                    sizes="(min-width: 1024px) 720px, 100vw"
                    className="h-auto w-full"
                  />
                ) : (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img src={src} alt={alt} loading="lazy" className="h-auto w-full" />
                )}
              </div>
              {caption && (
                <figcaption className="mt-2.5 text-center text-[13px] text-paper-subtle">
                  {caption}
                </figcaption>
              )}
            </div>
          );
        })}
      </figure>
    );
  },
  h2: ({ id, children }: WithNode) => (
    <Heading level={2} id={id as string}>
      {children}
    </Heading>
  ),
  h3: ({ id, children }: WithNode) => (
    <Heading level={3} id={id as string}>
      {children}
    </Heading>
  ),
  a: ({ href, children, node: _node, ...rest }: WithNode) => {
    const h = String(href ?? '');
    if (h.startsWith('/')) {
      return (
        <Link href={h} {...rest}>
          {children}
        </Link>
      );
    }
    return (
      <a href={h} {...(h.startsWith('http') ? { rel: 'noreferrer' } : {})} {...rest}>
        {children}
      </a>
    );
  },
  table: ({ children, node: _node, ...rest }: WithNode) => (
    <div className="scroll-room overflow-x-auto rounded-2xl bg-graphite-2 ring-1 ring-line ring-inset">
      <table {...rest}>{children}</table>
    </div>
  ),
} as unknown as Components;

export function DocBody({ tree }: { tree: Root }) {
  return (
    <div className="doc">
      {toJsxRuntime(tree, { Fragment, jsx, jsxs, components, passNode: true, ignoreInvalidStyle: true })}
    </div>
  );
}
