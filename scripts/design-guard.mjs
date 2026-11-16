#!/usr/bin/env node
/**
 * Design guard: fails lint when UI code breaks a house rule from docs/design/principles.md.
 *
 * A line can opt out of one rule with a marker comment naming it, e.g. `design-guard-allow spinner`, and a file can
 * opt out with `design-guard-allow-file <rule>` near its top. Markers say why; they are reviewed like code.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const ROOT = new URL('..', import.meta.url).pathname;
const SCAN = ['apps', 'packages'];
const EXTENSIONS = /\.(svelte|ts|tsx|css|html)$/;
const SKIP_DIRS = new Set([
  'node_modules',
  'dist',
  'target',
  'gen',
  'build',
  '.svelte-kit',
  '.next',
  'public',
  'src-tauri',
  'icons',
]);

// Colour literals are data in these files (the palette itself, wallpaper and scene definitions).
const COLOUR_FILES = [
  'packages/ui/src/styles/tokens.css',
  'apps/desktop/src/lib/wallpapers.ts',
  'apps/desktop/src/lib/backdrops.ts',
  'apps/desktop/src/lib/scenes.css',
  'apps/site/app/globals.css',
  // React Bits' GradientASCII, used as is: its default colours are part of the component.
  'apps/site/components/site/gradient-ascii.tsx',
  // Share images are drawn by next/og, which can't read CSS variables.
  'apps/site/lib/og.tsx',
];

const RULES = [
  {
    id: 'side-stripe',
    why: 'No coloured or thick edge down one side of a card, row, chip or callout. Use a dot, an icon with a word, or a tinted fill.',
    test: (line) =>
      /\bborder-[ls]-(?!0\b)[\w[]/.test(line) ||
      /\bborder-[re]-(?!0\b)(\d|\[)/.test(line) ||
      /(^|[\s"'`{])border-[ls](?=[\s"'`}]|$)/.test(line) ||
      /border-(left|inline-start)(-width|-color)?\s*:/.test(line) ||
      /inset[\s_]+-?\d+px[\s_]+0[\s_]+0/.test(line) ||
      /shadow-\[inset_-?\d+px_0/.test(line),
    allow: 'divider',
  },
  {
    id: 'colour',
    why: 'Colours come from tokens (packages/ui/src/styles/tokens.css), never literals.',
    test: (line) =>
      /\b(rgba?|hsla?|oklch|oklab|lab|lch|color)\(/.test(line) ||
      /(^|[\s:(,'"[])#[0-9a-fA-F]{3,8}(?![\w-])/.test(
        line.replace(/getElementById\([^)]*\)|querySelector\([^)]*\)/g, ''),
      ),
    allow: 'colour',
    files: (file) => !COLOUR_FILES.includes(file) && !file.endsWith('.svg'),
  },
  {
    id: 'backdrop',
    why: 'No CSS backdrop blur: translucency comes from the native material (vibrancy, Mica), which costs nothing.',
    test: (line) => /backdrop-(filter|blur)/.test(line),
  },
  {
    id: 'raw-palette',
    why: 'Use semantic colours (bg-surface, text-fg-muted…), not raw palette classes.',
    test: (line) =>
      /\b(bg|text|border|ring|fill|stroke|from|to|via|divide|outline|decoration|shadow)-(gray|slate|zinc|neutral|stone)-\d/.test(
        line,
      ) || /\b(bg|text|border)-(white|black)\b/.test(line),
  },
  {
    id: 'layout-transition',
    why: 'Animate transform and opacity only. Width, height, margins and positions cause layout on every frame.',
    test: (line) =>
      /\btransition-all\b/.test(line) ||
      /transition-\[[^\]]*\b(width|height|top|left|right|bottom|margin|padding|inset)\b/.test(line) ||
      /transition(-property)?\s*:\s*[^;]*\b(width|height|top|left|right|bottom|margin|padding|all)\b/.test(
        line,
      ),
    allow: 'layout',
  },
  {
    id: 'linear',
    why: 'Nothing in the physical world moves linearly. Use the ease tokens; linear is only for progress and spinners.',
    test: (line) =>
      /\bease-linear\b/.test(line) || /(transition|animation)[^;]*\blinear\b(?![-(])/.test(line),
    allow: 'spinner',
  },
  {
    id: 'infinite',
    why: 'No idle loops: an animation that never ends keeps the GPU awake. Only a busy spinner may loop.',
    test: (line) => /\binfinite\b/.test(line) || /\banimate-(spin|pulse|ping|bounce)\b/.test(line),
    allow: 'spinner',
  },
  {
    id: 'long',
    why: 'Interface motion stays at or under 300 ms. Mark a rare, earned moment with `design-guard-allow long`.',
    test: (line) => {
      for (const m of line.matchAll(/\bduration-(\d+)\b/g)) if (Number(m[1]) > 300) return true;
      if (/(transition|animation)/.test(line)) {
        for (const m of line.matchAll(/(\d*\.?\d+)(ms|s)\b/g)) {
          const ms = m[2] === 's' ? Number(m[1]) * 1000 : Number(m[1]);
          if (ms > 300) return true;
        }
      }
      return false;
    },
    allow: ['long', 'spinner'],
  },
  {
    id: 'scroll-room',
    why: 'A scroll box clips rings and shadows at its edge. Give it inner room (`scroll-room` or padding).',
    test: (line) =>
      /class=/.test(line) &&
      /\boverflow-(y-|x-)?(auto|scroll)\b/.test(line) &&
      !/\bscroll-room\b/.test(line) &&
      !/\bp[xytblr]?-(\d|\[)/.test(line),
    allow: 'scroll',
  },
];

function walk(dir, out) {
  for (const name of readdirSync(dir)) {
    if (SKIP_DIRS.has(name) || name.startsWith('.')) continue;
    const path = join(dir, name);
    const stat = statSync(path);
    if (stat.isDirectory()) walk(path, out);
    else if (EXTENSIONS.test(name)) out.push(path);
  }
  return out;
}

const files = SCAN.flatMap((d) => walk(join(ROOT, d), []));
const problems = [];

for (const path of files) {
  const file = relative(ROOT, path).split(sep).join('/');
  const text = readFileSync(path, 'utf8');
  const head = text.slice(0, 600);
  const lines = text.split('\n');
  for (const rule of RULES) {
    if (rule.files && !rule.files(file)) continue;
    const allows = [rule.allow ?? rule.id].flat();
    if (allows.some((a) => head.includes(`design-guard-allow-file ${a}`))) continue;
    lines.forEach((line, i) => {
      if (allows.some((a) => line.includes(`design-guard-allow ${a}`))) return;
      if (/^\s*(\/\/|\*|\/\*)/.test(line)) return; // comments describe rules; they don't break them
      if (rule.test(line)) problems.push({ file, line: i + 1, rule, text: line.trim() });
    });
  }
}

if (problems.length) {
  const byRule = new Map();
  for (const p of problems) byRule.set(p.rule.id, [...(byRule.get(p.rule.id) ?? []), p]);
  for (const [id, list] of byRule) {
    console.error(`\n✗ ${id}: ${list[0].rule.why}`);
    for (const p of list) console.error(`  ${p.file}:${p.line}  ${p.text.slice(0, 120)}`);
  }
  console.error(
    `\nDesign guard: ${problems.length} problem${problems.length === 1 ? '' : 's'} in ${files.length} files.`,
  );
  process.exit(1);
}
console.log(`Design guard: ${files.length} files follow the house rules.`);
