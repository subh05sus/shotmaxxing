/**
 * Editor state for one project window: the project, playback, selection, undo history, autosave and versions.
 * Every edit goes through `change()`, which records undo history (coalescing quick repeats such as a slider drag)
 * and schedules an autosave.
 */

import { env, toast } from '@shotmaxxing/ui';
import {
  SAMPLE_CURSOR,
  SAMPLE_KEYS,
  SAMPLE_PROJECT,
  type EffectBlock,
  type ZoomBlock,
} from '../../lib/sample';
import { cursorPath, loadProject, mediaUrl, planAutoZooms, saveProject } from '../../lib/native';
import { fromCore, toCore, zoomFromCore, type CoreProject } from '../../lib/project';

export interface TimedPoint {
  t: number;
  x: number;
  y: number;
}

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
    /** Flip the camera like a mirror, the way people see themselves in calls. */
    mirror: boolean;
    shadow: boolean;
  };
  audio: { mic: number; system: number; cleanup: boolean; silence: boolean };
  captions: { state: 'none' | 'busy' | 'ready'; style: 'bar' | 'bold'; show: boolean };
  keystrokes: boolean;
  /** Which tracks the take has, so panels only offer what exists. */
  has: { mic: boolean; system: boolean; camera: boolean };
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
  /** When it started, to tell how long is left. */
  startedAt: number;
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
    webcam: { on: false, shape: 'circle', size: 0.2, corner: 'br', shrink: true, mirror: true, shadow: true },
    audio: { mic: 0.9, system: 0.6, cleanup: true, silence: false },
    captions: { state: 'none', style: 'bar', show: true },
    keystrokes: true,
    has: { mic: true, system: true, camera: true },
  };
}

const DEPTH_SCALE = { subtle: 1.45, standard: 1.9, bold: 2.6 };

export class EditorState {
  project = $state<Project>(sampleProject());
  /** Where the project lives on disk; `null` for the built-in sample. */
  path = $state<string | null>(null);
  status = $state<'ready' | 'loading' | 'error'>('ready');
  error = $state('');
  saveError = $state('');
  /** The screen recording, for real projects. */
  media = $state<string | null>(null);
  /** The camera recording, for takes filmed with one. The offset places it on the screen's timeline. */
  camera = $state<{ url: string; offset: number } | null>(null);
  /** What was recorded besides the video, in seconds and capture points. */
  cursorPath = $state.raw<TimedPoint[]>(SAMPLE_CURSOR);
  clicks = $state.raw<TimedPoint[]>(SAMPLE_PROJECT.clicks);
  keys = $state.raw<{ t: number; label: string }[]>(SAMPLE_KEYS);
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
  #core: CoreProject | null = null;
  #video: HTMLVideoElement | null = null;
  #cameraVideo: HTMLVideoElement | null = null;
  /** The recorded sound, each on the screen's timeline. */
  #sounds: { kind: string; el: HTMLAudioElement; offset: number }[] = [];

  constructor() {
    const json = JSON.stringify(this.project);
    const now = Date.now();
    this.versions = [
      { id: 'v-import', label: 'Recorded by Claude Code', at: now - 2 * 3600_000, json },
      { id: 'v-auto', label: 'Auto zoom added 3 zooms', at: now - 2 * 3600_000 + 4000, json },
    ];
  }

  /** Opens a project from disk. The sample stays in place until it loads, so a failure can say what went wrong. */
  async load(path: string): Promise<void> {
    this.status = 'loading';
    try {
      const loaded = await loadProject(path);
      this.#core = loaded.project;
      this.path = loaded.root;
      this.project = fromCore(loaded.project, env.os === 'windows' ? 'windows' : 'mac');
      this.media = loaded.media ? await mediaUrl(loaded.media) : null;
      this.#releaseSounds();
      this.camera = null;
      for (const track of loaded.tracks) {
        const url = await mediaUrl(track.path);
        const offset = track.offset_us / 1e6;
        if (track.kind === 'camera') {
          this.camera = { url, offset };
        } else {
          const el = new Audio();
          el.preload = 'auto';
          el.src = url;
          this.#sounds.push({ kind: track.kind, el, offset });
        }
      }
      this.cursorPath = loaded.cursor.map((c) => ({ t: c.t_us / 1e6, x: c.x, y: c.y }));
      this.#cursorKey = '';
      const clicks: TimedPoint[] = [];
      const keys: { t: number; label: string }[] = [];
      for (const event of loaded.events) {
        if (event.type === 'mouse_down' && 'x' in event) {
          clicks.push({ t: event.t_us / 1e6, x: event.x, y: event.y });
        } else if (event.type === 'key_down' && 'label' in event && event.label) {
          keys.push({ t: event.t_us / 1e6, label: event.label });
        }
      }
      this.clicks = clicks;
      this.keys = keys;
      this.time = 0;
      this.selection = null;
      this.#past = [];
      this.#future = [];
      this.canUndo = false;
      this.canRedo = false;
      this.versions = [{ id: 'v-open', label: 'Opened', at: Date.now(), json: JSON.stringify(this.project) }];
      this.status = 'ready';
    } catch (error) {
      this.error = error instanceof Error ? error.message : String(error);
      this.status = 'error';
    }
  }

  /** The editor's <video>, when a real recording is shown. Playback follows its clock. */
  attachVideo(video: HTMLVideoElement | null): void {
    this.#video = video;
    if (video) video.currentTime = this.time;
  }

  /** The camera bubble's <video>. It follows the screen like the sound does. */
  attachCamera(video: HTMLVideoElement | null): void {
    this.#cameraVideo = video;
    this.#syncMedia(this.time);
  }

  /** Keeps a sound or the camera at `t` on the screen's timeline, playing or paused along with it. */
  #follow(el: HTMLMediaElement, offset: number, t: number, speed: number): void {
    const at = t - offset;
    const length = Number.isFinite(el.duration) ? el.duration : Infinity;
    if (at < 0 || at >= length) {
      // Before it starts or after it ends: hold the nearest frame, silently.
      if (!el.paused) el.pause();
      const edge = Math.max(0, Math.min(at, Number.isFinite(length) ? length : 0));
      if (Math.abs(el.currentTime - edge) > 0.05) el.currentTime = edge;
      return;
    }
    if (el.playbackRate !== speed) el.playbackRate = speed;
    // Small drift is left alone while playing: seeking a playing sound is audible.
    if (Math.abs(el.currentTime - at) > (this.playing ? 0.15 : 0.02)) el.currentTime = at;
    if (this.playing && el.paused) el.play().catch(() => {});
    else if (!this.playing && !el.paused) el.pause();
  }

  #syncMedia(t: number): void {
    const speed = this.rate * (this.clipAt(t)?.speed ?? 1);
    for (const sound of this.#sounds) {
      const level =
        sound.kind === 'microphone'
          ? this.project.audio.mic
          : sound.kind === 'system_audio'
            ? this.project.audio.system
            : 1;
      // Browsers stop at 100%; the export applies louder settings in full.
      sound.el.volume = Math.min(1, Math.max(0, level));
      this.#follow(sound.el, sound.offset, t, speed);
    }
    if (this.#cameraVideo && this.camera) this.#follow(this.#cameraVideo, this.camera.offset, t, speed);
  }

  #releaseSounds(): void {
    for (const sound of this.#sounds) {
      sound.el.pause();
      sound.el.removeAttribute('src');
      sound.el.load();
    }
    this.#sounds = [];
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
    this.#saveTimer = setTimeout(() => void this.#save(), 700);
    // A version is kept after a quiet minute of editing, so history stays readable.
    clearTimeout(this.#versionTimer);
    this.#versionTimer = setTimeout(() => this.snapshot(`Edited: ${label.toLowerCase()}`), 60_000);
  }

  async #save(label?: string): Promise<void> {
    try {
      if (this.path && this.#core) {
        this.#core = toCore(this.project, this.#core);
        await saveProject(this.path, this.#core, label);
      } else {
        localStorage.setItem(`smx.project.${this.project.id}`, JSON.stringify(this.project));
      }
      this.saveError = '';
      this.savedAt = Date.now();
    } catch (error) {
      // Sample previews may have no storage; real projects say why saving failed.
      if (this.path) this.saveError = error instanceof Error ? error.message : String(error);
    } finally {
      this.saving = false;
    }
  }

  snapshot(label: string): void {
    if (this.path) void this.#save(label);
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
    const video = this.#video;
    if (video) {
      video.currentTime = this.time;
      video.playbackRate = this.rate * (this.clipAt(this.time)?.speed ?? 1);
      video.play().catch(() => this.pause());
    }
    this.#syncMedia(this.time);
    const frame = (now: number) => {
      if (!this.playing) return;
      const dt = (now - this.#last) / 1000;
      if (env.lowPower && dt < 1 / 31) {
        this.#raf = requestAnimationFrame(frame);
        return;
      }
      this.#last = now;
      let t: number;
      if (video) {
        // The video is the clock; the frame loop only follows it and applies speed changes.
        t = video.currentTime;
        const speed = this.rate * (this.clipAt(t)?.speed ?? 1);
        if (video.playbackRate !== speed) video.playbackRate = speed;
      } else {
        t = this.time + dt * this.rate * (this.clipAt(this.time)?.speed ?? 1);
      }
      // Skip removed parts.
      if (!this.clipAt(t)) {
        const next = this.project.clips
          .filter((c) => c.start >= t - 0.001)
          .sort((a, b) => a.start - b.start)[0];
        t = next ? next.start : this.project.duration;
        if (video && next) video.currentTime = t;
      }
      if (t >= this.project.duration || t <= 0) {
        this.time = Math.min(this.project.duration, Math.max(0, t));
        this.pause();
        return;
      }
      this.time = t;
      this.#syncMedia(t);
      this.#raf = requestAnimationFrame(frame);
    };
    this.#raf = requestAnimationFrame(frame);
  }

  pause(): void {
    this.playing = false;
    cancelAnimationFrame(this.#raf);
    this.#video?.pause();
    this.#syncMedia(this.time);
  }

  toggle(): void {
    if (this.playing) this.pause();
    else this.play();
  }

  seek(t: number): void {
    this.time = Math.min(this.project.duration, Math.max(0, t));
    const video = this.#video;
    if (video && Math.abs(video.currentTime - this.time) > 0.02) video.currentTime = this.time;
    this.#syncMedia(this.time);
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
  async redoAutoZoom(): Promise<void> {
    if (this.path) {
      let planned: Awaited<ReturnType<typeof planAutoZooms>>;
      try {
        planned = await planAutoZooms(this.path, this.project.zoom.depth);
      } catch (error) {
        toast(`Couldn’t plan auto zoom again: ${error instanceof Error ? error.message : String(error)}`, {
          tone: 'danger',
        });
        return;
      }
      this.change('Redo auto zoom', () => {
        const manual = this.project.zooms.filter((z) => z.source === 'manual');
        this.project.zooms = [...planned.map(zoomFromCore), ...manual];
      });
      return;
    }
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
    const job: ExportJob = { id: `x${Date.now()}`, name, progress: 0, done: false, startedAt: Date.now() };
    this.exports.push(job);
    return this.exports[this.exports.length - 1];
  }

  #cursorKey = '';
  #cursorRequest = 0;

  /**
   * Redraws the cursor path when smoothing, snapping or looping change, so the preview shows the path the export
   * will draw. Cheap to call: it only asks the app when one of those settings actually changed.
   */
  async refreshCursorPath(): Promise<void> {
    const c = this.project.cursor;
    const key = `${c.smoothing}|${c.snap}|${c.loop}`;
    if (!this.path || key === this.#cursorKey) return;
    const first = this.#cursorKey === '';
    this.#cursorKey = key;
    // The path loaded with the project already uses its settings.
    if (first) return;
    const request = ++this.#cursorRequest;
    try {
      const path = await cursorPath(this.path, {
        smoothing: c.smoothing,
        snapToClicks: c.snap,
        loopToStart: c.loop,
      });
      // A newer change may have finished first; only the latest one counts.
      if (request === this.#cursorRequest)
        this.cursorPath = path.map((p) => ({ t: p.t_us / 1e6, x: p.x, y: p.y }));
    } catch {
      // The old path stays; the export still uses the new settings.
    }
  }

  /** Saves any pending change now, so an export sees exactly what's on screen. */
  async flush(): Promise<void> {
    if (!this.saving) return;
    clearTimeout(this.#saveTimer);
    await this.#save();
  }

  destroy(): void {
    this.pause();
    this.#releaseSounds();
    clearTimeout(this.#saveTimer);
    clearTimeout(this.#versionTimer);
  }
}
