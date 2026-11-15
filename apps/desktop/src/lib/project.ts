/**
 * Maps between a project as saved on disk (`project.json`, written by the Rust core) and the editor's own model.
 *
 * The editor works in seconds and in the few settings its panels show; the file keeps microseconds and more detail.
 * Saving starts from the file's last state and writes the editor's changes over it, so anything the editor doesn't
 * show (export settings, tuning, agent details) is kept as it was.
 */

import type { EffectBlock, ZoomBlock } from './sample';
import type { Aspect, Clip, Project } from '../routes/editor/store.svelte';

type Os = 'mac' | 'windows';

export interface CoreRange {
  start_us: number;
  end_us: number;
}

export interface CoreZoom {
  id: string;
  start_us: number;
  end_us: number;
  scale: number;
  focus: { x: number; y: number };
  follow_mouse?: boolean | null;
  ease_in_ms?: number;
  ease_out_ms?: number;
  source?: 'auto' | 'manual';
  reason?: string | null;
}

type ClickEffect = { kind: 'none' } | { kind: 'press' } | { kind: 'ripple'; color: string; size: number };

export interface CoreProject {
  id: string;
  name: string;
  /** A recording, or a screenshot framed the same way. Older projects leave it out. */
  kind?: 'video' | 'screenshot';
  source: { kind: string; app_name: string | null; size: { width: number; height: number }; scale: number };
  tracks: {
    id: string;
    kind: string;
    path: string;
    duration_us: number;
    offset_us: number;
    volume: number;
    muted: boolean;
    /** What to call the track, e.g. the music's own file name. */
    name?: string | null;
  }[];
  canvas: {
    aspect: string | { custom: { width: number; height: number } };
    background: { kind: string; id?: string };
    padding: number;
    corner_radius: number;
    shadow: { opacity: number; [key: string]: unknown };
    [key: string]: unknown;
  };
  cursor: {
    visible: boolean;
    theme: 'native' | 'macos_classic' | 'windows' | 'minimal';
    size: number;
    smoothing: number;
    snap_to_clicks: boolean;
    click_effect: ClickEffect;
    click_sound: boolean;
    motion_blur: number;
    loop_to_start: boolean;
    spotlight: { enabled: boolean; [key: string]: unknown };
    auto_hide_ms: number | null;
    [key: string]: unknown;
  };
  zoom: {
    auto: boolean;
    depth: string | { custom: number };
    motion: string | { custom: number };
    follow_mouse: boolean;
    keystrokes: boolean;
    tuning: {
      hold_ms: number;
      merge_gap_ms: number;
      dwell: boolean;
      typing?: boolean;
      [key: string]: unknown;
    };
    [key: string]: unknown;
  };
  zooms: CoreZoom[];
  edits: {
    cuts: CoreRange[];
    speed: { range: CoreRange; factor: number }[];
    texts: {
      id: string;
      range: CoreRange;
      text: string;
      position: { x: number; y: number };
      style?: string;
    }[];
    blurs: {
      id: string;
      range: CoreRange;
      rect: { x: number; y: number; width: number; height: number };
      strength?: number;
    }[];
    arrows: {
      id: string;
      range: CoreRange;
      from: { x: number; y: number };
      to: { x: number; y: number };
      color?: string | null;
    }[];
  };
  webcam: {
    visible: boolean;
    corner: 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';
    size: number;
    roundness: number;
    shrink_on_zoom: boolean;
    mirror: boolean;
    shadow: boolean;
    [key: string]: unknown;
  };
  captions: { range: CoreRange; text: string }[];
  voice?: { cleanup: boolean; cut_silences: boolean; silences: CoreRange[] };
  [key: string]: unknown;
}

const us = (seconds: number) => Math.max(0, Math.round(seconds * 1_000_000));
const sec = (micros: number) => micros / 1_000_000;

export function durationOf(core: CoreProject): number {
  const screens = core.tracks.filter((t) => t.kind === 'screen');
  return sec(Math.max(0, ...screens.map((t) => t.offset_us + t.duration_us)));
}

function aspectOf(value: CoreProject['canvas']['aspect']): Aspect {
  return typeof value === 'string' && ['16:9', '4:3', '1:1', '9:16', 'original'].includes(value)
    ? (value as Aspect)
    : 'original';
}

function depthOf(value: CoreProject['zoom']['depth']): Project['zoom']['depth'] {
  return value === 'subtle' || value === 'bold' ? value : 'standard';
}

const CORNERS = {
  top_left: 'tl',
  top_right: 'tr',
  bottom_left: 'bl',
  bottom_right: 'br',
} as const;

function clipsOf(core: CoreProject, duration: number): Clip[] {
  const cuts = [...core.edits.cuts].sort((a, b) => a.start_us - b.start_us);
  const clips: Clip[] = [];
  let at = 0;
  const speedAt = (t: number) =>
    core.edits.speed.find((s) => us(t) >= s.range.start_us && us(t) < s.range.end_us)?.factor ?? 1;
  const push = (start: number, end: number) => {
    if (end - start > 0.01) clips.push({ id: `c${clips.length + 1}`, start, end, speed: speedAt(start) });
  };
  for (const cut of cuts) {
    push(at, sec(cut.start_us));
    at = Math.max(at, sec(cut.end_us));
  }
  push(at, duration);
  return clips.length ? clips : [{ id: 'c1', start: 0, end: duration, speed: 1 }];
}

export function zoomFromCore(z: CoreZoom): ZoomBlock {
  return {
    id: z.id,
    start: sec(z.start_us),
    end: sec(z.end_us),
    scale: z.scale,
    focus: { x: z.focus.x, y: z.focus.y },
    reason: z.reason ?? (z.source === 'manual' ? 'Added by hand' : 'Auto zoom'),
    source: z.source ?? 'auto',
  };
}

/** The editor's model of a saved project. */
export function fromCore(core: CoreProject, os: Os): Project {
  const duration = durationOf(core);
  const effects: EffectBlock[] = [
    ...core.edits.texts.map((t) => ({
      id: t.id,
      kind: 'text' as const,
      start: sec(t.range.start_us),
      end: sec(t.range.end_us),
      label: t.text,
      pos: { ...t.position },
    })),
    ...core.edits.blurs.map((b) => ({
      id: b.id,
      kind: 'blur' as const,
      start: sec(b.range.start_us),
      end: sec(b.range.end_us),
      label: 'Blur',
      rect: { ...b.rect },
      strength: b.strength,
    })),
    ...core.edits.arrows.map((a) => ({
      id: a.id,
      kind: 'arrow' as const,
      start: sec(a.range.start_us),
      end: sec(a.range.end_us),
      label: 'Arrow',
      from: { ...a.from },
      to: { ...a.to },
    })),
  ];
  const theme = core.cursor.theme;
  const mic = core.tracks.find((t) => t.kind === 'microphone');
  const system = core.tracks.find((t) => t.kind === 'system_audio');
  const music = core.tracks.find((t) => t.kind === 'music' && !t.muted);
  return {
    id: core.id,
    title: core.name,
    kind: core.kind === 'screenshot' ? 'screenshot' : 'video',
    duration,
    capture: { width: core.source.size.width, height: core.source.size.height },
    canvas: {
      wallpaper:
        core.canvas.background.kind === 'wallpaper'
          ? (core.canvas.background.id ?? 'aurora-lime')
          : 'aurora-lime',
      padding: core.canvas.padding,
      radius: core.canvas.corner_radius,
      shadow: Math.min(1, Math.max(0, core.canvas.shadow.opacity / 0.6)),
      aspect: aspectOf(core.canvas.aspect),
    },
    cursor: {
      size: core.cursor.size,
      smoothing: core.cursor.smoothing,
      style: theme === 'minimal' ? 'dot' : theme === 'windows' ? 'windows' : theme === 'native' ? os : 'mac',
      click: core.cursor.click_effect.kind,
      snap: core.cursor.snap_to_clicks,
      sounds: core.cursor.click_sound,
      motionBlur: core.cursor.motion_blur > 0,
      hideIdle: (core.cursor.auto_hide_ms ?? 0) > 0,
      loop: core.cursor.loop_to_start,
      spotlight: core.cursor.spotlight.enabled,
    },
    zoom: {
      auto: core.zoom.auto,
      depth: depthOf(core.zoom.depth),
      motion: core.zoom.motion === 'instant' ? 'instant' : 'smooth',
      follow: core.zoom.follow_mouse,
      hold: core.zoom.tuning.hold_ms / 1000,
      mergeGap: core.zoom.tuning.merge_gap_ms / 1000,
      typing: core.zoom.tuning.typing ?? true,
      dwell: core.zoom.tuning.dwell,
    },
    clips: clipsOf(core, duration),
    zooms: core.zooms.map(zoomFromCore),
    effects,
    webcam: {
      on: core.webcam.visible && core.tracks.some((t) => t.kind === 'camera'),
      shape: core.webcam.roundness >= 0.9 ? 'circle' : core.webcam.roundness >= 0.15 ? 'rounded' : 'square',
      size: core.webcam.size,
      corner: CORNERS[core.webcam.corner] ?? 'br',
      shrink: core.webcam.shrink_on_zoom,
      mirror: core.webcam.mirror,
      shadow: core.webcam.shadow,
    },
    audio: {
      mic: mic?.volume ?? 1,
      system: system?.volume ?? 1,
      cleanup: core.voice?.cleanup ?? false,
      silence: core.voice?.cut_silences ?? false,
      silences: (core.voice?.silences ?? []).map((r) => ({ start: sec(r.start_us), end: sec(r.end_us) })),
      music: music
        ? {
            path: music.path,
            name: music.name || music.path.split('/').pop() || 'Music',
            volume: music.volume,
            start: Math.max(0, sec(music.offset_us)),
            seconds: sec(music.duration_us),
          }
        : null,
      voiceOvers: core.tracks
        .filter((t) => t.kind === 'voice_over')
        .map((t) => ({
          id: t.id,
          path: t.path,
          name: t.name || voiceOverName(t.path),
          volume: t.volume,
          start: Math.max(0, sec(t.offset_us)),
          seconds: sec(t.duration_us),
        })),
    },
    captions: { state: core.captions.length ? 'ready' : 'none', style: 'bar', show: true },
    keystrokes: core.zoom.keystrokes,
    has: { mic: !!mic, system: !!system, camera: core.tracks.some((t) => t.kind === 'camera') },
  };
}

const range = (start: number, end: number): CoreRange => ({ start_us: us(start), end_us: us(end) });

/** The saved project with the editor's changes written over `base`. */
/** Where a new blur or arrow goes before it's placed, in capture points. The preview and the export agree. */
export function effectDefaults(capture: { width: number; height: number }) {
  const { width: w, height: h } = capture;
  return {
    blur: { x: w * 0.35, y: h * 0.4, width: w * 0.3, height: h * 0.08 },
    from: { x: w * 0.75, y: h * 0.3 },
    to: { x: w * 0.6, y: h * 0.18 },
  };
}

/** A voice-over's name from its file: `media/voice-over 3.m4a` is “Voice-over 3”. */
export function voiceOverName(path: string): string {
  const n = /voice-over (\d+)\.m4a$/.exec(path)?.[1];
  return n ? `Voice-over ${n}` : 'Voice-over';
}

/** The editor's shadow strength (0 to 1) as the project's shadow opacity. */
export function shadowOpacity(strength: number): number {
  return Math.round(strength * 0.6 * 1000) / 1000;
}

export function toCore(p: Project, base: CoreProject): CoreProject {
  const core = plainCopy(base);
  core.name = p.title;
  core.canvas.aspect = p.canvas.aspect;
  core.canvas.background = { kind: 'wallpaper', id: p.canvas.wallpaper };
  core.canvas.padding = p.canvas.padding;
  core.canvas.corner_radius = p.canvas.radius;
  core.canvas.shadow = { ...core.canvas.shadow, opacity: shadowOpacity(p.canvas.shadow) };

  const theme = { mac: 'macos_classic', windows: 'windows', dot: 'minimal' } as const;
  const wasNative = core.cursor.theme === 'native';
  core.cursor.theme = wasNative && p.cursor.style !== 'dot' ? 'native' : theme[p.cursor.style];
  core.cursor.size = p.cursor.size;
  core.cursor.smoothing = p.cursor.smoothing;
  core.cursor.snap_to_clicks = p.cursor.snap;
  core.cursor.click_sound = p.cursor.sounds;
  core.cursor.motion_blur = p.cursor.motionBlur ? core.cursor.motion_blur || 0.3 : 0;
  core.cursor.auto_hide_ms = p.cursor.hideIdle ? core.cursor.auto_hide_ms || 2500 : null;
  core.cursor.loop_to_start = p.cursor.loop;
  core.cursor.spotlight = { ...core.cursor.spotlight, enabled: p.cursor.spotlight };
  core.cursor.click_effect =
    p.cursor.click === 'ripple'
      ? { kind: 'ripple', color: 'accent', size: 1 }
      : p.cursor.click === 'press'
        ? { kind: 'press' }
        : { kind: 'none' };

  core.zoom.auto = p.zoom.auto;
  core.zoom.depth = p.zoom.depth;
  core.zoom.motion = p.zoom.motion;
  core.zoom.follow_mouse = p.zoom.follow;
  core.zoom.keystrokes = p.keystrokes;
  core.zoom.tuning = {
    ...core.zoom.tuning,
    hold_ms: Math.round(p.zoom.hold * 1000),
    merge_gap_ms: Math.round(p.zoom.mergeGap * 1000),
    dwell: p.zoom.dwell,
    typing: p.zoom.typing,
  };
  const previous = new Map(base.zooms.map((z) => [z.id, z]));
  core.zooms = p.zooms.map((z) => ({
    ...(previous.get(z.id) ?? {}),
    id: z.id,
    start_us: us(z.start),
    end_us: us(z.end),
    scale: z.scale,
    focus: { x: z.focus.x, y: z.focus.y },
    source: z.source,
    reason: z.reason,
  }));

  // Clips are the parts kept; the file stores the parts removed and the speed changes.
  const clips = [...p.clips].sort((a, b) => a.start - b.start);
  const cuts: CoreRange[] = [];
  let at = 0;
  for (const clip of clips) {
    if (clip.start - at > 0.01) cuts.push(range(at, clip.start));
    at = Math.max(at, clip.end);
  }
  if (p.duration - at > 0.01) cuts.push(range(at, p.duration));
  core.edits.cuts = cuts;
  core.edits.speed = clips
    .filter((c) => c.speed !== 1)
    .map((c) => ({ range: range(c.start, c.end), factor: c.speed }));

  const spots = effectDefaults(p.capture);
  core.edits.texts = p.effects
    .filter((e) => e.kind === 'text')
    .map((e) => ({
      id: e.id,
      range: range(e.start, e.end),
      text: e.label,
      position: e.pos ?? { x: 0.05, y: 0.06 },
    }));
  core.edits.blurs = p.effects
    .filter((e) => e.kind === 'blur')
    .map((e) => ({
      id: e.id,
      range: range(e.start, e.end),
      rect: e.rect ?? spots.blur,
      strength: e.strength ?? 1,
    }));
  core.edits.arrows = p.effects
    .filter((e) => e.kind === 'arrow')
    .map((e) => ({
      id: e.id,
      range: range(e.start, e.end),
      from: e.from ?? spots.from,
      to: e.to ?? spots.to,
    }));

  core.webcam.visible = p.webcam.on;
  core.webcam.size = p.webcam.size;
  core.webcam.shrink_on_zoom = p.webcam.shrink;
  core.webcam.mirror = p.webcam.mirror;
  core.webcam.shadow = p.webcam.shadow;
  core.webcam.roundness = p.webcam.shape === 'circle' ? 1 : p.webcam.shape === 'rounded' ? 0.22 : 0.06;
  core.webcam.corner = ({ tl: 'top_left', tr: 'top_right', bl: 'bottom_left', br: 'bottom_right' } as const)[
    p.webcam.corner
  ];

  for (const track of core.tracks) {
    if (track.kind === 'microphone') track.volume = p.audio.mic;
    if (track.kind === 'system_audio') track.volume = p.audio.system;
  }
  // Music and voice-overs play on the finished video's timeline, from `start`.
  core.tracks = core.tracks.filter((t) => t.kind !== 'music' && t.kind !== 'voice_over');
  for (const voice of p.audio.voiceOvers) {
    core.tracks.push({
      id: voice.id,
      kind: 'voice_over',
      path: voice.path,
      duration_us: us(voice.seconds),
      offset_us: us(voice.start),
      volume: voice.volume,
      muted: false,
      name: voice.name,
    });
  }
  const music = p.audio.music;
  if (music) {
    core.tracks.push({
      id: 'music',
      kind: 'music',
      path: music.path,
      duration_us: us(music.seconds),
      offset_us: us(music.start),
      volume: music.volume,
      muted: false,
      name: music.name,
    });
  }
  core.voice = {
    cleanup: p.audio.cleanup,
    cut_silences: p.audio.silence,
    silences: p.audio.silences.map((s) => range(s.start, s.end)),
  };
  return core;
}

/** A deep copy. `structuredClone` can't copy Svelte state proxies; a JSON round trip can, and the file is JSON. */
function plainCopy<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
