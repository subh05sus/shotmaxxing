#!/usr/bin/env node
/**
 * Renders the screenshots in the README and docs from the app's browser preview, which shows the sample project and
 * a sample library, so nobody's own recordings end up in them. A headless Chrome of its own (a throwaway profile,
 * never your browser's) loads each page at the size of the window it lives in, at 2×, with motion reduced so nothing
 * is caught halfway through an animation.
 *
 *   pnpm dev                      # the preview, on http://localhost:1420
 *   node scripts/screenshots.mjs  # writes docs/images/*.png
 *
 * CHROME=/path/to/chrome picks another Chrome or Chromium, PREVIEW=http://… another preview address.
 */
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const PREVIEW = process.env.PREVIEW ?? 'http://localhost:1420';
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const OUT = new URL('../docs/images/', import.meta.url).pathname;
const PORT = 9300 + Math.floor(Math.random() * 600);

/**
 * Each picture: its file name, the page, the size of the window that page lives in, its theme, and the part of the
 * window to keep when it isn't all of it (in points).
 */
const SHOTS = [
  { name: 'editor', route: 'editor/sample', size: [1360, 860] },
  { name: 'editor-light', route: 'editor/sample', size: [1360, 860], theme: 'light' },
  { name: 'editor-zoom', route: 'editor/sample?tool=zoom', size: [1360, 860] },
  { name: 'library', route: 'home/library', size: [1120, 740] },
  { name: 'agents', route: 'home/agents', size: [1120, 740] },
  { name: 'record', route: 'record', size: [840, 600] },
  // The take card sits at the bottom of its window, which the preview fills with a sample desktop.
  { name: 'take-card', route: 'take', size: [420, 540], clip: { x: 0, y: 132, width: 420, height: 408 } },
];

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function reachable(url) {
  try {
    return (await fetch(url)).ok;
  } catch {
    return false;
  }
}

if (!(await reachable(PREVIEW))) {
  console.error(`The preview isn't running at ${PREVIEW}. Start it with \`pnpm dev\` first.`);
  process.exit(1);
}

const profile = mkdtempSync(join(tmpdir(), 'shotmaxxing-shots-'));
const chrome = spawn(
  CHROME,
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${profile}`,
    '--hide-scrollbars',
    '--no-first-run',
    '--no-default-browser-check',
    'about:blank',
  ],
  { stdio: 'ignore' },
);

try {
  // The page to drive, once the browser is listening.
  let target;
  for (let i = 0; i < 100 && !target; i++) {
    await wait(100);
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      target = list.find((t) => t.type === 'page');
    } catch {
      // Not listening yet.
    }
  }
  if (!target) throw new Error(`Chrome didn't start (${CHROME}). Set CHROME to its path.`);

  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  let next = 0;
  const waiting = new Map();
  socket.onmessage = (message) => {
    const reply = JSON.parse(message.data);
    if (!waiting.has(reply.id)) return;
    const { resolve, reject } = waiting.get(reply.id);
    waiting.delete(reply.id);
    if (reply.error) reject(new Error(reply.error.message));
    else resolve(reply.result);
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++next;
      waiting.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) =>
    (await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })).result?.value;

  /** Loads `route` afresh and waits for it to settle: fonts in, and nothing still arriving. */
  async function open(route) {
    await send('Page.navigate', { url: 'about:blank' });
    await send('Page.navigate', { url: `${PREVIEW}/#/${route}` });
    for (let i = 0; i < 50 && (await evaluate('document.readyState')) !== 'complete'; i++) await wait(100);
    await evaluate('document.fonts.ready.then(() => true)');
    await wait(1500);
  }

  await send('Page.enable');
  mkdirSync(OUT, { recursive: true });
  for (const shot of SHOTS) {
    const theme = shot.theme ?? 'dark';
    const [width, height] = shot.size;
    await send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 2, mobile: false });
    await send('Emulation.setEmulatedMedia', {
      features: [
        { name: 'prefers-color-scheme', value: theme },
        { name: 'prefers-reduced-motion', value: 'reduce' },
      ],
    });
    // Past the onboarding and the first-visit tours, in this theme.
    await open('home/library');
    await evaluate(`(() => {
      const saved = JSON.parse(localStorage.getItem('smx.settings') || '{}');
      const done = { onboardingDone: true, tourDone: true, stillTourDone: true, firstExportDone: true };
      localStorage.setItem('smx.settings', JSON.stringify({ ...saved, ...done, theme: '${theme}' }));
      localStorage.setItem('smx.theme', '${theme}');
      return true;
    })()`);
    await open(shot.route);
    const clip = shot.clip && { ...shot.clip, scale: 1 };
    const { data } = await send('Page.captureScreenshot', { format: 'png', ...(clip && { clip }) });
    writeFileSync(join(OUT, `${shot.name}.png`), Buffer.from(data, 'base64'));
    const [w, h] = clip ? [clip.width, clip.height] : [width, height];
    console.log(`docs/images/${shot.name}.png  ${w * 2}×${h * 2}`);
  }
  socket.close();
} finally {
  chrome.kill();
  await wait(300);
  rmSync(profile, { recursive: true, force: true });
}
