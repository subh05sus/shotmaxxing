/**
 * The bridge to the Rust side. Inside Tauri every call goes to a command; in a plain browser (design work,
 * screenshot QA) the same calls return sample answers, so every screen can be previewed without the app.
 */

import { invoke } from '@tauri-apps/api/core';

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
