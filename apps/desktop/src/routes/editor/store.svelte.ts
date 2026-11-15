/**
 * Editor state for one project window: the project, playback, selection, undo history, autosave and versions.
 * Every edit goes through `change()`, which records undo history (coalescing quick repeats such as a slider drag)
 * and schedules an autosave.
 */

import { env, toast } from '@shotmaxxing/ui';
import { timecode } from '../../lib/format';
import {
  SAMPLE_CAPTIONS,
  SAMPLE_CURSOR,
  SAMPLE_KEYS,
  SAMPLE_PROJECT,
  type EffectBlock,
  type ZoomBlock,
} from '../../lib/sample';
import {
  SAMPLE_MUSIC,
  cancelCaptions,
  captionModel,
  chooseMusic,
  cleanVoice,
  cursorPath,
  discardVoiceOver,
  findPauses,
  generateCaptions,
  isTauri,
  loadProject,
  mediaUrl,
  onCaptionProgress,
  openPermissionSettings,
  planAutoZooms,
  revealInFolder,
  saveCaptions,
  saveProject,
  startVoiceOver,
  stopVoiceOver,
  type CaptionModel,
  type CaptionStage,
} from '../../lib/native';
import { captionAt, rowUnits, shownCaptions as captionsShown, type CaptionStyle } from '../../lib/captions';
import { settings } from '../../lib/settings.svelte';
import { dropRanges, keepRanges } from './clips';
import {
  effectDefaults,
  fromCore,
  toCore,
  voiceOverName,
  zoomFromCore,
  type CoreProject,
} from '../../lib/project';
import { clickSamples } from './click';

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

/**
 * A caption: what was said, and when, in seconds on the recording's timeline (so it follows cuts and speed changes,
 * like titles); a voice-over's caption is on the finished video's timeline instead, like the voice-over.
 */
export interface CaptionLine {
  id: string;
  start: number;
  end: number;
  text: string;
  /** The voice-over it transcribes, by its file. */
  voiceOver?: string;
}

/** Narration recorded in the editor, laid on the finished video's timeline. */
export interface VoiceOver {
  id: string;
  /** The file, in the project folder. */
  path: string;
  name: string;
  /** 1 is as recorded. */
  volume: number;
  /** Where it starts on the finished video, in seconds. */
  start: number;
  seconds: number;
}

export interface Project {
  id: string;
  title: string;
  /** A recording, or a screenshot: one picture, where nothing plays and every effect shows. */
  kind: 'video' | 'screenshot';
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
  audio: {
    mic: number;
    system: number;
    /** Play and export the cleaned-up copy of the voice. */
    cleanup: boolean;
    /** Long pauses where nothing happens are cut. */
    silence: boolean;
    /** The pauses that were cut, in seconds of the recording, so turning it off brings exactly those back. */
    silences: { start: number; end: number }[];
    /**
     * Music under the video. It plays on the finished video's timeline from `start` (seconds), untouched by cuts
     * and speed changes, and fades out with the video when it runs past the end. `path` is in the project folder.
     */
    music: { path: string; name: string; volume: number; start: number; seconds: number } | null;
    /**
     * Narration recorded in the editor. Each plays on the finished video's timeline from `start` (seconds), like
     * music, so cuts and speed changes made later don't move it. `path` is in the project folder.
     */
    voiceOvers: VoiceOver[];
  };
  captions: {
    /** What was said and when, made from the voice on this computer and fixed by hand. */
    lines: CaptionLine[];
    style: CaptionStyle;
    /** Shown in the editor and drawn into the export. */
    show: boolean;
    /** Whisper's code for the language they were made in ("en"), when known. */
    language: string | null;
  };
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
  /** When it started rendering, to tell how long is left. */
  startedAt: number;
  /** Waiting for an earlier export to finish: one renders at a time. */
  waiting?: boolean;
}

function sampleProject(): Project {
  return {
    id: SAMPLE_PROJECT.id,
    title: SAMPLE_PROJECT.title,
    kind: 'video',
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
    audio: {
      mic: 0.9,
      system: 0.6,
      cleanup: true,
      silence: false,
      silences: [],
      music: null,
      voiceOvers: [],
    },
    captions: { lines: [], style: 'bar', show: true, language: null },
    keystrokes: true,
    has: { mic: true, system: true, camera: true },
  };
}

const DEPTH_SCALE = { subtle: 1.45, standard: 1.9, bold: 2.6 };

/** A recorded sound in the preview, on the screen's timeline. */
interface Sound {
  kind: string;
  el: HTMLAudioElement;
  offset: number;
  raw: string;
  /** The microphone's cleaned-up copy, once made. */
  clean: string | null;
  /** Set up the first time this sound is turned past 100%, which only Web Audio can play. */
  gain: { source: MediaElementAudioSourceNode; node: GainNode } | null;
}

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
  /** A voice tool at work: making the cleaned-up voice, or finding pauses. */
  voiceBusy = $state<'cleanup' | 'silence' | null>(null);
  /** A voice-over being recorded: where on the finished video it starts. Everything else is muted meanwhile. */
  voiceOver = $state<{ start: number } | null>(null);
  /** A voice-over is starting or finishing. */
  voiceOverBusy = $state(false);
  /** Captions being made: where the job is, and how far through that stage (0 to 1). */
  captionJob = $state<{ id: string; stage: CaptionStage | 'starting'; progress: number } | null>(null);
  /** Whether captions can be made here, and whether the speech model is downloaded; null until asked. */
  captionModel = $state<CaptionModel | null>(null);
  /** The captions the video shows, in the order the export picks from. */
  shownCaptions = $derived(
    captionsShown(
      this.project.captions.lines,
      this.project.audio.voiceOvers.map((v) => v.path),
    ),
  );
  /** Players for the voice-overs, by id, and the file each is for. */
  #voices = new Map<string, { el: HTMLAudioElement; file: string }>();
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
  /** The recorded sound, each on the screen's timeline. The microphone may also have a cleaned-up copy. */
  #sounds: Sound[] = [];
  /** The music's player, and the file it's for. It follows the project, so undo and redo bring it back. */
  #music: HTMLAudioElement | null = null;
  #musicFile: string | null = null;

  constructor() {
    const json = JSON.stringify(this.project);
    const now = Date.now();
    this.versions = [
      { id: 'v-import', label: 'Recorded by Claude Code', at: now - 2 * 3600_000, json },
      { id: 'v-auto', label: 'Auto zoom added 3 zooms', at: now - 2 * 3600_000 + 4000, json },
    ];
  }

  /** A screenshot: nothing plays, there's no timeline, and every title, arrow and blur shows. */
  get still(): boolean {
    return this.project.kind === 'screenshot';
  }

  /** The built-in sample as a screenshot, to try the still editor in a browser. */
  useStillSample(): void {
    const sample = sampleProject();
    this.project = {
      ...sample,
      kind: 'screenshot',
      title: 'Screenshot',
      duration: 0,
      canvas: { ...sample.canvas, aspect: 'original' },
      clips: [],
      zooms: [],
      effects: sample.effects.map((e) => ({ ...e, start: 0, end: 1 })),
      has: { mic: false, system: false, camera: false },
    };
    this.cursorPath = [];
    this.clicks = [];
    this.keys = [];
    this.time = 0;
    this.versions = [{ id: 'v-open', label: 'Taken', at: Date.now(), json: JSON.stringify(this.project) }];
  }

  /** The whole frame's size in capture points, as the preview lays it out: how far a title moves per point. */
  framePoints = { width: 0, height: 0 };

  /** Moves the selected title, arrow or blur by `dx`, `dy` capture points. It stays inside the frame. */
  nudge(dx: number, dy: number): boolean {
    const sel = this.selection;
    const effect = sel?.kind === 'effect' ? this.project.effects.find((e) => e.id === sel.id) : undefined;
    if (!effect) return false;
    const { width, height } = this.project.capture;
    const spots = effectDefaults(this.project.capture);
    const within = (v: number, max: number) => Math.min(max, Math.max(0, v));
    this.change(`Move ${effect.label}`, () => {
      if (effect.kind === 'text') {
        // Titles sit on the frame, in fractions of it.
        const pos = effect.pos ?? { x: 0.05, y: 0.06 };
        const fw = this.framePoints.width || width;
        const fh = this.framePoints.height || height;
        effect.pos = { x: within(pos.x + dx / fw, 0.95), y: within(pos.y + dy / fh, 0.95) };
      } else if (effect.kind === 'arrow') {
        const from = effect.from ?? spots.from;
        const to = effect.to ?? spots.to;
        effect.from = { x: within(from.x + dx, width), y: within(from.y + dy, height) };
        effect.to = { x: within(to.x + dx, width), y: within(to.y + dy, height) };
      } else {
        const rect = effect.rect ?? spots.blur;
        effect.rect = {
          ...rect,
          x: within(rect.x + dx, width - rect.width),
          y: within(rect.y + dy, height - rect.height),
        };
      }
    });
    return true;
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
        } else if (track.kind === 'music' || track.kind === 'voice_over') {
          // Music and voice-overs follow the project's audio settings instead; see #syncMusic and #syncVoices.
          continue;
        } else {
          const el = new Audio();
          el.preload = 'auto';
          // Asked for with CORS, which the app's file server answers, so Web Audio may raise its level.
          el.crossOrigin = 'anonymous';
          el.src = url;
          const clean = track.cleaned ? await mediaUrl(track.cleaned) : null;
          this.#sounds.push({ kind: track.kind, el, offset, raw: url, clean, gain: null });
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
    // While a voice-over records, the video plays silently so nothing echoes into the microphone.
    const monitor = this.voiceOver ? 0 : 1;
    for (const sound of this.#sounds) {
      // The microphone plays from its cleaned-up copy while cleanup is on, as the export will.
      const source =
        sound.kind === 'microphone' && this.project.audio.cleanup && sound.clean ? sound.clean : sound.raw;
      if (sound.el.getAttribute('src') !== source) sound.el.src = source;
      const level =
        sound.kind === 'microphone'
          ? this.project.audio.mic
          : sound.kind === 'system_audio'
            ? this.project.audio.system
            : 1;
      this.#setLevel(sound, Math.max(0, level) * monitor);
      this.#follow(sound.el, sound.offset, t, speed);
    }
    if (this.#cameraVideo && this.camera) this.#follow(this.#cameraVideo, this.camera.offset, t, speed);
    this.#syncMusic(t, monitor);
    this.#syncVoices(t, monitor);
  }

  /** Voice-overs play on the finished video's timeline, like music: through cuts, ignoring speed changes. */
  #syncVoices(t: number, monitor: number): void {
    const voices = this.path ? this.project.audio.voiceOvers : [];
    for (const [id, player] of this.#voices) {
      const voice = voices.find((v) => v.id === id);
      if (!voice || `${this.path}/${voice.path}` !== player.file) {
        this.#voices.delete(id);
        player.el.pause();
        player.el.removeAttribute('src');
        player.el.load();
      }
    }
    const out = this.outputTimeAt(t);
    for (const voice of voices) {
      const file = `${this.path}/${voice.path}`;
      const player = this.#voices.get(voice.id);
      if (!player) {
        const el = new Audio();
        el.preload = 'auto';
        this.#voices.set(voice.id, { el, file });
        void mediaUrl(file).then((url) => {
          if (this.#voices.get(voice.id)?.el !== el) return;
          el.src = url;
          this.#syncMedia(this.time);
        });
        continue;
      }
      if (!player.el.getAttribute('src')) continue;
      // An element plays at most at full volume; above 100% is for the export.
      player.el.volume = Math.min(1, Math.max(0, voice.volume)) * monitor;
      if (out < voice.start - 0.01 || out > voice.start + voice.seconds) {
        if (!player.el.paused) player.el.pause();
        continue;
      }
      this.#follow(player.el, voice.start, out, this.rate);
    }
  }

  #releaseVoices(): void {
    for (const { el } of this.#voices.values()) {
      el.pause();
      el.removeAttribute('src');
      el.load();
    }
    this.#voices.clear();
  }

  /**
   * Records a voice-over from the playhead: the video plays, silently, while the person talks, and the take lands
   * on the finished video's timeline where it started. Pausing, the end of the video or Stop finishes it.
   */
  async recordVoiceOver(microphone?: string): Promise<void> {
    if (this.voiceOver || this.voiceOverBusy || this.still) return;
    if (isTauri && !this.path) {
      toast('Open one of your recordings to add a voice-over to it.');
      return;
    }
    this.voiceOverBusy = true;
    try {
      this.pause();
      if (this.time >= this.project.duration - 0.05) this.seek(0);
      this.rate = 1;
      await startVoiceOver(this.path ?? '', microphone);
      this.voiceOver = { start: this.outputTimeAt(this.time) };
      this.voiceOverBusy = false;
      this.play();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast(`Couldn’t record a voice-over: ${message}`, {
        tone: 'danger',
        timeout: 10000,
        action: /› Microphone|Privacy/.test(message)
          ? { label: 'Open settings', run: () => openPermissionSettings('microphone') }
          : undefined,
      });
    } finally {
      this.voiceOverBusy = false;
    }
  }

  /** Finishes the voice-over being recorded and adds it, or with `keep` false throws it away. */
  async finishVoiceOver(keep = true): Promise<void> {
    const recording = this.voiceOver;
    if (!recording || this.voiceOverBusy) return;
    this.voiceOverBusy = true;
    this.voiceOver = null;
    if (this.playing) this.pause();
    try {
      if (!keep) {
        await discardVoiceOver();
        toast('Voice-over thrown away.');
        return;
      }
      const file = await stopVoiceOver();
      const p = this.project;
      const voice: VoiceOver = {
        id: `voice-over-${Date.now()}`,
        path: file.path,
        name: voiceOverName(file.path),
        volume: 1,
        start: recording.start,
        seconds: file.seconds,
      };
      this.change('Record voice-over', () => p.audio.voiceOvers.push(voice));
      this.#syncMedia(this.time);
      toast(`${voice.name} added: ${Math.max(1, Math.round(file.seconds))} s from ${timecode(voice.start)}`, {
        tone: 'success',
        action: { label: 'Undo', run: () => this.undo() },
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast(`Couldn’t save the voice-over: ${message}`, { tone: 'danger', timeout: 10000 });
    } finally {
      this.voiceOverBusy = false;
    }
  }

  removeVoiceOver(id: string): void {
    const voices = this.project.audio.voiceOvers;
    const voice = voices.find((v) => v.id === id);
    if (!voice) return;
    this.change(`Remove ${voice.name}`, () => voices.splice(voices.indexOf(voice), 1));
    this.#syncMedia(this.time);
  }

  /** Music plays on the finished video's timeline: it keeps going across cuts and ignores speed changes. */
  #syncMusic(t: number, monitor = 1): void {
    const music = this.project.audio.music;
    const file = music && this.path ? `${this.path}/${music.path}` : null;
    if (file !== this.#musicFile) {
      this.#musicFile = file;
      this.#releaseMusic();
      if (file) {
        void mediaUrl(file).then((url) => {
          if (this.#musicFile !== file) return;
          const el = new Audio();
          el.preload = 'auto';
          el.src = url;
          this.#music = el;
          this.#syncMedia(this.time);
        });
      }
      return;
    }
    const el = this.#music;
    if (!el || !music) return;
    const out = this.outputTimeAt(t);
    const total = this.outputDuration;
    // Music longer than the video fades out over its end, as the export does.
    const fade = music.start + music.seconds > total + 1e-3 ? Math.min(2, (total - music.start) / 2) : 0;
    const level = fade > 0 ? Math.min(1, Math.max(0, (total - out) / fade)) : 1;
    el.volume = Math.min(1, Math.max(0, music.volume * level)) * monitor;
    this.#follow(el, music.start, out, this.rate);
  }

  #releaseMusic(): void {
    const el = this.#music;
    this.#music = null;
    if (!el) return;
    el.pause();
    el.removeAttribute('src');
    el.load();
  }

  /** Asks for a music file, copies it into the project and lays it under the whole video. */
  async addMusic(): Promise<void> {
    let file;
    try {
      // The sample has no folder to copy into; it shows a typical song.
      file = this.path ? await chooseMusic(this.path) : SAMPLE_MUSIC;
    } catch (error) {
      toast(`Couldn’t add the music: ${error instanceof Error ? error.message : String(error)}`, {
        tone: 'danger',
        timeout: 8000,
      });
      return;
    }
    if (!file) return;
    const p = this.project;
    // Under a voice, music sits well back; on its own, it carries the video.
    const music = {
      path: file.path,
      name: file.name,
      volume: p.has.mic || p.audio.voiceOvers.length ? 0.3 : 0.8,
      start: 0,
      seconds: file.seconds,
    };
    this.change(p.audio.music ? 'Change music' : 'Add music', () => (p.audio.music = music));
    this.#syncMedia(this.time);
  }

  removeMusic(): void {
    this.change('Remove music', () => (this.project.audio.music = null));
    this.#syncMedia(this.time);
  }

  /** Turns voice cleanup on or off. The cleaned-up copy is made the first time, on this computer. */
  async setCleanup(on: boolean): Promise<void> {
    const mic = this.#sounds.find((s) => s.kind === 'microphone');
    if (on && this.path && mic && !mic.clean) {
      this.voiceBusy = 'cleanup';
      try {
        mic.clean = await mediaUrl(await cleanVoice(this.path));
      } catch (error) {
        toast(`Couldn’t clean up the voice: ${error instanceof Error ? error.message : String(error)}`, {
          tone: 'danger',
          timeout: 8000,
        });
        return;
      } finally {
        this.voiceBusy = null;
      }
    }
    this.change(on ? 'Voice cleanup' : 'Original voice', () => (this.project.audio.cleanup = on));
    this.#syncMedia(this.time);
  }

  /** Cuts the long pauses where nothing happens on screen, or brings exactly those back. */
  async setCutSilences(on: boolean): Promise<void> {
    const p = this.project;
    if (!on) {
      this.change('Bring back pauses', () => {
        p.clips = keepRanges(p.clips, p.audio.silences);
        p.audio.silence = false;
        p.audio.silences = [];
      });
      return;
    }
    this.voiceBusy = 'silence';
    let found: { start: number; end: number }[];
    try {
      // The sample has no recording to listen to; it shows two typical pauses.
      found = this.path
        ? await findPauses(this.path)
        : [
            { start: 12.4, end: 14.1 },
            { start: 27.2, end: 28.9 },
          ];
    } catch (error) {
      toast(`Couldn’t look for pauses: ${error instanceof Error ? error.message : String(error)}`, {
        tone: 'danger',
        timeout: 8000,
      });
      return;
    } finally {
      this.voiceBusy = null;
    }
    if (!found.length) {
      toast('No long pauses to cut. Every pause over a second has something happening on screen.');
      return;
    }
    this.change('Cut silences', () => {
      p.clips = dropRanges(p.clips, found);
      p.audio.silence = true;
      p.audio.silences = found;
    });
    const seconds = found.reduce((sum, s) => sum + s.end - s.start, 0);
    toast(`Cut ${found.length} ${found.length === 1 ? 'pause' : 'pauses'}, ${seconds.toFixed(1)} s in all.`, {
      action: { label: 'Undo', run: () => this.undo() },
    });
  }

  /** Asks whether captions can be made here and whether the speech model has been downloaded. */
  async checkCaptionModel(): Promise<void> {
    try {
      this.captionModel = await captionModel();
    } catch {
      this.captionModel = null;
    }
  }

  /** The caption at the playhead and how far through its time it is (0 to 1), as the export picks it. */
  get captionNow(): { line: CaptionLine; fraction: number } | null {
    const lines = this.shownCaptions;
    if (!lines.length || this.still) return null;
    const out = lines.some((l) => l.voiceOver) ? this.exportTimeAt(this.time) : null;
    return captionAt(lines, this.time, out);
  }

  /** Where a caption starts on the recording's timeline: a voice-over's is on the finished video's. */
  captionStart(line: CaptionLine): number {
    return line.voiceOver ? this.sourceTimeAt(line.start) : line.start;
  }

  /**
   * Makes captions from the voice (the microphone and any voice-overs) on this computer, downloading the speech
   * model the first time. They replace any made before, in one step that undo takes back.
   */
  async generateCaptions(): Promise<void> {
    if (this.captionJob) return;
    const id = `captions-${Date.now()}`;
    this.captionJob = { id, stage: 'starting', progress: 0 };
    const going = () => this.captionJob?.id === id;
    let stop: (() => void) | null = null;
    let made: CaptionLine[];
    let language: string | null;
    try {
      if (!this.path) {
        // The sample has no voice to listen to: it goes through the stages, then shows typical lines.
        const stages: [CaptionStage, number][] = [
          ['download', 1200],
          ['listen', 400],
          ['transcribe', 1600],
        ];
        for (const [stage, ms] of stages) {
          const started = performance.now();
          for (let at = 0; at < ms; at = performance.now() - started) {
            if (!going()) return;
            this.captionJob = { id, stage, progress: at / ms };
            await new Promise((done) => setTimeout(done, 100));
          }
        }
        made = SAMPLE_CAPTIONS.map((c, i) => ({ id: `k${i + 1}`, ...c }));
        language = 'en';
      } else {
        // The voices are read from the saved project, so a voice-over just recorded is saved first.
        await this.flush();
        stop = await onCaptionProgress((job, stage, progress) => {
          if (job === id && going()) this.captionJob = { id, stage, progress };
        });
        const outcome = await generateCaptions({
          id,
          project: this.path,
          language: null,
          rowUnits: rowUnits(this.#frameAspect, this.project.captions.style),
        });
        if (outcome.status === 'cancelled' || !going()) return;
        const stamp = Date.now().toString(36);
        made = outcome.captions.map((c, i) => ({
          id: `k${stamp}-${i}`,
          start: c.startUs / 1e6,
          end: c.endUs / 1e6,
          text: c.text,
          ...(c.voiceOver ? { voiceOver: c.voiceOver } : {}),
        }));
        language = outcome.language;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast(`Couldn’t make captions: ${message}`, { tone: 'danger', timeout: 10000 });
      return;
    } finally {
      stop?.();
      if (going()) this.captionJob = null;
      // The model is downloaded now, unless that's what failed.
      void this.checkCaptionModel();
    }
    if (!made.length) {
      toast('No words were heard. Is the voice silent, or muted at the source?', { timeout: 8000 });
      return;
    }
    const again = this.project.captions.lines.length > 0;
    this.change(again ? 'Make captions again' : 'Make captions', () => {
      const captions = this.project.captions;
      captions.lines = made;
      captions.language = language;
      captions.show = true;
    });
    toast(
      `Captions ready: ${made.length} ${made.length === 1 ? 'line' : 'lines'}. Fix any word in the list.`,
      {
        tone: 'success',
        action: { label: 'Undo', run: () => this.undo() },
      },
    );
  }

  /** Stops making captions. Nothing changes in the project. */
  cancelCaptions(): void {
    const job = this.captionJob;
    if (!job) return;
    this.captionJob = null;
    if (this.path) void cancelCaptions(job.id);
  }

  /**
   * Changes a caption's words. Clearing them all keeps the caption as it was until its box is left, which removes it
   * (`removeCaption`), so undo brings it back in one step.
   */
  editCaption(id: string, text: string): void {
    const line = this.project.captions.lines.find((l) => l.id === id);
    if (!line || line.text === text || !text.trim()) return;
    this.change('Edit caption', () => (line.text = text));
  }

  removeCaption(id: string): void {
    const lines = this.project.captions.lines;
    const index = lines.findIndex((l) => l.id === id);
    if (index < 0) return;
    this.change('Remove caption', () => lines.splice(index, 1));
  }

  removeCaptions(): void {
    if (!this.project.captions.lines.length) return;
    this.change('Remove captions', () => (this.project.captions.lines = []));
    toast('Captions removed.', { action: { label: 'Undo', run: () => this.undo() } });
  }

  /** Saves the captions as a subtitle file, timed to the finished video, where the person picks. */
  async saveSubtitles(format: 'srt' | 'vtt'): Promise<void> {
    if (!this.path) {
      toast('The sample has no folder to save into. Open a recording to save its subtitles.');
      return;
    }
    try {
      // The file is made from the saved project.
      await this.flush();
      const saved = await saveCaptions(this.path, this.project.title, settings.exportFolder || null, format);
      if (!saved) return;
      toast(`Saved ${saved.split(/[\\/]/).pop()}`, {
        tone: 'success',
        action: { label: 'Show', run: () => void revealInFolder(saved) },
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast(`Couldn’t save the subtitles: ${message}`, { tone: 'danger', timeout: 8000 });
    }
  }

  /** The finished frame's shape (width ÷ height), as the preview lays it out. */
  get #frameAspect(): number {
    const { width, height } = this.framePoints;
    return width > 0 && height > 0 ? width / height : 16 / 9;
  }

  /**
   * Plays a sound at `level`, where 1 is as recorded. Media elements stop at 100%, so louder settings (the sliders
   * go to 150%, as the export does) pass through a gain node, made once for that sound and kept after.
   */
  #setLevel(sound: Sound, level: number): void {
    if (level > 1 && !sound.gain) {
      try {
        this.#audio ??= new AudioContext();
        const source = this.#audio.createMediaElementSource(sound.el);
        const node = this.#audio.createGain();
        source.connect(node).connect(this.#audio.destination);
        sound.gain = { source, node };
        if (this.playing && this.#audio.state === 'suspended') void this.#audio.resume();
      } catch {
        // No Web Audio here: the preview stops at 100%, and the export still applies the full level.
      }
    }
    sound.el.volume = Math.min(1, level);
    if (sound.gain) sound.gain.node.gain.value = Math.max(1, level);
  }

  #releaseSounds(): void {
    for (const sound of this.#sounds) {
      sound.el.pause();
      sound.el.removeAttribute('src');
      sound.el.load();
      sound.gain?.source.disconnect();
      sound.gain?.node.disconnect();
    }
    this.#sounds = [];
    this.#releaseMusic();
    this.#musicFile = null;
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
    this.#syncMedia(this.time);
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
    this.#syncMedia(this.time);
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

  #audio: AudioContext | null = null;
  #clickSound: AudioBuffer | null = null;

  /** Plays the click sound now, as the export lays it under each press. */
  #playClick(): void {
    if (this.voiceOver) return;
    try {
      this.#audio ??= new AudioContext();
      const audio = this.#audio;
      if (audio.state === 'suspended') void audio.resume();
      if (!this.#clickSound) {
        const samples = clickSamples(audio.sampleRate);
        this.#clickSound = audio.createBuffer(1, samples.length, audio.sampleRate);
        this.#clickSound.copyToChannel(samples, 0);
      }
      const source = audio.createBufferSource();
      source.buffer = this.#clickSound;
      source.connect(audio.destination);
      source.start();
    } catch {
      // No sound output here: the preview stays quiet, and the export is unaffected.
    }
  }

  // Playback runs a frame loop only while playing. Low power caps it at 30 fps.
  play(): void {
    if (this.playing || this.still) return;
    if (this.time >= this.project.duration - 0.05) this.time = 0;
    this.playing = true;
    // A suspended audio context mutes the sounds routed through it.
    if (this.#audio?.state === 'suspended' && this.#sounds.some((s) => s.gain)) void this.#audio.resume();
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
      // Presses crossed since the last frame click, as they will in the export. Jumps over cuts stay quiet.
      if (this.project.cursor.sounds && t > this.time && t - this.time < 0.25) {
        for (const c of this.clicks) if (c.t > this.time && c.t <= t && this.clipAt(c.t)) this.#playClick();
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
    // A running audio context keeps the sound hardware awake, so it rests between plays.
    if (this.#audio?.state === 'running') void this.#audio.suspend();
    this.#video?.pause();
    this.#syncMedia(this.time);
    // A voice-over ends when playback does: paused, or at the end of the video.
    if (this.voiceOver) void this.finishVoiceOver();
  }

  toggle(): void {
    if (this.playing) this.pause();
    else this.play();
  }

  seek(t: number): void {
    if (this.still) return;
    // A voice-over follows the video as it plays; jumping elsewhere finishes it first.
    if (this.voiceOver) void this.finishVoiceOver();
    this.time = Math.min(this.project.duration, Math.max(0, t));
    const video = this.#video;
    if (video && Math.abs(video.currentTime - this.time) > 0.02) video.currentTime = this.time;
    this.#syncMedia(this.time);
  }

  clipAt(t: number): Clip | undefined {
    return this.project.clips.find((c) => t >= c.start && t < c.end);
  }

  /** Where moment `t` of the recording lands in the finished video, after cuts and speed changes. */
  outputTimeAt(t: number): number {
    let out = 0;
    for (const c of this.project.clips) if (t > c.start) out += (Math.min(t, c.end) - c.start) / c.speed;
    return out;
  }

  /** Where moment `t` of the recording plays in the finished video, or null when it was cut. */
  exportTimeAt(t: number): number | null {
    let out = 0;
    for (const c of this.project.clips) {
      if (t >= c.start && t < c.end) return out + (t - c.start) / c.speed;
      out += (c.end - c.start) / c.speed;
    }
    return null;
  }

  /** The moment of the recording that plays `out` seconds into the finished video. */
  sourceTimeAt(out: number): number {
    let at = 0;
    for (const c of this.project.clips) {
      const length = (c.end - c.start) / c.speed;
      if (out < at + length) return c.start + Math.max(0, out - at) * c.speed;
      at += length;
    }
    return this.project.clips.at(-1)?.end ?? 0;
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

  #replanTimer: ReturnType<typeof setTimeout> | undefined;
  #replanRequest = 0;

  /** Turns auto zoom on (planning zooms again) or off (keeping only zooms added or changed by hand). */
  async setAutoZoom(on: boolean): Promise<void> {
    if (on) {
      this.change('Auto zoom', () => (this.project.zoom.auto = true));
      await this.redoAutoZoom('Auto zoom');
      return;
    }
    this.#replanRequest++;
    this.change('Auto zoom off', () => {
      this.project.zoom.auto = false;
      this.project.zooms = this.project.zooms.filter((z) => z.source === 'manual');
    });
  }

  /** Plans auto zooms again once a zoom setting stops changing, in the same undo step as the setting. */
  replanSoon(label: string): void {
    clearTimeout(this.#replanTimer);
    this.#replanTimer = setTimeout(() => void this.redoAutoZoom(label), 250);
  }

  /** Re-plans auto zooms with the current settings, keeping zooms added by hand. Does nothing while it's off. */
  async redoAutoZoom(label = 'Redo auto zoom'): Promise<void> {
    if (!this.project.zoom.auto) return;
    const request = ++this.#replanRequest;
    if (this.path) {
      let planned: Awaited<ReturnType<typeof planAutoZooms>>;
      try {
        // The planner reads the settings from the project file, so changes still waiting to be saved go first.
        await this.flush();
        planned = await planAutoZooms(this.path, this.project.zoom.depth);
      } catch (error) {
        toast(`Couldn’t plan auto zoom again: ${error instanceof Error ? error.message : String(error)}`, {
          tone: 'danger',
        });
        return;
      }
      // A newer change started another plan, or turned auto zoom off.
      if (request !== this.#replanRequest || !this.project.zoom.auto) return;
      this.change(label, () => {
        const manual = this.project.zooms.filter((z) => z.source === 'manual');
        this.project.zooms = [...planned.map(zoomFromCore), ...manual];
      });
      return;
    }
    const scale = DEPTH_SCALE[this.project.zoom.depth];
    this.change(label, () => {
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
        start: this.still ? 0 : t,
        // A still shows every effect; its range only keeps the project's shape.
        end: this.still ? 1 : Math.min(this.project.duration, t + 3),
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
    // Closing mid-voice-over throws it away; the app does too, if this doesn't get the chance.
    if (this.voiceOver) {
      this.voiceOver = null;
      void discardVoiceOver();
    }
    this.pause();
    this.#releaseSounds();
    this.#releaseVoices();
    clearTimeout(this.#replanTimer);
    clearTimeout(this.#saveTimer);
    void this.#audio?.close();
    this.#audio = null;
    this.#clickSound = null;
    clearTimeout(this.#versionTimer);
  }
}
