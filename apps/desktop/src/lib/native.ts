/**
 * The bridge to the Rust side. Inside Tauri every call goes to a command; in a plain browser (design work,
 * screenshot QA) the same calls return sample answers, so every screen can be previewed without the app.
 */

import { convertFileSrc, invoke } from '@tauri-apps/api/core';

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
    kinds.map((kind) => ({ kind, state: previewGrants.has(kind) ? 'granted' : 'not_determined' })),
  );
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
    { id: 'window-3', kind: 'window', app: 'Notes', title: 'Launch checklist', width: 900, height: 700 },
    {
      id: 'window-4',
      kind: 'window',
      app: 'Terminal',
      title: 'shotmaxxing take run',
      width: 980,
      height: 620,
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
      record: 'record',
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
  kind: 'camera' | 'microphone' | 'system_audio' | 'music';
  path: string;
  /** Where it starts on the screen's timeline; negative when it started first. */
  offset_us: number;
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

export type ExportFormat = 'h264' | 'hevc' | 'gif';

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
}

export type ExportOutcome = { status: 'done'; path: string; file: string } | { status: 'cancelled' };

// Browser preview: exports run on a timer so the flow can be tried without the app.
const previewExports = new Map<string, { cancelled: boolean; listeners: Set<(p: number) => void> }>();
const progressListeners = new Set<(id: string, progress: number) => void>();

/** Exports a project with every edit applied. Resolves when the file is written or the export is cancelled. */
export function exportProject(request: ExportRequest): Promise<ExportOutcome> {
  return call('export_project', { request }, async () => {
    const job = { cancelled: false, listeners: new Set<(p: number) => void>() };
    previewExports.set(request.id, job);
    const started = performance.now();
    while (!job.cancelled) {
      const progress = Math.min(1, (performance.now() - started) / 3200);
      for (const listener of progressListeners) listener(request.id, progress);
      if (progress >= 1) break;
      await new Promise((r) => setTimeout(r, 120));
    }
    previewExports.delete(request.id);
    const ext = request.format === 'gif' ? 'gif' : 'mp4';
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
  listener: (id: string, progress: number) => void,
): Promise<() => void> {
  if (!isTauri) {
    progressListeners.add(listener);
    return () => progressListeners.delete(listener);
  }
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  return getCurrentWebviewWindow().listen<{ id: string; progress: number }>('export-progress', (event) =>
    listener(event.payload.id, event.payload.progress),
  );
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

/** Editor routes carry a project path; it is encoded so it survives as one route segment. */
export function editorTarget(path: string): string {
  return path === 'sample' ? path : encodeURIComponent(path);
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

/** Small pictures of sources, as data URLs keyed by source id. Empty in a browser preview. */
export function sourceThumbnails(ids: string[]): Promise<Record<string, string>> {
  return call('source_thumbnails', { ids }, () => ({}));
}

export function connectAgent(agent: string): Promise<{ ok: boolean; message: string }> {
  return call(
    'connect_agent',
    { agent },
    () =>
      new Promise<{ ok: boolean; message: string }>((r) =>
        setTimeout(() => r({ ok: true, message: 'Connected' }), 900),
      ),
  );
}

export function revealInFolder(path: string): Promise<void> {
  return call('reveal_in_folder', { path }, () => undefined);
}

export async function platform(): Promise<'mac' | 'windows'> {
  if (!isTauri) return document.documentElement.dataset.os === 'windows' ? 'windows' : 'mac';
  const { platform } = await import('@tauri-apps/plugin-os');
  return platform() === 'windows' ? 'windows' : 'mac';
}
