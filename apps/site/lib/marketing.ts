/**
 * The comparison and use-case pages (app/alternatives, app/vs, app/use-cases): what each says, kept here as data so
 * the pages, the sitemap and llms.txt list the same ones.
 *
 * Claims about other apps stick to what their own sites said when checked (see `CHECKED`). Where we don't know, the
 * table says so rather than guess.
 */

/** When the other apps' sites were last read for these pages. */
export const CHECKED = 'November 2026';

export interface Row {
  feature: string;
  us: string;
  them: string;
}

export interface Comparison {
  /** The page's path, e.g. /vs/loom. */
  path: string;
  /** The other app. */
  name: string;
  site: string;
  /** The <title> and the search snippet. */
  title: string;
  description: string;
  eyebrow: string;
  h1: string;
  lede: string;
  rows: Row[];
  /** Why people pick Shotmaxxing instead. */
  reasons: { title: string; text: string }[];
  /** When the other app is the better pick: said plainly. */
  theirs: string;
  faq: [string, string][];
}

const AGENT_ROW = (them = 'Not that we know of'): Row => ({
  feature: 'An AI agent can film the demo for you',
  us: 'Yes, over MCP: Claude Code, Codex, Cursor and more',
  them,
});

export const COMPARISONS: Comparison[] = [
  {
    path: '/alternatives/screen-studio',
    name: 'Screen Studio',
    site: 'https://screen.studio',
    title: 'Free Screen Studio alternative: open source, with AI agents',
    description:
      'Shotmaxxing is a free, open-source Screen Studio alternative for Mac: automatic zooms, a smooth cursor, beautiful backgrounds, and AI agents that can film the demo for you.',
    eyebrow: 'Screen Studio alternative',
    h1: 'The free, open-source Screen Studio alternative.',
    lede: 'The polished look people buy Screen Studio for, automatic zooms, a smooth cursor and a beautiful frame, without a subscription. And an AI agent can film the take for you.',
    rows: [
      { feature: 'Price', us: 'Free, with no account, trial or watermark', them: 'Paid subscription' },
      { feature: 'Source code', us: 'Open (AGPL-3.0)', them: 'Closed' },
      { feature: 'Runs on', us: 'macOS (Windows builds in testing)', them: 'macOS' },
      {
        feature: 'Automatic zooms on clicks and typing',
        us: 'Yes, each with the reason it is there',
        them: 'Yes',
      },
      { feature: 'Smooth, bigger cursor', us: 'Yes: 8 styles, sway, click bounce', them: 'Yes' },
      { feature: 'Backgrounds, padding and shadow', us: 'Photos, colours, gradients and blur', them: 'Yes' },
      AGENT_ROW(),
    ],
    reasons: [
      {
        title: 'No subscription',
        text: 'Shotmaxxing is free and open source. Nothing stops working when a card expires, and there is no watermark on the export.',
      },
      {
        title: 'Agents can film it',
        text: 'Ask Claude Code or Codex to “film a demo of the sign-up flow” and it plans the take, drives the app at a human pace and hands you an edited project.',
      },
      {
        title: 'Yours to read and change',
        text: 'The whole app is on GitHub. Read how it records, file an issue, or send the feature you need.',
      },
    ],
    theirs:
      'Screen Studio is a mature, polished app with years of refinement. If you want a paid product with its own support team and don’t need AI agents, it is a fine choice.',
    faq: [
      [
        'Is Shotmaxxing really a free alternative to Screen Studio?',
        'Yes. Shotmaxxing is free and open source under the AGPL-3.0. It records your screen, adds automatic zooms on clicks and typing, smooths and enlarges the cursor, and puts the recording on a background with padding, corners and a shadow.',
      ],
      [
        'Can I open my Screen Studio projects in Shotmaxxing?',
        'No. The project formats are different, so record the demo again. With an agent filming it, that can be one sentence.',
      ],
      [
        'Does Shotmaxxing work on Windows?',
        'Windows builds exist, but they haven’t been tested on a Windows computer yet, so expect rough edges. macOS is the tested platform.',
      ],
    ],
  },
  {
    path: '/vs/recordly',
    name: 'Recordly',
    site: 'https://recordly.dev',
    title: 'Shotmaxxing vs Recordly: two open-source screen studios compared',
    description:
      'Shotmaxxing and Recordly are both free, open-source screen recorders with automatic zooms. Here is how they differ, including AI agents that film takes for you.',
    eyebrow: 'Shotmaxxing vs Recordly',
    h1: 'Shotmaxxing vs Recordly.',
    lede: 'Two free, open-source screen studios with automatic zooms and a polished frame. The biggest difference: in Shotmaxxing, an AI agent can film the demo for you.',
    rows: [
      { feature: 'Price', us: 'Free', them: 'Free' },
      { feature: 'Source code', us: 'Open (AGPL-3.0)', them: 'Open (AGPL-3.0)' },
      { feature: 'Runs on', us: 'macOS (Windows builds in testing)', them: 'macOS, Windows and Linux' },
      { feature: 'Automatic zooms', us: 'Yes, each with the reason it is there', them: 'Yes' },
      { feature: 'Cursor styles and effects', us: '8 styles, sway, click bounce, motion blur', them: 'Yes' },
      { feature: 'Backgrounds', us: 'Photos, colours, gradients and blur', them: 'Yes' },
      AGENT_ROW(),
      {
        feature: 'Planned takes with a safety card',
        us: 'Yes: asks first, Esc three times stops it',
        them: 'Not that we know of',
      },
    ],
    reasons: [
      {
        title: 'Agents that film',
        text: 'Shotmaxxing has an MCP server built in. An agent reads your app’s code, writes a plan, and films it at a human pace while you watch the take card.',
      },
      {
        title: 'A native app',
        text: 'Shotmaxxing is Rust and Tauri, with capture on ScreenCaptureKit, so it stays light and keeps your computer cool while it records.',
      },
      {
        title: 'Zooms that explain themselves',
        text: 'Every automatic zoom on the timeline says why it is there, “Click on Create account”, so you know which to keep.',
      },
    ],
    theirs:
      'Recordly runs on Linux today, and Shotmaxxing doesn’t yet. If you record on Linux, or want its webcam and extension features, try Recordly.',
    faq: [
      ['Are Shotmaxxing and Recordly both free?', 'Yes. Both are free and open source under the AGPL-3.0.'],
      [
        'Which one works with AI agents?',
        'Shotmaxxing ships an MCP server, so Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode and Gemini CLI can plan and film takes with it.',
      ],
    ],
  },
  {
    path: '/vs/loom',
    name: 'Loom',
    site: 'https://www.loom.com',
    title: 'Shotmaxxing vs Loom: polished demos on your computer, not quick links in the cloud',
    description:
      'Loom is for quick video messages shared as links. Shotmaxxing makes polished, edited demo videos on your computer, with automatic zooms and no account.',
    eyebrow: 'Shotmaxxing vs Loom',
    h1: 'Shotmaxxing vs Loom.',
    lede: 'Loom is great for a quick video message with a link. Shotmaxxing is for the demo you put on your website, in your docs or in a launch post: zoomed, smooth and framed.',
    rows: [
      {
        feature: 'Made for',
        us: 'Polished demos, tutorials and launch videos',
        them: 'Quick video messages shared as links',
      },
      { feature: 'Where recordings live', us: 'On your computer', them: 'Loom’s cloud' },
      { feature: 'Account', us: 'None', them: 'Needed' },
      { feature: 'Price', us: 'Free and open source', them: 'Free plan with limits, paid plans' },
      { feature: 'Automatic zooms and smooth cursor', us: 'Yes', them: 'Not the focus' },
      { feature: 'Share as a link', us: 'No, you export a file', them: 'Yes, at once' },
      AGENT_ROW(),
    ],
    reasons: [
      {
        title: 'It comes out edited',
        text: 'Zooms follow your clicks and typing, the cursor glides, and the recording sits on a background. No timeline work needed.',
      },
      {
        title: 'Nothing is uploaded',
        text: 'Recordings, projects and captions stay on your computer, which matters for unreleased products and client work.',
      },
      {
        title: 'Made to publish',
        text: 'Export 16:9 for YouTube, 1:1 for X and LinkedIn, 9:16 for Shorts and Reels, a GIF for docs, or 4K.',
      },
    ],
    theirs:
      'For a fast “here’s what I mean” message to a teammate, with comments and view counts, Loom is the better tool. Many people use both.',
    faq: [
      [
        'Is Shotmaxxing a Loom alternative?',
        'For polished demo videos, yes. For quick messages shared as links with comments, Loom does something Shotmaxxing doesn’t: Shotmaxxing exports a file you upload wherever you like.',
      ],
      [
        'Does Shotmaxxing upload my recordings?',
        'No. Everything stays on your computer. The app only checks for a newer release when it opens and every six hours, which you can turn off.',
      ],
    ],
  },
  {
    path: '/vs/cap',
    name: 'Cap',
    site: 'https://cap.so',
    title: 'Shotmaxxing vs Cap: open-source screen recorders compared',
    description:
      'Cap and Shotmaxxing are both open-source screen recorders. Cap pairs a studio editor with cloud share links; Shotmaxxing is local-only, free for every use, and lets AI agents film for you.',
    eyebrow: 'Shotmaxxing vs Cap',
    h1: 'Shotmaxxing vs Cap.',
    lede: 'Both are open source and both make polished recordings. Cap adds instant cloud share links. Shotmaxxing keeps everything on your computer, is free for any use, and lets an AI agent film the demo.',
    rows: [
      { feature: 'Source code', us: 'Open (AGPL-3.0)', them: 'Open' },
      {
        feature: 'Price',
        us: 'Free for any use',
        them: 'Free for personal use, paid licence and cloud plans',
      },
      { feature: 'Runs on', us: 'macOS (Windows builds in testing)', them: 'macOS and Windows' },
      { feature: 'Share as a cloud link', us: 'No, you export a file', them: 'Yes' },
      {
        feature: 'Automatic zooms',
        us: 'Yes, each with the reason it is there',
        them: 'Yes, in Studio mode',
      },
      AGENT_ROW(),
    ],
    reasons: [
      {
        title: 'Free for work too',
        text: 'Use it for your company’s product videos without a licence. There is no paid tier.',
      },
      {
        title: 'Agents that film',
        text: 'An AI agent can plan and film the take while you watch, then hand you an edited project to tweak.',
      },
      {
        title: 'Local by design',
        text: 'There is no cloud to sign in to. Recordings, captions and projects never leave your computer.',
      },
    ],
    theirs:
      'If your team wants instant share links, comments and a hosted library, or needs a tested Windows app today, Cap is a good pick.',
    faq: [
      [
        'Is Shotmaxxing free for commercial use?',
        'Yes. Shotmaxxing is free for any use under the AGPL-3.0, with no paid tier.',
      ],
      [
        'Can Shotmaxxing share a link like Cap?',
        'No. Shotmaxxing exports a video file, MP4 or GIF, that you upload wherever you like.',
      ],
    ],
  },
];

export interface UseCase {
  path: string;
  /** Short name, for links. */
  name: string;
  title: string;
  description: string;
  eyebrow: string;
  h1: string;
  lede: string;
  steps: { title: string; text: string; guide: [href: string, label: string] }[];
  tips: string[];
  faq: [string, string][];
}

export const USE_CASES: UseCase[] = [
  {
    path: '/use-cases/product-demos',
    name: 'Product demos',
    title: 'Make a product demo video that edits itself',
    description:
      'Record your product once and Shotmaxxing adds the zooms, smooths the cursor and frames it on a background. Free and open source, for Mac.',
    eyebrow: 'Product demos',
    h1: 'Product demo videos, without the editing.',
    lede: 'Record the flow once. Shotmaxxing zooms in on every click and on typing, makes the cursor easy to follow, and puts it on a beautiful background, ready for your landing page.',
    steps: [
      {
        title: 'Record the flow',
        text: 'Pick the window or the display and record. Shotmaxxing keeps where the cursor went and what you clicked and typed.',
        guide: ['/docs/recording', 'Recording'],
      },
      {
        title: 'Let it edit',
        text: 'Zooms follow the clicks and typing, each with its reason on the timeline. Keep, move or drop any of them.',
        guide: ['/docs/auto-zoom', 'Automatic zoom'],
      },
      {
        title: 'Make it yours',
        text: 'Choose a photo, colour or gradient behind it, a cursor style and size, and the shape for where it goes.',
        guide: ['/docs/background', 'Background and shape'],
      },
      {
        title: 'Export',
        text: 'MP4 for the web, 1:1 or 9:16 for social, a GIF for the README, or 4K.',
        guide: ['/docs/export', 'Export'],
      },
    ],
    tips: [
      'Make the cursor 1.5× and turn Smoothing up: it reads on a phone.',
      'Record at the size you’ll show it. A smaller window means crisper zooms.',
      'Turn on Snap to clicks, so near misses land on the button you meant.',
    ],
    faq: [
      [
        'How do I make a product demo video on a Mac?',
        'Install Shotmaxxing, record the flow you want to show, and export. It adds automatic zooms on clicks and typing, a smooth cursor and a background, so the export is ready for a landing page without manual editing.',
      ],
      [
        'How long should a product demo be?',
        'For a landing page, 15 to 45 seconds that show one job from start to finish. Shotmaxxing can cut silences and speed up the slow parts.',
      ],
    ],
  },
  {
    path: '/use-cases/tutorials',
    name: 'Tutorials and docs',
    title: 'Record tutorials and docs videos with captions',
    description:
      'Record clear software tutorials with automatic zooms, a cursor that is easy to follow, captions made on your computer, and GIFs and screenshots for your docs.',
    eyebrow: 'Tutorials and docs',
    h1: 'Tutorials people can actually follow.',
    lede: 'Zooms land where you click, the cursor is big and smooth, and captions are made on your computer. Add a GIF or a framed screenshot to your docs from the same app.',
    steps: [
      {
        title: 'Record with your voice',
        text: 'Record the screen with your microphone, and the camera if you like. Clean voice removes the room’s noise.',
        guide: ['/docs/sound', 'Sound'],
      },
      {
        title: 'Tidy it up',
        text: 'Cut the silences in one click, and let automatic zoom follow what you do.',
        guide: ['/docs/editor', 'The editor'],
      },
      {
        title: 'Add captions',
        text: 'Captions are made on your computer from what you said, and you can edit any word.',
        guide: ['/docs/captions', 'Captions'],
      },
      {
        title: 'Screenshots too',
        text: 'Framed screenshots with the same backgrounds, for the steps that need a still.',
        guide: ['/docs/screenshots', 'Screenshots'],
      },
    ],
    tips: [
      'Point with the cursor before you click: the zoom has time to arrive.',
      'Export a GIF at 800 px for docs pages; it loops without a jump.',
      'Use the Annotate tool to label the one button that matters.',
    ],
    faq: [
      [
        'Can Shotmaxxing make captions without uploading audio?',
        'Yes. Captions are made by a speech model that runs on your computer. It downloads once, the first time you make captions.',
      ],
      [
        'Can I make GIFs for documentation?',
        'Yes. The GIF preset exports 800 px on the long side at 15 fps, and Loop to start makes it end where it began.',
      ],
    ],
  },
  {
    path: '/use-cases/launch-videos',
    name: 'Launch and social videos',
    title: 'Make launch videos for X, LinkedIn, Product Hunt and Reels',
    description:
      'Turn one screen recording into launch videos for X, LinkedIn, Product Hunt, YouTube Shorts and Reels, with zooms, music and captions. Free and open source.',
    eyebrow: 'Launch and social videos',
    h1: 'One recording, every launch post.',
    lede: 'Record once, then export 16:9 for YouTube and Product Hunt, 1:1 for X and LinkedIn, and 9:16 for Shorts, Reels and TikTok. Zooms and captions come along.',
    steps: [
      {
        title: 'Record the moment',
        text: 'Show the one thing people should remember. Shotmaxxing does the zooms.',
        guide: ['/docs/recording', 'Recording'],
      },
      {
        title: 'Pick a look',
        text: 'A photo, colour or gradient, padding, corners and shadow, or a look from a content pack in one click.',
        guide: ['/docs/background', 'Background and shape'],
      },
      {
        title: 'Add sound',
        text: 'Music you chose, click sounds, and captions for people watching muted.',
        guide: ['/docs/sound', 'Sound'],
      },
      {
        title: 'Export for each place',
        text: 'The presets choose the shape, size and frame rate for each network.',
        guide: ['/docs/export', 'Export'],
      },
    ],
    tips: [
      'Most social video plays muted: turn captions on.',
      'Lead with the result, then show how. The first two seconds decide.',
      'Bold zoom depth reads better on a phone.',
    ],
    faq: [
      [
        'What shape should a launch video be?',
        '16:9 for YouTube, websites and Product Hunt; 1:1 for X and LinkedIn feeds; 9:16 for Shorts, Reels and TikTok. Shotmaxxing’s export presets set the shape for you.',
      ],
      [
        'Can I add music?',
        'Yes. Add a music file you chose; Shotmaxxing doesn’t include any music of its own.',
      ],
    ],
  },
  {
    path: '/use-cases/ai-agent-demos',
    name: 'AI agent demos',
    title: 'Let an AI agent record your demo video',
    description:
      'Ask Claude Code, Codex or Cursor to film a demo of your app. Shotmaxxing plans the take from your code, drives the app at a human pace, and hands you an edited video.',
    eyebrow: 'AI agent demos',
    h1: 'Ask your AI agent to film the demo.',
    lede: 'Tell Claude Code “film a demo of the sign-up flow with Shotmaxxing”. It reads your code, writes a plan, and films it while you watch, then hands you an edited project.',
    steps: [
      {
        title: 'Connect your agent',
        text: 'One click on the Agents page connects Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode or Gemini CLI.',
        guide: ['/docs/agents', 'Agents'],
      },
      {
        title: 'Ask for the demo',
        text: 'The agent reads the routes and labels from your code, then shows you a short plan before it films.',
        guide: ['/docs/agents', 'Agents'],
      },
      {
        title: 'Allow the take',
        text: 'The take card asks first and shows the plan. A red edge goes round the screen while it drives; Esc three times stops it.',
        guide: ['/docs/permissions', 'Permissions'],
      },
      {
        title: 'Tweak and export',
        text: 'The take opens in the editor, already zoomed and styled. Change anything, or ask the agent to.',
        guide: ['/docs/command-line', 'Command line'],
      },
    ],
    tips: [
      'Have the dev server running and test data in place before you ask.',
      'Ask for the plan first, then say “film it” once it looks right.',
      'Agents never type into password fields: sign-ins come to you.',
    ],
    faq: [
      [
        'Can an AI agent record my screen?',
        'With Shotmaxxing, yes, and only when you allow it. The app asks before an agent sees or records the screen and before every take, and pressing Esc three times or moving the mouse stops it.',
      ],
      [
        'Which AI agents work with Shotmaxxing?',
        'Any agent that speaks MCP. Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode and Gemini CLI connect in one click.',
      ],
    ],
  },
];

/** Every marketing page, for the sitemap, llms.txt and the footer. */
export const MARKETING_PAGES = [
  ...COMPARISONS.map((c) => ({ path: c.path, title: c.title, description: c.description })),
  ...USE_CASES.map((u) => ({ path: u.path, title: u.title, description: u.description })),
];
