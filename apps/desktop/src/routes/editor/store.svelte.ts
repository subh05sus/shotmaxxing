/**
 * Editor state for one project window: the project, playback, selection, undo history, autosave and versions.
 * Every edit goes through `change()`, which records undo history (coalescing quick repeats such as a slider drag)
 * and schedules an autosave.
 */

import { env } from '@shotmaxxing/ui';
import { SAMPLE_PROJECT, type EffectBlock, type ZoomBlock } from '../../lib/sample';

export type Aspect = '16:9' | '4:3' | '1:1' | '9:16' | 'original';
export type ToolId = 'background' | 'cursor' | 'zoom' | 'annotate' | 'camera' | 'audio' | 'ai';

export interface Clip {
  id: string;
  start: number;
  end: number;
  speed: number;
}

export interface Project {
  id: string;
  title: string;
  duration: number;
  capture: { width: number; height: number };
  canvas: { wallpaper: string; padding: number; radius: number; shadow: number; aspect: Aspect };
  cursor: {
    size: number;
    smoothing: number;
    style: 'mac' | 'windows' | 'dot';
    click: 'ripple' | 'press' | 'none';
    snap: boolean;
    sounds: boolean;
    motionBlur: boolean;
    hideIdle: boolean;
    loop: boolean;
    spotlight: boolean;
  };
  zoom: {
    auto: boolean;
    depth: 'subtle' | 'standard' | 'bold';
    motion: 'smooth' | 'instant';
    follow: boolean;
    hold: number;
    mergeGap: number;
    typing: boolean;
    dwell: boolean;
  };
  clips: Clip[];
  zooms: ZoomBlock[];
  effects: EffectBlock[];
  webcam: {
    on: boolean;
    shape: 'circle' | 'rounded' | 'square';
    size: number;
    corner: 'tl' | 'tr' | 'bl' | 'br';
    shrink: boolean;
  };
  audio: { mic: number; system: number; cleanup: boolean; silence: boolean };
  captions: { state: 'none' | 'busy' | 'ready'; style: 'bar' | 'bold'; show: boolean };
  keystrokes: boolean;
}

export type Selection = { kind: 'zoom' | 'effect' | 'clip'; id: string } | null;

export interface Version {
  id: string;
  label: string;
  at: number;
  json: string;
}

export interface ExportJob {
  id: string;
  name: string;
  progress: number;
  done: boolean;
}

function sampleProject(): Project {
  return {
    id: SAMPLE_PROJECT.id,
    title: SAMPLE_PROJECT.title,
    duration: SAMPLE_PROJECT.duration,
    capture: { ...SAMPLE_PROJECT.capture },
    canvas: { wallpaper: 'aurora-lime', padding: 0.08, radius: 14, shadow: 0.6, aspect: '16:9' },
    cursor: {
      size: 1.6,
      smoothing: 0.6,
      style: env.os === 'windows' ? 'windows' : 'mac',
      click: 'ripple',
      snap: true,
      sounds: false,
      motionBlur: true,
      hideIdle: true,
      loop: false,
      spotlight: false,
    },
    zoom: {
      auto: true,
      depth: 'standard',
      motion: 'smooth',
      follow: false,
      hold: 1.1,
      mergeGap: 1.4,
      typing: true,
      dwell: true,
    },
    clips: [{ id: 'c1', start: 0, end: SAMPLE_PROJECT.duration, speed: 1 }],
    zooms: SAMPLE_PROJECT.zooms.map((z) => ({ ...z, focus: { ...z.focus } })),
    effects: SAMPLE_PROJECT.effects.map((e) => ({ ...e })),
    webcam: { on: false, shape: 'circle', size: 0.2, corner: 'br', shrink: true },
    audio: { mic: 0.9, system: 0.6, cleanup: true, silence: false },
    captions: { state: 'none', style: 'bar', show: true },
    keystrokes: true,
  };
}

const DEPTH_SCALE = { subtle: 1.45, standard: 1.9, bold: 2.6 };

export class EditorState {
  project = $state<Project>(sampleProject());
  time = $state(10.6);
  playing = $state(false);
  rate = $state(1);
  selection = $state<Selection>(null);
  tool = $state<ToolId>('background');
  savedAt = $state(Date.now());
  saving = $state(false);
  canUndo = $state(false);
  canRedo = $state(false);
  versions = $state<Version[]>([]);
  exports = $state<ExportJob[]>([]);

  #past: string[] = [];
  #future: string[] = [];
  #lastLabel = '';
  #lastAt = 0;
  #saveTimer: ReturnType<typeof setTimeout> | undefined;
  #versionTimer: ReturnType<typeof setTimeout> | undefined;
  #raf = 0;
  #last = 0;

  constructor() {
    const json = JSON.stringify(this.project);
    const now = Date.now();
    this.versions = [
      { id: 'v-import', label: 'Recorded by Claude Code', at: now - 2 * 3600_000, json },
      { id: 'v-auto', label: 'Auto zoom added 3 zooms', at: now - 2 * 3600_000 + 4000, json },
    ];
  }

  /** Applies an edit with undo history. Repeats of the same edit within 800 ms count as one step. */
  change(label: string, apply: () => void): void {
    const now = performance.now();
    if (label !== this.#lastLabel || now - this.#lastAt > 800) {
      this.#past.push(JSON.stringify(this.project));
      if (this.#past.length > 200) this.#past.shift();
      this.#future = [];
    }
    this.#lastLabel = label;
    this.#lastAt = now;
    apply();
    this.canUndo = this.#past.length > 0;
    this.canRedo = false;
    this.#scheduleSave(label);
  }

  undo(): void {
    const previous = this.#past.pop();
    if (!previous) return;
    this.#future.push(JSON.stringify(this.project));
    this.project = JSON.parse(previous);
    this.#lastLabel = '';
    this.canUndo = this.#past.length > 0;
    this.canRedo = true;
    this.#scheduleSave('Undo');
  }

  redo(): void {
    const next = this.#future.pop();
    if (!next) return;
    this.#past.push(JSON.stringify(this.project));
    this.project = JSON.parse(next);
    this.#lastLabel = '';
    this.canUndo = true;
    this.canRedo = this.#future.length > 0;
    this.#scheduleSave('Redo');
  }

  #scheduleSave(label: string): void {
    this.saving = true;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      try {
        localStorage.setItem(`smx.project.${this.project.id}`, JSON.stringify(this.project));
      } catch {
        /* preview only */
      }
      this.saving = false;
      this.savedAt = Date.now();
    }, 700);
    // A version is kept after a quiet minute of editing, so history stays readable.
    clearTimeout(this.#versionTimer);
    this.#versionTimer = setTimeout(() => this.snapshot(`Edited: ${label.toLowerCase()}`), 60_000);
  }

  snapshot(label: string): void {
    this.versions = [
      { id: `v-${Date.now()}`, label, at: Date.now(), json: JSON.stringify(this.project) },
      ...this.versions,
    ].slice(0, 100);
  }

  restore(version: Version): void {
    this.snapshot('Before restoring an earlier version');
    this.change(`Restore ${version.label}`, () => {
      this.project = JSON.parse(version.json);
    });
  }

  // Playback runs a frame loop only while playing. Low power caps it at 30 fps.
  play(): void {
    if (this.playing) return;
    if (this.time >= this.project.duration - 0.05) this.time = 0;
    this.playing = true;
    this.#last = performance.now();
    const frame = (now: number) => {
      if (!this.playing) return;
      const dt = (now - this.#last) / 1000;
      if (env.lowPower && dt < 1 / 31) {
        this.#raf = requestAnimationFrame(frame);
        return;
      }
      this.#last = now;
      const clip = this.clipAt(this.time);
      let t = this.time + dt * this.rate * (clip?.speed ?? 1);
      // Skip removed parts.
      if (!this.clipAt(t)) {
        const next = this.project.clips
          .filter((c) => c.start >= t - 0.001)
          .sort((a, b) => a.start - b.start)[0];
        t = next ? next.start : this.project.duration;
      }
      if (t >= this.project.duration || t <= 0) {
        this.time = Math.min(this.project.duration, Math.max(0, t));
        this.pause();
        return;
      }
      this.time = t;
      this.#raf = requestAnimationFrame(frame);
    };
    this.#raf = requestAnimationFrame(frame);
  }

  pause(): void {
    this.playing = false;
    cancelAnimationFrame(this.#raf);
  }

  toggle(): void {
    if (this.playing) this.pause();
    else this.play();
  }

  seek(t: number): void {
    this.time = Math.min(this.project.duration, Math.max(0, t));
  }

  clipAt(t: number): Clip | undefined {
    return this.project.clips.find((c) => t >= c.start && t < c.end);
  }

  /** Length after cuts and speed changes. */
  get outputDuration(): number {
    return this.project.clips.reduce((sum, c) => sum + (c.end - c.start) / c.speed, 0);
  }

  split(): void {
    const t = this.time;
    const clip = this.clipAt(t);
    if (!clip || t - clip.start < 0.2 || clip.end - t < 0.2) return;
    this.change('Split', () => {
      const index = this.project.clips.findIndex((c) => c.id === clip.id);
      const right: Clip = { id: `c${Date.now()}`, start: t, end: clip.end, speed: clip.speed };
      this.project.clips[index].end = t;
      this.project.clips.splice(index + 1, 0, right);
      this.selection = { kind: 'clip', id: right.id };
    });
  }

  addZoom(): void {
    const t = this.time;
    const scale = DEPTH_SCALE[this.project.zoom.depth];
    this.change('Add zoom', () => {
      const z: ZoomBlock = {
        id: `z${Date.now()}`,
        start: Math.max(0, t - 0.3),
        end: Math.min(this.project.duration, t + 2.7),
        scale,
        focus: { x: this.project.capture.width / 2, y: this.project.capture.height / 2 },
        reason: 'Added by hand',
        source: 'manual',
      };
      this.project.zooms.push(z);
      this.selection = { kind: 'zoom', id: z.id };
    });
  }

  deleteSelection(): string | null {
    const sel = this.selection;
    if (!sel) return null;
    let label: string | null = null;
    this.change('Delete', () => {
      if (sel.kind === 'zoom') {
        this.project.zooms = this.project.zooms.filter((z) => z.id !== sel.id);
        label = 'Zoom deleted';
      } else if (sel.kind === 'effect') {
        this.project.effects = this.project.effects.filter((e) => e.id !== sel.id);
        label = 'Effect deleted';
      } else if (this.project.clips.length > 1) {
        this.project.clips = this.project.clips.filter((c) => c.id !== sel.id);
        label = 'Part removed';
      }
    });
    this.selection = null;
    return label;
  }

  /** Re-plans auto zooms at the chosen depth, keeping zooms added by hand. */
  redoAutoZoom(): void {
    const scale = DEPTH_SCALE[this.project.zoom.depth];
    this.change('Redo auto zoom', () => {
      const manual = this.project.zooms.filter((z) => z.source === 'manual');
      const auto = SAMPLE_PROJECT.zooms
        .filter((z) => z.source === 'auto')
        .filter((z) => this.project.zoom.typing || !z.reason.startsWith('Typing'))
        .map((z) => ({ ...z, focus: { ...z.focus }, scale: Math.max(1.2, scale * (z.scale / 1.9)) }));
      this.project.zooms = [...auto, ...manual];
    });
  }

  addEffect(kind: EffectBlock['kind']): void {
    const t = this.time;
    const labels = { text: 'Title', blur: 'Blur', arrow: 'Arrow' };
    this.change(`Add ${kind}`, () => {
      const e: EffectBlock = {
        id: `e${Date.now()}`,
        kind,
        start: t,
        end: Math.min(this.project.duration, t + 3),
        label: labels[kind],
      };
      this.project.effects.push(e);
      this.selection = { kind: 'effect', id: e.id };
    });
  }

  startExport(name: string): ExportJob {
    const job: ExportJob = { id: `x${Date.now()}`, name, progress: 0, done: false };
    this.exports.push(job);
    return this.exports[this.exports.length - 1];
  }

  destroy(): void {
    this.pause();
    clearTimeout(this.#saveTimer);
    clearTimeout(this.#versionTimer);
  }
}
