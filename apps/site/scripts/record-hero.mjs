#!/usr/bin/env node
/**
 * Films the landing page's hero video from the real Shotmaxxing editor, playing its sample project.
 *
 * The desktop app's interface runs in a browser with sample data (`pnpm --filter @shotmaxxing/desktop dev`, port
 * 1420). This opens it in headless Chrome, skips setup, opens the sample in the editor and plays it on a clock this
 * script moves by hand: `performance.now`, `requestAnimationFrame` and every CSS animation advance exactly one frame
 * between captures, so the video plays at true speed and stays smooth however slow each capture is. Then ffmpeg makes an MP4 and a WebM for the page and a GIF for the README.
 *
 *   node scripts/record-hero.mjs [--seconds 24] [--fps 30] [--url http://localhost:1420]
 *
 * Needs Google Chrome and ffmpeg. Writes public/hero/ (served at /hero/…) and ../../docs/images/hero.gif.
 */
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const SECONDS = Number(arg('seconds', 21));
const FPS = Number(arg('fps', 30));
const URL = arg('url', 'http://localhost:1420');
const CHROME = arg('chrome', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome');
const WIDTH = 1440;
const HEIGHT = 900;
const SCALE = 2;

const here = resolve(import.meta.dirname, '..');
const outDir = join(here, 'public', 'hero');
const work = mkdtempSync(join(tmpdir(), 'smx-hero-'));
const frames = join(work, 'frames');
mkdirSync(frames, { recursive: true });
mkdirSync(outDir, { recursive: true });

// What happens on screen, by time into the video: the editor's own shortcuts open its tools while the sample plays.
// It starts and ends on the Background tool with the cursor in the same place, so the loop has no jump.
const ACTIONS = [
  { at: 0.0, key: ' ', note: 'play' },
  { at: 2.9, key: '3', note: 'Zoom tool: every zoom with its reason' },
  { at: 9.6, key: '2', note: 'Cursor tool' },
  { at: 16.0, key: '1', note: 'Background tool' },
];

// Runs in the page before the app: a clock that only moves when the script says so.
const CLOCK = `(() => {
  const realNow = performance.now.bind(performance);
  const realRaf = requestAnimationFrame.bind(window);
  const realCaf = cancelAnimationFrame.bind(window);
  let manual = false, clock = 0, id = 0;
  const queue = new Map();
  const born = new WeakMap();
  performance.now = () => (manual ? clock : realNow());
  window.requestAnimationFrame = (cb) => {
    if (!manual) return realRaf(cb);
    queue.set(++id, cb);
    return -id;
  };
  window.cancelAnimationFrame = (n) => (n < 0 ? queue.delete(-n) : realCaf(n));
  const sync = () => {
    for (const a of document.getAnimations()) {
      if (!born.has(a)) { born.set(a, clock); a.pause(); }
      const at = clock - born.get(a);
      const end = a.effect?.getComputedTiming().endTime ?? Infinity;
      if (Number.isFinite(end) && at >= end) a.finish();
      else a.currentTime = at;
    }
  };
  window.__hero = {
    start() { clock = realNow(); manual = true; },
    async step(ms) {
      clock += ms;
      const due = [...queue.values()];
      queue.clear();
      for (const cb of due) cb(clock);
      await new Promise((r) => setTimeout(r, 0));
      sync();
      await new Promise((r) => setTimeout(r, 0));
    },
  };
})();`;

const chrome = spawn(
  CHROME,
  [
    '--headless=new',
    '--remote-debugging-port=9333',
    `--user-data-dir=${join(work, 'profile')}`,
    `--window-size=${WIDTH},${HEIGHT}`,
    '--hide-scrollbars',
    '--mute-audio',
    '--no-first-run',
    '--no-default-browser-check',
    'about:blank',
  ],
  { stdio: 'ignore' },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function target() {
  for (let i = 0; i < 50; i++) {
    try {
      const list = await (await fetch('http://127.0.0.1:9333/json/list')).json();
      const page = list.find((t) => t.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* not up yet */
    }
    await sleep(200);
  }
  throw new Error('Chrome did not start');
}

const ws = new WebSocket(await target());
await new Promise((r) => ws.addEventListener('open', r, { once: true }));
let seq = 0;
const waiting = new Map();
const listeners = new Map();
ws.addEventListener('message', (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && waiting.has(msg.id)) {
    const { ok, fail } = waiting.get(msg.id);
    waiting.delete(msg.id);
    msg.error ? fail(new Error(`${msg.error.message}`)) : ok(msg.result);
  } else if (msg.method) {
    listeners.get(msg.method)?.forEach((f) => f(msg.params));
  }
});
const log = process.env.DEBUG ? (...a) => console.error('·', ...a) : () => {};
const send = (method, params = {}) =>
  new Promise((ok, fail) => {
    log(method);
    const id = ++seq;
    waiting.set(id, { ok, fail });
    ws.send(JSON.stringify({ id, method, params }));
  });
const once = (method) =>
  new Promise((r) => {
    const set = listeners.get(method) ?? new Set();
    const f = (p) => {
      set.delete(f);
      r(p);
    };
    set.add(f);
    listeners.set(method, set);
  });

try {
  await send('Page.enable');
  await send('Page.addScriptToEvaluateOnNewDocument', { source: CLOCK });
  await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride', {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: SCALE,
    mobile: false,
  });
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });

  // Setup and the tour are done, updates aren't checked, and the theme is dark.
  let loaded = once('Page.loadEventFired');
  await send('Page.navigate', { url: `${URL}/` });
  await loaded;
  const settings = {
    onboardingDone: true,
    tourDone: true,
    stillTourDone: true,
    firstExportDone: true,
    checkUpdates: false,
    theme: 'dark',
  };
  await send('Runtime.evaluate', {
    expression: `localStorage.setItem('smx.settings', ${JSON.stringify(JSON.stringify(settings))}); localStorage.setItem('smx.theme', 'dark');`,
  });
  // Only the hash changes, which isn't a new page load, so reload with it to start the app with these settings.
  loaded = once('Page.loadEventFired');
  await send('Runtime.evaluate', { expression: `location.hash = '#/editor/sample'; location.reload();` });
  await loaded;
  await sleep(2500); // fonts, pictures and the sample's first frame

  const KEYS = { ' ': ['Space', 32], Home: ['Home', 36] };
  const press = async (key) => {
    const [code, keyCode] = KEYS[key] ?? [`Digit${key}`, key.charCodeAt(0)];
    const base = { key, code, windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode };
    await send('Input.dispatchKeyEvent', {
      type: 'keyDown',
      ...(key.length === 1 ? { text: key } : {}),
      ...base,
    });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
  };
  await press('Home'); // the sample opens part way in
  await sleep(300);

  // From here, time only moves when this script says so.
  await send('Runtime.evaluate', { expression: '__hero.start()' });
  const total = Math.round(SECONDS * FPS);
  const todo = [...ACTIONS];
  for (let f = 0; f < total; f++) {
    const t = f / FPS;
    while (todo.length && todo[0].at <= t) await press(todo.shift().key);
    if (f > 0)
      await send('Runtime.evaluate', { expression: `__hero.step(${1000 / FPS})`, awaitPromise: true });
    const shot = await send('Page.captureScreenshot', { format: 'jpeg', quality: 94 });
    writeFileSync(join(frames, `${String(f).padStart(5, '0')}.jpg`), Buffer.from(shot.data, 'base64'));
    if (f % FPS === 0) process.stdout.write(`\r${Math.round(t)} s of ${SECONDS}`);
  }
  process.stdout.write('\n');
  // The editor's own clock should read the length of the video (it stops at the sample's end).
  const shown = await send('Runtime.evaluate', {
    expression: `document.body.innerText.match(/\\d:\\d\\d\\.\\d(?= \\/)/)?.[0]`,
    returnByValue: true,
  });
  console.log(`Filmed ${SECONDS} s; the editor's clock reads ${shown.result.value}`);
} finally {
  ws.close();
  chrome.kill();
}

// The page plays 1600 px wide at most, so 2× that is plenty for sharp text on a retina screen.
const ff = (args) => {
  const r = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', ...args], { stdio: 'inherit' });
  if (r.status) throw new Error(`ffmpeg failed: ${args.join(' ')}`);
};
const input = ['-framerate', String(FPS), '-i', join(frames, '%05d.jpg')];
// A seamless loop: the video starts 0.6 s in, and its last 0.6 s fade into those first 0.6 s, so the end flows into
// the start instead of jumping.
const D = 0.6;
const loop = (tail) =>
  `split[a][b];[a]trim=start=${D},setpts=PTS-STARTPTS[x];[b]trim=end=${D},setpts=PTS-STARTPTS[y];` +
  `[x][y]xfade=transition=fade:duration=${D}:offset=${SECONDS - 2 * D},${tail}`;
const big = loop('scale=2400:-2:flags=lanczos');
ff([
  ...input,
  '-filter_complex',
  big,
  '-c:v',
  'libx264',
  '-preset',
  'slow',
  '-crf',
  '24',
  '-pix_fmt',
  'yuv420p',
  '-movflags',
  '+faststart',
  '-an',
  join(outDir, 'hero.mp4'),
]);
ff([
  ...input,
  '-filter_complex',
  big,
  '-c:v',
  'libvpx-vp9',
  '-b:v',
  '0',
  '-crf',
  '40',
  '-row-mt',
  '1',
  '-an',
  join(outDir, 'hero.webm'),
]);
ff(['-i', join(outDir, 'hero.mp4'), '-frames:v', '1', '-q:v', '3', join(outDir, 'hero-poster.jpg')]);
ff([
  ...input,
  '-filter_complex',
  loop(
    'fps=12,scale=960:-2:flags=lanczos,split[p1][p2];[p1]palettegen=max_colors=160:stats_mode=diff[p];[p2][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle',
  ),
  join(here, '..', '..', 'docs', 'images', 'hero.gif'),
]);
rmSync(work, { recursive: true, force: true });
console.log('Wrote public/hero/hero.mp4, hero.webm, hero-poster.jpg and docs/images/hero.gif');
