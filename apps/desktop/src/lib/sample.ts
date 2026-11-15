/**
 * Sample content for previews, the sample project and first-run screens. Everything is fictional: the products
 * shown inside recordings ("Northwind Notes") are made up.
 */

const now = Date.now();
const hours = (h: number) => new Date(now - h * 3600_000);

export interface LibraryItem {
  id: string;
  title: string;
  app: string;
  duration: number;
  created: Date;
  wallpaper: string;
  scene: number;
  agent?: string;
  kind: 'recording' | 'screenshot';
  sizeMb: number;
  /** Real projects: where the project folder is, and a URL for its video. */
  path?: string;
  media?: string;
}

export const LIBRARY: LibraryItem[] = [
  {
    id: 'launch-demo',
    title: 'Launch demo',
    app: 'Safari',
    duration: 42,
    created: hours(2),
    wallpaper: 'aurora-lime',
    scene: 0,
    agent: 'Claude Code',
    kind: 'recording',
    sizeMb: 38,
  },
  {
    id: 'onboarding-flow',
    title: 'Onboarding flow',
    app: 'Figma',
    duration: 75,
    created: hours(26),
    wallpaper: 'dusk',
    scene: 1,
    kind: 'recording',
    sizeMb: 61,
  },
  {
    id: 'export-bug',
    title: 'Bug repro: export stalls at 99%',
    app: 'Display',
    duration: 9,
    created: hours(74),
    wallpaper: 'graphite',
    scene: 2,
    kind: 'recording',
    sizeMb: 7,
  },
  {
    id: 'sheets-sum',
    title: 'Budget with =SUM',
    app: 'Chrome',
    duration: 37,
    created: hours(96),
    wallpaper: 'mint',
    scene: 3,
    agent: 'Codex',
    kind: 'recording',
    sizeMb: 24,
  },
  {
    id: 'pricing-tour',
    title: 'Pricing page walkthrough',
    app: 'Safari',
    duration: 58,
    created: hours(170),
    wallpaper: 'ocean',
    scene: 4,
    agent: 'Claude Code',
    kind: 'recording',
    sizeMb: 44,
  },
  {
    id: 'release-notes',
    title: 'Release notes 0.3',
    app: 'Notes',
    duration: 0,
    created: hours(200),
    wallpaper: 'peach',
    scene: 5,
    kind: 'screenshot',
    sizeMb: 1,
  },
  {
    id: 'weekly-update',
    title: 'Weekly product update',
    app: 'Keynote',
    duration: 151,
    created: hours(320),
    wallpaper: 'violet-haze',
    scene: 1,
    kind: 'recording',
    sizeMb: 112,
  },
  {
    id: 'settings-tour',
    title: 'Settings tour',
    app: 'Shotmaxxing',
    duration: 62,
    created: hours(400),
    wallpaper: 'midnight',
    scene: 0,
    kind: 'recording',
    sizeMb: 49,
  },
];

export type AgentState = 'connected' | 'available' | 'not_installed';

export interface Agent {
  id: string;
  name: string;
  kind: string;
  state: AgentState;
  lastTake?: string;
}

export const AGENTS: Agent[] = [
  { id: 'claude-code', name: 'Claude Code', kind: 'Terminal agent', state: 'connected', lastTake: '2 h ago' },
  { id: 'codex', name: 'Codex', kind: 'Terminal agent and app', state: 'connected', lastTake: '4 days ago' },
  { id: 'claude-desktop', name: 'Claude Desktop', kind: 'Desktop app', state: 'available' },
  { id: 'cursor', name: 'Cursor', kind: 'Code editor', state: 'available' },
  { id: 'vscode', name: 'VS Code', kind: 'Code editor', state: 'available' },
  { id: 'windsurf', name: 'Windsurf', kind: 'Code editor', state: 'not_installed' },
  { id: 'opencode', name: 'OpenCode', kind: 'Terminal agent', state: 'not_installed' },
  { id: 'gemini-cli', name: 'Gemini CLI', kind: 'Terminal agent', state: 'available' },
];

export const STARTER_PROMPTS = [
  'Film a 30-second demo of my app’s sign-up flow and export it for X.',
  'Record a walkthrough of shotmaxxing.dev in Safari, zooming on each section.',
  'Show how to total a budget in Google Sheets with =SUM, then open it in the editor.',
];

export interface Guide {
  id: string;
  title: string;
  minutes: number;
  group: 'Record' | 'Edit' | 'Export' | 'Use with AI';
  summary: string;
  steps: string[];
  tool?: string;
}

export const GUIDES: Guide[] = [
  {
    id: 'record-window',
    title: 'Record one window',
    minutes: 1,
    group: 'Record',
    summary: 'Film just the app you are showing, with nothing else on screen.',
    steps: [
      'Click Record in the menu bar or press ⌥⇧5.',
      'Choose Window, then click the window you want to film.',
      'Use the app as you normally would. Shotmaxxing hides the real cursor and draws a smooth one later.',
      'Click Stop in the recording pill. The editor opens with zooms already added.',
    ],
  },
  {
    id: 'record-camera',
    title: 'Record with your camera and voice',
    minutes: 2,
    group: 'Record',
    summary: 'Add yourself in a bubble and talk people through it.',
    steps: [
      'In the record dialog, turn on Camera and Microphone.',
      'Pick the camera and microphone you want from the lists.',
      'After recording, open Camera in the editor to move or resize the bubble.',
      'Turn on Shrink on zoom so the bubble never covers the action.',
    ],
    tool: 'camera',
  },
  {
    id: 'screenshot',
    title: 'Take a styled screenshot',
    minutes: 1,
    group: 'Record',
    summary: 'A window on a background with a real shadow, copied in one step.',
    steps: [
      'Press ⌥⇧4, choose Screen or Window, and pick what to capture.',
      'Pick a background, padding and corners. The next screenshot starts with the same look.',
      'Add a title, an arrow or a blur, then press ⌘C to copy it at full resolution, or Save image for a PNG.',
    ],
  },
  {
    id: 'auto-zoom',
    title: 'How auto zoom decides',
    minutes: 3,
    group: 'Edit',
    summary: 'Where the camera pushes in, why, and how to tune it.',
    steps: [
      'Auto zoom reads your clicks, typing, the control you used and what changed on screen.',
      'Only what you record counts: a click somewhere else, or the one that stops the recording, makes no zoom.',
      'The camera pushes in just before each moment and holds through it, so the picture is still as you click or type.',
      'Each zoom on the timeline says why it exists. Hover a block to read it.',
      'Choose Subtle, Standard or Bold for how far it pushes in.',
      'Open Advanced to change how long zooms last and when nearby moments merge.',
      'Drag a block’s edges to fix one, or delete it with ⌫.',
    ],
    tool: 'zoom',
  },
  {
    id: 'cursor',
    title: 'Make the cursor easy to follow',
    minutes: 2,
    group: 'Edit',
    summary: 'Bigger, smoother and landing exactly on every click.',
    steps: [
      'Open Cursor and pick Large or Huge.',
      'Raise Smoothing to iron out shaky hands.',
      'Keep Snap to clicks on so near misses land on the button.',
      'Pick a click effect: Ripple shows where you clicked, Press is subtler.',
    ],
    tool: 'cursor',
  },
  {
    id: 'voice-over',
    title: 'Record a voice-over',
    minutes: 1,
    group: 'Edit',
    summary: 'Talk people through the video after you’ve recorded it.',
    steps: [
      'Open Audio in the editor and put the playhead where you want to start talking.',
      'Click Record a voice-over. The video plays without sound while you talk, so nothing echoes.',
      'Press Space or Stop when you’re done. It starts where the playhead was and stays in step through later cuts.',
      'Set its volume, record another, or remove it with the cross.',
    ],
    tool: 'audio',
  },
  {
    id: 'captions',
    title: 'Add captions',
    minutes: 1,
    group: 'Edit',
    summary: 'Write down what’s said, on this computer, in any of 99 languages.',
    steps: [
      'Open AI in the editor and click Generate captions. The first time, it downloads a 60 MB speech model.',
      'It listens to your microphone (or the computer’s sound when there’s no microphone) and to any voice-overs.',
      'Fix any word in the list, or clear a line to remove it. Pick Bar or Bold for the look.',
      'While Show is on, captions are drawn in the preview and in exports. Save subtitles makes an SRT or WebVTT file to upload with the video.',
    ],
    tool: 'ai',
  },
  {
    id: 'trim',
    title: 'Trim, split and speed up',
    minutes: 2,
    group: 'Edit',
    summary: 'Cut the dead time without losing your place.',
    steps: [
      'Move the playhead to where a cut should start and press S to split.',
      'Select the part you don’t want and press ⌫.',
      'To speed up a slow part, select it and choose a speed in the clip menu.',
    ],
  },
  {
    id: 'blur',
    title: 'Blur private details',
    minutes: 1,
    group: 'Edit',
    summary: 'Hide emails, tokens and names before you share.',
    steps: [
      'Open Annotate and choose Blur.',
      'Drag a box over the private part on the canvas.',
      'Drag the blur’s ends on the timeline to cover the right moments.',
    ],
    tool: 'annotate',
  },
  {
    id: 'export-social',
    title: 'Export for YouTube, X and LinkedIn',
    minutes: 1,
    group: 'Export',
    summary: 'Pick where it’s going; Shotmaxxing picks the settings.',
    steps: [
      'Click Export, or press ⌘E.',
      'Choose a destination preset. The size and time estimate update right away.',
      'Keep editing while it exports. A notification tells you when it’s done.',
    ],
  },
  {
    id: 'gif',
    title: 'Make a looping GIF',
    minutes: 1,
    group: 'Export',
    summary: 'Small, smooth and with no jump at the loop point.',
    steps: [
      'In Cursor › Advanced, turn on Loop to start.',
      'Export with the GIF preset.',
      'Keep it under 15 seconds for a small file.',
    ],
  },
  {
    id: 'connect-agent',
    title: 'Connect Claude Code or Codex',
    minutes: 1,
    group: 'Use with AI',
    summary: 'One click, and your agent can film demos for you.',
    steps: [
      'Open Agents in the sidebar.',
      'Click Connect next to your agent. Shotmaxxing adds itself to the agent’s tools.',
      'Start a new chat with your agent and ask it to film something.',
    ],
  },
  {
    id: 'one-sentence',
    title: 'Film a demo with one sentence',
    minutes: 2,
    group: 'Use with AI',
    summary: 'Describe what viewers should see; the agent does the rest.',
    steps: [
      'Ask your agent something like “Film a 30-second demo of my app’s sign-up flow.”',
      'The agent prepares the app, plans every step, then films it in one smooth take.',
      'Watch it in the take card. Pause or take over at any time.',
      'The finished take opens in the editor, ready to tweak or export.',
    ],
  },
  {
    id: 'take-script',
    title: 'Write a take script',
    minutes: 3,
    group: 'Use with AI',
    summary: 'Plan a take in YAML and re-film it exactly, any time.',
    steps: [
      'Create a file like demo.yaml with an app and a list of beats.',
      'Beats are things like click, type, key, wait and say.',
      'Run it with shotmaxxing take run demo.yaml.',
      'Re-run it after every release to keep your docs videos current.',
    ],
  },
  {
    id: 'agent-handoff',
    title: 'When the agent needs you',
    minutes: 1,
    group: 'Use with AI',
    summary: 'Sign-ins and checks only you can do, without breaking the take.',
    steps: [
      'When a step needs you, the take card says what to do.',
      'Do the step yourself; the agent waits and touches nothing.',
      'Click Hand back, and the take carries on from there.',
    ],
  },
];

/** The editor's sample project: the "Launch demo" above, filmed by an agent. */
export interface ZoomBlock {
  id: string;
  start: number;
  end: number;
  scale: number;
  focus: { x: number; y: number };
  reason: string;
  source: 'auto' | 'manual';
}

export interface EffectBlock {
  id: string;
  kind: 'text' | 'blur' | 'arrow';
  start: number;
  end: number;
  label: string;
  /** Text: position as a fraction of the canvas. */
  pos?: { x: number; y: number };
  /** Blur: the region in capture points. */
  rect?: { x: number; y: number; width: number; height: number };
  /** Blur: 1 is the standard blur. */
  strength?: number;
  /** Arrow: from and to, in capture points. */
  from?: { x: number; y: number };
  to?: { x: number; y: number };
}

/** The sample's captions, as transcribing its voice would make them (seconds on the recording's timeline). */
export const SAMPLE_CAPTIONS = [
  { start: 0.4, end: 2.8, text: 'Here’s how sign-up works in Northwind.' },
  { start: 2.9, end: 6.4, text: 'Start with Get started, top right.' },
  { start: 9.0, end: 14.6, text: 'Then enter your work email.' },
  { start: 15.3, end: 18.6, text: 'Pick a password you’ll remember.' },
  { start: 18.8, end: 23.6, text: 'Create the account, and you’re in.' },
  { start: 30.0, end: 35.0, text: 'Pricing is one click away.' },
];

export const SAMPLE_PROJECT = {
  id: 'sample',
  title: 'Launch demo',
  duration: 42,
  capture: { width: 1440, height: 900 },
  zooms: [
    {
      id: 'z1',
      start: 2.1,
      end: 6.2,
      scale: 1.9,
      focus: { x: 1305, y: 110 },
      reason: 'Click on Get started',
      source: 'auto',
    },
    {
      id: 'z2',
      start: 9.0,
      end: 15.4,
      scale: 2.2,
      focus: { x: 1060, y: 440 },
      reason: 'Typing in Work email',
      source: 'auto',
    },
    {
      id: 'z3',
      start: 18.4,
      end: 23.6,
      scale: 1.6,
      focus: { x: 1060, y: 520 },
      reason: 'Click on Create account, then the result',
      source: 'auto',
    },
    {
      id: 'z4',
      start: 30.2,
      end: 35.0,
      scale: 1.45,
      focus: { x: 430, y: 110 },
      reason: 'Pointing at Pricing',
      source: 'manual',
    },
  ] as ZoomBlock[],
  effects: [
    {
      id: 'e1',
      kind: 'blur',
      start: 10.0,
      end: 15.0,
      label: 'Blur email',
      rect: { x: 840, y: 416, width: 440, height: 48 },
    },
    { id: 'e2', kind: 'text', start: 24.5, end: 29.0, label: 'New in 0.3' },
  ] as EffectBlock[],
  clicks: [
    { t: 3.0, x: 1305, y: 110 },
    { t: 9.4, x: 1060, y: 440 },
    { t: 19.2, x: 1060, y: 600 },
    { t: 31.0, x: 430, y: 110 },
  ],
};

/** A cursor path through the sample, keyed by time (seconds). */
export const SAMPLE_CURSOR: { t: number; x: number; y: number }[] = [
  { t: 0, x: 720, y: 760 },
  { t: 2.5, x: 1305, y: 110 },
  { t: 4.6, x: 1240, y: 190 },
  { t: 9.0, x: 1060, y: 440 },
  { t: 15.2, x: 1090, y: 460 },
  { t: 18.9, x: 1060, y: 600 },
  { t: 26.0, x: 860, y: 520 },
  { t: 30.6, x: 430, y: 110 },
  { t: 36.0, x: 760, y: 470 },
  { t: 42.0, x: 900, y: 640 },
];

/** Shortcuts shown in the sample, keyed by time (seconds). */
export const SAMPLE_KEYS: { t: number; label: string }[] = [
  { t: 15.2, label: '⇥  Tab' },
  { t: 19.0, label: '↩  Return' },
];
