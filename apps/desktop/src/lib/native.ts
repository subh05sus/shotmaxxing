/**
 * The bridge to the Rust side. Inside Tauri every call goes to a command; in a plain browser (design work,
 * screenshot QA) the same calls return sample answers, so every screen can be previewed without the app.
 */

import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { env } from '@shotmaxxing/ui';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type PermissionKind = 'screen' | 'microphone' | 'camera' | 'accessibility' | 'input_monitoring';
export type PermissionState = 'granted' | 'denied' | 'not_determined' | 'not_needed';

export interface PermissionStatus {
  kind: PermissionKind;
  state: PermissionState;
}

export interface PowerStatus {
  low_power: boolean;
  reason: string | null;
  thermal: 'nominal' | 'fair' | 'serious' | 'critical';
  on_battery: boolean;
}

export interface CaptureSource {
  id: string;
  kind: 'display' | 'window';
  title: string;
  app?: string;
  width: number;
  height: number;
  /** A small picture of the source, as a data URL, when the platform can take one. */
  thumbnail?: string;
  /** A window Stage Manager parked in its strip: `thumbnail` is its app's icon, and it comes forward to be taken. */
  parked?: boolean;
  /** A full-screen app's window, filling a Space of its own (macOS). */
  full_screen?: boolean;
  /** On another Space: `thumbnail` is its app's icon, and the Mac goes there to take it. */
  away?: boolean;
}

export interface SourcePicture {
  /** A data URL: a picture of the source, or the app icon of a parked window or one on another Space. */
  url: string | null;
  parked: boolean;
  away: boolean;
}

/**
 * Calls a Rust command. Sample answers are used only in a plain browser; inside the app a failure is thrown, so the
 * screen can say what went wrong instead of pretending it worked.
 */
async function call<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T | Promise<T>,
): Promise<T> {
  if (!isTauri) return await fallback();
  return await invoke<T>(command, args);
}

// Browser preview: permissions flip to granted a moment after "opening settings", so the flow can be tried out.
const previewGrants = new Set<PermissionKind>(['screen', 'microphone']);

export function permissionStatus(): Promise<PermissionStatus[]> {
  const kinds: PermissionKind[] = ['screen', 'microphone', 'camera', 'accessibility', 'input_monitoring'];
  return call('permission_status', undefined, () =>
    kinds.map((kind) => ({ kind, state: previewState(kind) })),
  );
}

/** What each system reports: Windows asks nothing for the screen or for input, and never says "not yet". */
function previewState(kind: PermissionKind): PermissionState {
  if (env.os === 'windows') {
    if (kind !== 'microphone' && kind !== 'camera') return 'not_needed';
    return previewGrants.has(kind) ? 'granted' : 'denied';
  }
  return previewGrants.has(kind) ? 'granted' : 'not_determined';
}

export function openPermissionSettings(kind: PermissionKind): Promise<void> {
  return call('open_permission_settings', { kind }, () => {
    setTimeout(() => previewGrants.add(kind), 1200);
  });
}

export function powerStatus(): Promise<PowerStatus> {
  return call('power_status', undefined, () => ({
    low_power: false,
    reason: null,
    thermal: 'nominal',
    on_battery: false,
  }));
}

export function listSources(): Promise<CaptureSource[]> {
  return call('list_sources', undefined, () => [
    { id: 'display-1', kind: 'display', title: 'Built-in Retina Display', width: 3024, height: 1964 },
    { id: 'display-2', kind: 'display', title: 'Studio Display', width: 5120, height: 2880 },
    { id: 'window-1', kind: 'window', app: 'Safari', title: 'Shotmaxxing docs', width: 1440, height: 900 },
    { id: 'window-2', kind: 'window', app: 'Figma', title: 'Onboarding v3', width: 1600, height: 1000 },
    // Parked by Stage Manager, so the preview shows that state too.
    {
      id: 'window-3',
      kind: 'window',
      app: 'Notes',
      title: 'Launch checklist',
      width: 900,
      height: 700,
      parked: true,
    },
    {
      id: 'window-4',
      kind: 'window',
      app: 'Terminal',
      title: 'shotmaxxing take run',
      width: 980,
      height: 620,
    },
    // Full screen on a Space of its own, so the preview shows that state too.
    {
      id: 'window-6',
      kind: 'window',
      app: 'Google Chrome',
      title: 'Pricing · Northwind',
      width: 1470,
      height: 835,
      full_screen: true,
      away: true,
    },
    { id: 'window-5', kind: 'window', app: 'Xcode', title: 'Northwind.xcodeproj', width: 1680, height: 1050 },
  ]);
}

export type WindowKind = 'home' | 'editor' | 'record' | 'onboarding' | 'pill' | 'take';

/** Opens (or focuses) a window. In a browser preview, the current page moves to that screen instead. */
export function openWindow(kind: WindowKind, target?: string): Promise<void> {
  return call('open_window', { kind, target }, () => {
    const map: Record<WindowKind, string> = {
      home: target ?? 'home/library',
      editor: `editor/${target ?? 'sample'}`,
      record: target === 'screenshot' ? 'record?mode=screenshot' : 'record',
      onboarding: 'onboarding',
      pill: 'pill',
      take: 'take',
    };
    location.hash = `#/${map[kind]}`;
  });
}

/** Closes the window this screen is in (no-op in a browser preview). */
export async function closeThisWindow(): Promise<void> {
  if (!isTauri) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().close();
}

/** Moves from one window to another: opens the next, then closes this one. */
export async function handOff(kind: WindowKind, target?: string): Promise<void> {
  await openWindow(kind, target);
  await closeThisWindow();
}

// Recording -----------------------------------------------------------------------------------------------------

export interface StartedRecording {
  project: string;
}

export interface StoppedRecording {
  project: string;
  duration_us: number;
  zooms: number;
  dropped_frames: number;
}

/** Starts recording a source from `listSources` into a new project. */
/** Choices from Settings that shape a recording. */
export interface RecordingOptions {
  fps?: number;
  hideDesktopIcons?: boolean;
  /** Empty for the default folder. */
  folder?: string;
  /** Record what the computer plays. */
  systemAudio?: boolean;
  /** A microphone id, `default` for the system's choice, or nothing for no microphone. */
  microphone?: string;
  /** A camera id, `default`, or nothing for no camera. */
  camera?: string;
}

/** What the recording of a project wanted to say (a microphone that wasn't allowed, dropped frames), once. */
export function takeRecordingNotices(project: string): Promise<string[]> {
  return call('take_recording_notices', { project }, () => []);
}

export function startRecording(
  source: string,
  name?: string,
  options?: RecordingOptions,
): Promise<StartedRecording> {
  return call('start_recording', { source, name, options }, () => ({ project: 'sample' }));
}

/** Stops, saves the project with auto zoom applied, and says where it is. */
export function stopRecording(): Promise<StoppedRecording> {
  return call('stop_recording', undefined, () => ({
    project: 'sample',
    duration_us: 42_000_000,
    zooms: 4,
    dropped_frames: 0,
  }));
}

export function discardRecording(): Promise<void> {
  return call('discard_recording', undefined, () => undefined);
}

/** Throws away what was recorded so far and starts again on the same source. */
export function restartRecording(): Promise<StartedRecording> {
  return call('restart_recording', undefined, () => ({ project: 'sample' }));
}

// Projects ------------------------------------------------------------------------------------------------------

/** A project as saved on disk (`project.json`). The editor maps it to its own model; see `project.ts`. */
export type CoreProject = import('./project').CoreProject;
export type CoreZoom = import('./project').CoreZoom;

export interface CursorPoint {
  t_us: number;
  x: number;
  y: number;
}

export type CoreEvent =
  | { type: 'mouse_down'; t_us: number; x: number; y: number; button: string; clicks?: number }
  | { type: 'mouse_up'; t_us: number; x: number; y: number }
  | { type: 'key_down'; t_us: number; label?: string | null; redacted?: boolean }
  | { type: 'scroll'; t_us: number; x: number; y: number }
  | { type: 'focus'; t_us: number; label?: string | null; role: string }
  | { type: string; t_us: number };

export interface TrackMedia {
  kind: 'camera' | 'microphone' | 'system_audio' | 'music' | 'voice_over';
  path: string;
  /** Where it starts on the screen's timeline; negative when it started first. */
  offset_us: number;
  /** The cleaned-up copy of the microphone, once voice cleanup has made one. */
  cleaned: string | null;
}

/** A stretch of the recording, in seconds. */
export interface Span {
  start: number;
  end: number;
}

/** The long pauses in a take's voice where nothing happens on screen: what "Cut silences" takes out. */
export function findPauses(project: string): Promise<Span[]> {
  return call('find_pauses', { path: project }, () => [
    { start: 12.4, end: 14.1 },
    { start: 27.2, end: 28.9 },
  ]);
}

/** Makes the cleaned-up copy of a take's voice; returns its path. */
export function cleanVoice(project: string): Promise<string> {
  return call('clean_voice', { path: project }, () => new Promise((done) => setTimeout(() => done(''), 900)));
}

// Captions -------------------------------------------------------------------------------------------------------

/** Whether this computer can make captions, and whether the speech model still has to be downloaded first. */
export interface CaptionModel {
  /** Why captions can't be made here, when they can't. */
  unsupported: string | null;
  /** Downloaded and checked, so captions start at once. */
  ready: boolean;
  /** The model's download size, in bytes. */
  bytes: number;
}

export function captionModel(): Promise<CaptionModel> {
  return call('caption_model', undefined, () => ({ unsupported: null, ready: false, bytes: 59_707_625 }));
}

/** Where a caption job is: waiting for another project's, downloading the model, listening, or transcribing. */
export type CaptionStage = 'waiting' | 'download' | 'listen' | 'transcribe';

/** A caption made from speech, in microseconds on its voice's timeline. */
export interface MadeCaption {
  startUs: number;
  endUs: number;
  text: string;
  /** The voice-over it was heard in, by its file: timed on the finished video. */
  voiceOver?: string;
}

export type CaptionOutcome =
  { status: 'done'; captions: MadeCaption[]; language: string | null } | { status: 'cancelled' };

/**
 * Makes captions from a project's voices (its microphone and voice-overs, as saved), downloading the speech model
 * first when it isn't here yet. `rowUnits` is how many units a row of the frame holds, so each caption fits in two
 * rows. Progress comes through `onCaptionProgress`.
 */
export function generateCaptions(request: {
  id: string;
  project: string;
  language: string | null;
  rowUnits: number;
}): Promise<CaptionOutcome> {
  return call('generate_captions', { request }, () => ({ status: 'cancelled' }) as const);
}

export function cancelCaptions(id: string): Promise<void> {
  return call('cancel_captions', { id }, () => undefined);
}

/**
 * Saves a project's captions as a subtitle file where the person picks, starting in `folder` (the export folder;
 * none means next to the project). Resolves with where it went, or null when they cancelled.
 */
export function saveCaptions(
  project: string,
  name: string,
  folder: string | null,
  format: 'srt' | 'vtt',
): Promise<string | null> {
  return call('save_captions', { project, name, folder, format }, () => null);
}

/** Calls `listener` as caption jobs from this window make progress. Returns a function that stops listening. */
export async function onCaptionProgress(
  listener: (id: string, stage: CaptionStage, progress: number) => void,
): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  return getCurrentWebviewWindow().listen<{ id: string; stage: CaptionStage; progress: number }>(
    'caption-progress',
    (event) => listener(event.payload.id, event.payload.stage, event.payload.progress),
  );
}

export interface LoadedProject {
  root: string;
  project: CoreProject;
  media: string | null;
  tracks: TrackMedia[];
  cursor: CursorPoint[];
  events: CoreEvent[];
}

export interface ProjectSummary {
  path: string;
  name: string;
  kind: 'video' | 'screenshot';
  duration_us: number;
  created_at_ms: number;
  updated_at_ms: number;
  app: string | null;
  agent: string | null;
  zooms: number;
  width: number;
  height: number;
  media: string | null;
  size_bytes: number;
}

// Screenshots ------------------------------------------------------------------------------------------------

/** A screenshot's look in the project's own units: what the next screenshot starts from. */
export interface StillLook {
  wallpaper: string;
  padding: number;
  cornerRadius: number;
  shadowOpacity: number;
}

/**
 * Takes a screenshot of `source` and saves it as a new project, starting from `look` when there is one. Resolves
 * with the project's folder.
 */
export function takeScreenshot(source: string, folder?: string, look?: StillLook | null): Promise<string> {
  return call(
    'take_screenshot',
    { source, folder: folder || null, look: look ?? null },
    () => 'sample-still',
  );
}

/** Copies a screenshot project to the clipboard, styled as the editor shows it, pixel for pixel. */
export function copyScreenshot(path: string): Promise<void> {
  return call('copy_screenshot', { path }, () => undefined);
}

/** Calls `listener` when the open record dialog is asked for a recording or a screenshot. */
export async function onRecordMode(listener: (mode: 'video' | 'screenshot') => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  return getCurrentWebviewWindow().listen<'video' | 'screenshot'>('record-mode', (event) =>
    listener(event.payload),
  );
}

export function loadProject(path: string): Promise<LoadedProject> {
  return call('load_project', { path }, () => {
    throw new Error('Real projects open in the app.');
  });
}

/** The drawn cursor path for these settings, the same path the export draws. */
export function cursorPath(
  path: string,
  settings: { smoothing: number; snapToClicks: boolean; loopToStart: boolean },
): Promise<CursorPoint[]> {
  return call('cursor_path', { path, ...settings }, () => [] as CursorPoint[]);
}

/** Saves the editor's changes; with a label, also keeps a version in the project's history. */
export function saveProject(path: string, project: CoreProject, label?: string): Promise<void> {
  return call('save_project', { path, project, label }, () => undefined);
}

/** Every project in the projects folder, newest first. Empty in a browser preview, which shows samples instead. */
export function listProjects(folder?: string): Promise<ProjectSummary[]> {
  return call('list_projects', { folder: folder || null }, () => []);
}

/** The default projects folder, the short way (`~/Movies/Shotmaxxing`). */
export function defaultProjectsFolder(fallback: string): Promise<string> {
  return call('default_projects_folder', undefined, () => fallback);
}

/** Asks where to save recordings. Resolves to the folder the short way, or null if cancelled. */
export function chooseProjectsFolder(current?: string): Promise<string | null> {
  return call('choose_projects_folder', { current: current || null }, () => null);
}

/** `png` is one frame, styled like the video. */
export type ExportFormat = 'h264' | 'hevc' | 'gif' | 'png';

export interface ExportRequest {
  id: string;
  /** The `.smx` project folder. */
  project: string;
  /** The file name without its extension. */
  name: string;
  width: number;
  height: number;
  fps: number;
  format: ExportFormat;
  /** Bits per second. */
  bitrate: number;
  /** Where to save, the short way; next to the project when empty. */
  folder?: string;
  /** For a PNG: the moment of the recording to show, in seconds. */
  at?: number;
}

export type ExportOutcome = { status: 'done'; path: string; file: string } | { status: 'cancelled' };

// Browser preview: exports run on a timer so the flow can be tried without the app.
const previewExports = new Map<string, { cancelled: boolean; listeners: Set<(p: number) => void> }>();
const progressListeners = new Set<(id: string, progress: number, waiting: boolean) => void>();

/** Exports a project with every edit applied. Resolves when the file is written or the export is cancelled. */
export function exportProject(request: ExportRequest): Promise<ExportOutcome> {
  return call('export_project', { request }, async () => {
    const job = { cancelled: false, listeners: new Set<(p: number) => void>() };
    previewExports.set(request.id, job);
    const started = performance.now();
    while (!job.cancelled) {
      const progress = Math.min(1, (performance.now() - started) / 3200);
      for (const listener of progressListeners) listener(request.id, progress, false);
      if (progress >= 1) break;
      await new Promise((r) => setTimeout(r, 120));
    }
    previewExports.delete(request.id);
    const ext = request.format === 'gif' ? 'gif' : request.format === 'png' ? 'png' : 'mp4';
    return job.cancelled
      ? { status: 'cancelled' }
      : {
          status: 'done',
          path: `~/Movies/Shotmaxxing/${request.name}.${ext}`,
          file: `${request.name}.${ext}`,
        };
  });
}

export function cancelExport(id: string): Promise<void> {
  return call('cancel_export', { id }, () => {
    const job = previewExports.get(id);
    if (job) job.cancelled = true;
  });
}

/** Calls `listener` as exports from this window make progress. Returns a function that stops listening. */
export async function onExportProgress(
  listener: (id: string, progress: number, waiting: boolean) => void,
): Promise<() => void> {
  if (!isTauri) {
    progressListeners.add(listener);
    return () => progressListeners.delete(listener);
  }
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  return getCurrentWebviewWindow().listen<{ id: string; progress: number; waiting?: boolean }>(
    'export-progress',
    (event) => listener(event.payload.id, event.payload.progress, event.payload.waiting ?? false),
  );
}

// Agent takes ---------------------------------------------------------------------------------------------------

/** Who connected, as the system reports it, never as the agent describes itself. */
export interface TakeCaller {
  pid: number;
  /** The program that connected: `shotmaxxing` when it's the command. */
  program: string;
  path: string;
  /** The program that started it: the agent, e.g. `claude`. */
  agent: string | null;
  /** The app it all runs in, e.g. "Terminal". */
  app: string | null;
  /** Who signed that app, when its signature checks out: "Apple", or the developer's name. */
  signer: string | null;
  /** Whether the signature was checked at all (macOS only, so far). */
  checked: boolean;
}

/**
 * What the take card shows. `access`: an agent asks to see and record the screen for its session. `take`: an
 * agent's take, from asking the person through to the saved project.
 */
export interface TakeCardState {
  id: number;
  kind: 'access' | 'take';
  phase: 'asking' | 'running' | 'paused' | 'waiting' | 'done' | 'stopped';
  /** The agent's name for itself. */
  client: string;
  caller: TakeCaller | null;
  app: string;
  /** What an agent asking for access wants to do first, e.g. "look at TextEdit". */
  purpose: string;
  beats: string[];
  /** The beat in progress, counting from 0; `beats.length` while the take is being saved. */
  beat: number;
  done: number;
  seconds: number;
  /** What the person was asked to do, while the take waits for them. */
  waiting: string | null;
  /** The hold the person asked for, from the moment they ask until it's lifted. It starts after the current step. */
  hold: 'pause' | 'take_over' | null;
  result: {
    beats_done: number;
    stopped: string | null;
    project: string | null;
    video: string | null;
    seconds: number;
  } | null;
}

export function takeCard(): Promise<TakeCardState | null> {
  return call('take_card', undefined, () => null);
}

/** The person's answer to what the card asks. Every take asks; access lasts until the agent disconnects. */
export function answerTake(id: number, allow: boolean): Promise<void> {
  return call('answer_take', { id, allow }, () => undefined);
}

export function stopTake(id: number): Promise<void> {
  return call('stop_take', { id }, () => undefined);
}

/** Pauses the take once the step in progress ends. */
export function pauseTake(id: number): Promise<void> {
  return call('pause_take', { id }, () => undefined);
}

export function resumeTake(id: number): Promise<void> {
  return call('resume_take', { id }, () => undefined);
}

/** Hands control to the person once the step in progress ends, until they hand back. */
export function takeOverTake(id: number): Promise<void> {
  return call('take_over_take', { id }, () => undefined);
}

export function handBackTake(id: number): Promise<void> {
  return call('hand_back_take', { id }, () => undefined);
}

export function dismissTake(id: number): Promise<void> {
  return call('dismiss_take', { id }, () => undefined);
}

/** Calls `listener` whenever the take card changes. Returns a function that stops listening. */
export async function onTakeCard(listener: (card: TakeCardState) => void): Promise<() => void> {
  if (!isTauri) return () => {};
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  return getCurrentWebviewWindow().listen<TakeCardState>('take-card', (event) => listener(event.payload));
}

export function chooseExportFolder(current?: string): Promise<string | null> {
  return call('choose_export_folder', { current: current || null }, () => null);
}

export function setMenuBarIcon(visible: boolean): Promise<void> {
  return call('set_menu_bar_icon', { visible }, () => undefined);
}

/** Whether the app opens at login, as the system has it. */
export function launchAtLogin(): Promise<boolean> {
  return call('launch_at_login', undefined, () => false);
}

export function setLaunchAtLogin(enabled: boolean): Promise<void> {
  return call('set_launch_at_login', { enabled }, () => undefined);
}

/** Opens a web page in the default browser. */
export async function openLink(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  await invoke('open_link', { url });
}

export function renameProject(path: string, name: string): Promise<void> {
  return call('rename_project', { path, name }, () => undefined);
}

/** Copies a project next to the original and returns the copy's path. */
export function duplicateProject(path: string): Promise<string> {
  return call('duplicate_project', { path }, () => path);
}

/** Moves a project to the Trash (the Recycle Bin on Windows). */
export function trashProject(path: string): Promise<void> {
  return call('trash_project', { path }, () => undefined);
}

export function planAutoZooms(path: string, depth: string): Promise<CoreZoom[]> {
  return call('plan_auto_zooms', { path, depth }, () => []);
}

/** A URL the web view can stream a local file from. */
export async function mediaUrl(path: string): Promise<string> {
  return convertFileSrc(path);
}

// Voice-overs ----------------------------------------------------------------------------------------------------

/** A finished voice-over: its file in the project folder and how long it plays. */
export interface VoiceOverFile {
  path: string;
  seconds: number;
}

// The browser preview pretends to record, so the flow can be tried without a microphone.
let previewVoiceStarted = 0;
let previewVoiceCount = 0;

/**
 * Starts recording a voice-over into `project` from `microphone` (the default when empty). Resolves once the
 * microphone is sending sound, so playback can start in step with it.
 */
export function startVoiceOver(project: string, microphone?: string): Promise<void> {
  return call('start_voice_over', { project, microphone: microphone || null }, async () => {
    await new Promise((r) => setTimeout(r, 300));
    previewVoiceStarted = performance.now();
  });
}

/** Stops the voice-over and finishes its file. */
export function stopVoiceOver(): Promise<VoiceOverFile> {
  return call('stop_voice_over', undefined, () => ({
    path: `media/voice-over ${++previewVoiceCount}.m4a`,
    seconds: (performance.now() - previewVoiceStarted) / 1000,
  }));
}

/** Stops the voice-over and throws it away. */
export function discardVoiceOver(): Promise<void> {
  return call('discard_voice_over', undefined, () => undefined);
}

/** Music the person picked, copied into the project. */
export interface MusicFile {
  /** The copy, relative to the project folder. */
  path: string;
  /** The file's own name, to show. */
  name: string;
  seconds: number;
}

/** Asks for a music file and copies it into the project's media folder. `null` when the person cancels. */
export function chooseMusic(project: string): Promise<MusicFile | null> {
  return call('choose_music', { project }, () => SAMPLE_MUSIC);
}

/** What the browser preview and the sample project "pick". */
export const SAMPLE_MUSIC: MusicFile = { path: 'media/music.mp3', name: 'Sunrise drive.mp3', seconds: 154 };

/** Editor routes carry a project path; it is encoded so it survives as one route segment. */
export function editorTarget(path: string): string {
  return path === 'sample' || path === 'sample-still' ? path : encodeURIComponent(path);
}

export interface AgentStatus {
  id: string;
  installed: boolean;
  connected: boolean;
}

/** Which agents are installed here and which already have Shotmaxxing connected. */
export function detectAgents(): Promise<AgentStatus[]> {
  return call('detect_agents', undefined, () => []);
}

export interface Device {
  id: string;
  name: string;
  is_default: boolean;
}

/** Microphones and cameras, the system default first. A browser preview lists sample devices. */
export function listDevices(): Promise<{ microphones: Device[]; cameras: Device[] }> {
  return call('list_devices', undefined, () => ({
    microphones: [
      { id: 'mic-1', name: 'MacBook Pro Microphone', is_default: true },
      { id: 'mic-2', name: 'Studio Display Microphone', is_default: false },
    ],
    cameras: [{ id: 'cam-1', name: 'FaceTime HD Camera', is_default: true }],
  }));
}

/** Small pictures of sources, keyed by source id. Empty in a browser preview. */
export function sourceThumbnails(ids: string[]): Promise<Record<string, SourcePicture>> {
  return call('source_thumbnails', { ids }, () => ({}));
}

export interface ConnectResult {
  ok: boolean;
  message: string;
  /** When the settings have to be changed by hand: the file, and the lines to add. */
  manual?: { path: string; snippet: string };
}

/** Adds Shotmaxxing to an agent's MCP settings. */
export function connectAgent(agent: string): Promise<ConnectResult> {
  return call(
    'connect_agent',
    { agent },
    () => new Promise<ConnectResult>((r) => setTimeout(() => r({ ok: true, message: 'Connected' }), 900)),
  );
}

/** Takes Shotmaxxing out of an agent's MCP settings. */
export function disconnectAgent(agent: string): Promise<ConnectResult> {
  return call('disconnect_agent', { agent }, () => ({ ok: true, message: 'Disconnected' }));
}

/** The command agents run to reach Shotmaxxing, for setting one up by hand. */
export function mcpCommand(): Promise<string> {
  return call('mcp_command', undefined, () => 'shotmaxxing');
}

export function revealInFolder(path: string): Promise<void> {
  return call('reveal_in_folder', { path }, () => undefined);
}

export async function platform(): Promise<'mac' | 'windows'> {
  if (!isTauri) return document.documentElement.dataset.os === 'windows' ? 'windows' : 'mac';
  const { platform } = await import('@tauri-apps/plugin-os');
  return platform() === 'windows' ? 'windows' : 'mac';
}
